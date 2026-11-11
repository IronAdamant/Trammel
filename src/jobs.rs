//! Background verify runs for the MCP server: `verify_step` with
//! `background: true` returns a job id at once, `job_status` polls it and
//! `cancel_job` stops it. The shape follows Chisel's `start_job` /
//! `job_status` / `cancel_job`.
//!
//! Jobs live in the server process. A CLI call exits as soon as it answers,
//! so it cannot start one; `enable` is called only by `trammel mcp`.

use crate::harness::{self, VerifyRequest};
use crate::json::Value;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Jobs that may run at once, across all projects.
pub const MAX_RUNNING: usize = 4;
/// Finished jobs kept for `job_status`.
const KEEP_FINISHED: usize = 32;
/// The longest `job_status` waits before answering.
pub const MAX_WAIT_S: f64 = 600.0;

struct Job {
    id: String,
    started: Instant,
    timeout_s: u64,
    cancel: Arc<AtomicBool>,
    result: Option<Value>,
}

static SERVER: AtomicBool = AtomicBool::new(false);

fn jobs() -> &'static Mutex<Vec<Job>> {
    static JOBS: OnceLock<Mutex<Vec<Job>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Called by the MCP server: from now on jobs can outlive the call.
pub fn enable() {
    SERVER.store(true, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    SERVER.load(Ordering::Relaxed)
}

/// Start `request` on its own thread. Err(reason) when too many run.
pub fn start(mut request: VerifyRequest) -> Result<Value, String> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let cancel = Arc::new(AtomicBool::new(false));
    request.cancel = Some(Arc::clone(&cancel));
    let timeout_s = request.timeout.as_secs();
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let id = format!("verify-{stamp}-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    {
        let mut all = jobs().lock().map_err(|e| e.to_string())?;
        let running = all.iter().filter(|j| j.result.is_none()).count();
        if running >= MAX_RUNNING {
            return Err(format!("{running} verify jobs are already running"));
        }
        all.push(Job { id: id.clone(), started: Instant::now(), timeout_s, cancel, result: None });
    }
    let thread_id = id.clone();
    std::thread::spawn(move || {
        let verdict = harness::verify(&request);
        if let Ok(mut all) = jobs().lock() {
            if let Some(job) = all.iter_mut().find(|j| j.id == thread_id) { job.result = Some(verdict); }
            // Drop the oldest finished jobs beyond the limit.
            while all.iter().filter(|j| j.result.is_some()).count() > KEEP_FINISHED {
                let Some(oldest) = all.iter().position(|j| j.result.is_some()) else { break };
                all.remove(oldest);
            }
        }
    });
    Ok(crate::canon::obj(vec![
        ("job_id", Value::string(id)),
        ("status", Value::string("running")),
        ("timeout_s", Value::Int(timeout_s as i64)),
        ("next", Value::string("Call job_status with this job_id; pass wait_s to wait for the result")),
    ]))
}

/// The job's state. With `wait_s`, wait up to that long (at most 600 s) for
/// it to finish before answering.
pub fn status(id: &str, wait_s: f64) -> Value {
    let deadline = Instant::now() + Duration::from_secs_f64(wait_s.clamp(0.0, MAX_WAIT_S));
    loop {
        let snapshot = jobs().lock().ok().and_then(|all| all.iter().find(|j| j.id == id).map(|j| (j.started.elapsed(), j.timeout_s, j.result.clone(), j.cancel.load(Ordering::Relaxed))));
        let Some((elapsed, timeout_s, result, cancelling)) = snapshot else {
            return crate::canon::obj(vec![
                ("job_id", Value::string(id)),
                ("status", Value::string("not_found")),
                ("hint", Value::string("Job ids last while the MCP server runs, for the 32 most recent finished jobs. Start a new one with verify_step background: true")),
            ]);
        };
        let elapsed_s = Value::Float(crate::canon::py_round(elapsed.as_secs_f64(), 1));
        if let Some(result) = result {
            let state = if crate::canon::get(&result, "cancelled").and_then(crate::canon::as_bool) == Some(true) { "cancelled" } else { "completed" };
            return crate::canon::obj(vec![
                ("job_id", Value::string(id)),
                ("status", Value::string(state)),
                ("elapsed_s", elapsed_s),
                ("result", result),
            ]);
        }
        if Instant::now() >= deadline {
            let state = if cancelling { "cancel_requested" } else { "running" };
            return crate::canon::obj(vec![
                ("job_id", Value::string(id)),
                ("status", Value::string(state)),
                ("elapsed_s", elapsed_s),
                ("timeout_s", Value::Int(timeout_s as i64)),
                ("next", Value::string("Not finished yet. Call job_status again, with wait_s up to 600 to wait here")),
            ]);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Ask a job to stop. The test command is killed within a few milliseconds.
pub fn cancel(id: &str) -> Value {
    let found = jobs().lock().ok().and_then(|all| all.iter().find(|j| j.id == id).map(|j| {
        j.cancel.store(true, Ordering::Relaxed);
        j.result.is_some()
    }));
    let status = match found {
        None => "not_found",
        Some(true) => "already_finished",
        Some(false) => "cancel_requested",
    };
    crate::canon::obj(vec![
        ("job_id", Value::string(id)),
        ("status", Value::string(status)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_job_returns_at_once_and_its_result_comes_from_job_status() {
        let dir = std::env::temp_dir().join(format!("trammel-jobs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let started = start(VerifyRequest::new(dir.clone(), harness::no_op_cmd(), Duration::from_secs(30))).unwrap();
        let id = crate::canon::get(&started, "job_id").and_then(crate::canon::as_str).unwrap().to_string();
        assert_eq!(crate::canon::get(&started, "status").and_then(crate::canon::as_str), Some("running"));
        let done = status(&id, 30.0);
        assert_eq!(crate::canon::get(&done, "status").and_then(crate::canon::as_str), Some("completed"), "{}", crate::canon::stable(&done));
        let result = crate::canon::get(&done, "result").unwrap();
        assert_eq!(crate::canon::get(result, "success").and_then(crate::canon::as_bool), Some(true));
        assert_eq!(crate::canon::get(&cancel(&id), "status").and_then(crate::canon::as_str), Some("already_finished"));
        assert_eq!(crate::canon::get(&status("verify-nope", 0.0), "status").and_then(crate::canon::as_str), Some("not_found"));
        // A slow job: no answer yet without waiting, then cancelled.
        let slow: Vec<String> = if cfg!(windows) { vec!["cmd".into(), "/C".into(), "ping -n 30 127.0.0.1 >nul".into()] } else { vec!["sh".into(), "-c".into(), "sleep 30".into()] };
        let started = start(VerifyRequest::new(dir.clone(), slow, Duration::from_secs(60))).unwrap();
        let id = crate::canon::get(&started, "job_id").and_then(crate::canon::as_str).unwrap().to_string();
        let now = status(&id, 0.2);
        assert_eq!(crate::canon::get(&now, "status").and_then(crate::canon::as_str), Some("running"), "{}", crate::canon::stable(&now));
        assert!(crate::canon::get(&now, "next").is_some());
        assert_eq!(crate::canon::get(&cancel(&id), "status").and_then(crate::canon::as_str), Some("cancel_requested"));
        let gone = status(&id, 20.0);
        assert_eq!(crate::canon::get(&gone, "status").and_then(crate::canon::as_str), Some("cancelled"), "{}", crate::canon::stable(&gone));
        crate::workspace::forget(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
