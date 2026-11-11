//! Isolated edit verification. The project is copied (a reused copy per
//! project, see `workspace`), edits are applied, every language gets a
//! preflight, then the test command runs with a timeout.

use crate::json::Value;
use crate::lang::brackets::bracket_errors;
use crate::workspace::Workspace;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const DEFAULT_CAP: usize = 4096;

/// Seconds a verify may run when nothing else says: enough for an
/// incremental build of a large workspace, well under an MCP client's idle
/// timeout.
pub const DEFAULT_TIMEOUT_S: u64 = 300;
/// The longest a verify that answers in the same call may run.
pub const MAX_SYNC_TIMEOUT_S: u64 = 600;
/// The longest a background verify (a job) may run.
pub const MAX_JOB_TIMEOUT_S: u64 = 3600;

pub struct VerifyRequest {
    pub edits: Vec<(String, String)>,
    pub root: PathBuf,
    pub test_cmd: Vec<String>,
    pub timeout: Duration,
    pub full_output: bool,
    pub cap: usize,
    /// Set by `cancel_job`; the test command is killed when it turns true.
    pub cancel: Option<Arc<AtomicBool>>,
}

impl VerifyRequest {
    pub fn new(root: PathBuf, test_cmd: Vec<String>, timeout: Duration) -> VerifyRequest {
        VerifyRequest { edits: Vec::new(), root, test_cmd, timeout, full_output: false, cap: DEFAULT_CAP, cancel: None }
    }
}

pub fn verify(request: &VerifyRequest) -> Value {
    let static_analysis = static_analysis(&request.edits);
    let preflight = preflight(&request.edits);
    // An edit with unbalanced brackets cannot pass. Say so now instead of
    // copying the project and running tests that may not even import it.
    if crate::canon::get(&preflight, "ok").and_then(crate::canon::as_bool) == Some(false) {
        return crate::canon::obj(vec![
            ("success", Value::Bool(false)),
            ("score", Value::Float(0.0)),
            ("output", Value::string("")),
            ("trace", Value::string("preflight failed; tests were not run. Fix the issues in preflight.issues and verify again.")),
            ("truncated", Value::Bool(false)),
            ("static_analysis", static_analysis),
            ("preflight", preflight),
        ]);
    }
    if request.test_cmd.is_empty() {
        return crate::canon::obj(vec![
            ("success", Value::Bool(false)),
            ("score", Value::Float(0.0)),
            ("trace", Value::string("no test command")),
            ("truncated", Value::Bool(false)),
            ("static_analysis", static_analysis),
            ("preflight", preflight),
        ]);
    }
    let started = Instant::now();
    let mut ws = match Workspace::prepare(&request.root) {
        Ok(ws) => ws,
        Err(err) => return fail(&err, static_analysis, preflight),
    };
    for (path, content) in &request.edits {
        if let Err(err) = ws.write_edit(path, content) {
            ws.finish();
            return fail(&err, static_analysis, preflight);
        }
    }
    let mut env: Vec<(&str, PathBuf)> = Vec::new();
    if let Some(target) = &ws.target { env.push(("CARGO_TARGET_DIR", target.clone())); }
    let outcome = run_cmd(&ws.tree, &request.test_cmd, request.timeout, &env, request.cancel.as_deref());
    let reused = ws.cached_copy();
    let written = ws.files_written;
    ws.finish();
    let (trace, truncated) = cap_text(&outcome.trace, request.cap, request.full_output);
    let (output, out_trunc) = cap_text(&outcome.output, request.cap, request.full_output);
    let mut out = crate::canon::obj(vec![
        ("success", Value::Bool(outcome.success)),
        ("score", Value::Float(if outcome.success { 1.0 } else { 0.0 })),
        ("output", Value::string(output)),
        ("trace", Value::string(trace)),
        ("truncated", Value::Bool(truncated || out_trunc)),
        ("elapsed_s", Value::Float(crate::canon::py_round(started.elapsed().as_secs_f64(), 1))),
        ("timeout_s", Value::Int(request.timeout.as_secs() as i64)),
        ("copy", crate::canon::obj(vec![("reused", Value::Bool(reused)), ("files_written", Value::Int(written as i64))])),
        ("static_analysis", static_analysis),
        ("preflight", preflight),
    ]);
    if outcome.timed_out {
        crate::canon::insert(&mut out, "timed_out", Value::Bool(true));
        crate::canon::insert(&mut out, "hint", Value::string(format!(
            "The test command ran past timeout_s ({}s) and was killed. Pass a larger timeout_s (up to {MAX_SYNC_TIMEOUT_S} per call), set verify_timeout_s in .trammel.json, or pass background: true (up to {MAX_JOB_TIMEOUT_S}s) and poll job_status. The copy and its build cache are kept, so the next run starts where this one stopped.",
            request.timeout.as_secs()
        )));
    }
    if outcome.cancelled { crate::canon::insert(&mut out, "cancelled", Value::Bool(true)); }
    out
}

struct Ran { success: bool, output: String, trace: String, timed_out: bool, cancelled: bool }

/// Finds `name` on `path` the way a Windows shell does: every `PATHEXT` suffix in
/// order, so `npm` finds `npm.cmd`. A hit under `Microsoft\WindowsApps` is only a
/// fallback, because `python3.exe` there is usually the Store installer stub that
/// exits 49 without running anything. Returns the path and whether it is that alias.
/// Names with a directory or an extension are left to `Command`.
pub fn find_program(name: &str, path: Option<&std::ffi::OsStr>, pathext: &str) -> Option<(PathBuf, bool)> {
    let as_path = Path::new(name);
    if as_path.components().count() != 1 || as_path.extension().is_some() { return None; }
    let mut alias = None;
    for dir in std::env::split_paths(path?) {
        let is_alias = dir.ends_with(Path::new("Microsoft").join("WindowsApps"));
        for ext in pathext.split(';').filter(|e| !e.is_empty()) {
            let candidate = dir.join(format!("{name}{}", ext.to_ascii_lowercase()));
            // Store aliases are reparse points that `is_file` cannot follow.
            if std::fs::symlink_metadata(&candidate).is_err() { continue; }
            if !is_alias { return Some((candidate, false)); }
            alias.get_or_insert((candidate, true));
        }
    }
    alias
}

fn windows_lookup(name: &str) -> Option<(PathBuf, bool)> {
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    find_program(name, std::env::var_os("PATH").as_deref(), &pathext)
}

/// The program `Command` should start. On Windows a bare `npm` or `npx` is a
/// `.cmd` shim that `Command::new` does not search for, so resolve it first.
pub(crate) fn program(name: &str) -> PathBuf {
    if cfg!(windows) {
        if let Some((found, _)) = windows_lookup(name) { return found; }
    }
    PathBuf::from(name)
}

/// The Python launcher for the default test command. Elsewhere `python3`. On
/// Windows the first real `python3` or `python`, then the `py -3` launcher,
/// skipping Store aliases while any real install exists.
pub fn python_cmd() -> Vec<String> {
    if !cfg!(windows) { return vec!["python3".into()]; }
    // Searching PATH x PATHEXT costs tens of milliseconds on Windows; the
    // answer does not change while the server runs.
    static CHOICE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    CHOICE.get_or_init(|| {
        let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        windows_python(std::env::var_os("PATH").as_deref(), &pathext)
    }).clone()
}

/// The Windows choice behind `python_cmd`, for a given PATH and PATHEXT.
fn windows_python(path: Option<&std::ffi::OsStr>, pathext: &str) -> Vec<String> {
    for name in ["python3", "python"] {
        if let Some((_, false)) = find_program(name, path, pathext) { return vec![name.into()]; }
    }
    if find_program("py", path, pathext).is_some() { return vec!["py".into(), "-3".into()]; }
    vec!["python3".into()]
}

/// A command that succeeds and does nothing, for languages without a runner.
pub fn no_op_cmd() -> Vec<String> {
    if cfg!(windows) { vec!["cmd".into(), "/C".into(), "exit 0".into()] } else { vec!["true".into()] }
}

/// Read a pipe into a shared buffer while the command runs, so a timeout can
/// still report what was printed.
fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> (Arc<Mutex<Vec<u8>>>, std::thread::JoinHandle<()>) {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&buffer);
    let handle = std::thread::spawn(move || {
        let Some(mut pipe) = pipe else { return };
        let mut chunk = [0u8; 8192];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => { if let Ok(mut held) = sink.lock() { held.extend_from_slice(&chunk[..n]); } }
            }
        }
    });
    (buffer, handle)
}

fn text_of(buffer: &Arc<Mutex<Vec<u8>>>) -> String {
    buffer.lock().map(|held| String::from_utf8_lossy(&held).into_owned()).unwrap_or_default()
}

fn run_cmd(cwd: &Path, cmd: &[String], timeout: Duration, env: &[(&str, PathBuf)], cancel: Option<&AtomicBool>) -> Ran {
    let failed = |trace: String| Ran { success: false, output: String::new(), trace, timed_out: false, cancelled: false };
    if cmd.is_empty() { return failed("empty test command".into()); }
    let mut command = Command::new(program(&cmd[0]));
    command.args(&cmd[1..]).current_dir(cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for (key, value) in env { command.env(key, value); }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => return failed(format!("could not start {}: {err}. Pass test_cmd or set it in .trammel.json", cmd[0])),
    };
    // Read both pipes while the test runs. Reading after exit deadlocks once
    // the output fills the pipe buffer, and the run then looks like a timeout.
    let (stdout, out_reader) = drain(child.stdout.take());
    let (stderr, err_reader) = drain(child.stderr.take());
    let start = Instant::now();
    loop {
        let cancelled = cancel.map(|c| c.load(Ordering::Relaxed)).unwrap_or(false);
        if cancelled || start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            // Do not join the readers: a grandchild may still hold the pipes.
            let why = if cancelled { "cancelled by cancel_job".to_string() } else { format!("test command exceeded {}s and was killed", timeout.as_secs()) };
            let trace = format!("{why}\n{}", text_of(&stderr));
            return Ran { success: false, output: text_of(&stdout), trace, timed_out: !cancelled, cancelled };
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = out_reader.join();
                let _ = err_reader.join();
                return Ran { success: status.success(), output: text_of(&stdout), trace: text_of(&stderr), timed_out: false, cancelled: false };
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(err) => return failed(err.to_string()),
        }
    }
}

fn cap_text(text: &str, cap: usize, full: bool) -> (String, bool) {
    if full || text.len() <= cap { return (text.to_string(), false); }
    let head = cap / 2;
    let tail = cap - head;
    let mut out = text.chars().take(head).collect::<String>();
    out.push_str("\n…\n");
    let chars: Vec<char> = text.chars().collect();
    if chars.len() > tail { out.extend(chars[chars.len() - tail..].iter()); }
    (out, true)
}

/// True when every edit passes the bracket check.
pub fn preflight_ok(edits: &[(String, String)]) -> bool {
    edits.iter().all(|(path, content)| bracket_errors(path.rsplit('.').next().unwrap_or(""), content).is_empty())
}

fn preflight(edits: &[(String, String)]) -> Value {
    let mut issues = Vec::new();
    for (path, content) in edits {
        let ext = path.rsplit('.').next().unwrap_or("");
        for err in bracket_errors(ext, content) {
            issues.push(format!("{path}: {err}"));
        }
    }
    crate::canon::obj(vec![
        ("ok", Value::Bool(issues.is_empty())),
        ("issues", Value::Array(issues.into_iter().map(Value::String).collect())),
    ])
}

fn static_analysis(edits: &[(String, String)]) -> Value {
    let mut warnings = Vec::new();
    let mut score: f64 = 1.0;
    for (path, content) in edits {
        let markers = content.matches("TODO").count() + content.matches("FIXME").count();
        if markers > 2 {
            warnings.push(format!("{path}: contains {markers} TODO/FIXME markers"));
            score -= 0.05;
        }
        if content.contains('\t') && content.contains("    ") {
            warnings.push(format!("{path}: mixed indentation (tabs and spaces)"));
            score -= 0.05;
        }
    }
    crate::canon::obj(vec![
        ("confidence", Value::Float((score.max(0.0) * 100.0).round() / 100.0)),
        ("warnings", Value::Array(warnings.into_iter().map(Value::String).collect())),
        ("edited_files", Value::Array(edits.iter().map(|(p, _)| Value::string(p.clone())).collect())),
    ])
}

fn fail(message: &str, static_analysis: Value, preflight: Value) -> Value {
    crate::canon::obj(vec![
        ("success", Value::Bool(false)),
        ("score", Value::Float(0.0)),
        ("trace", Value::string(message)),
        ("truncated", Value::Bool(false)),
        ("static_analysis", static_analysis),
        ("preflight", preflight),
    ])
}

pub fn default_cap() -> usize { DEFAULT_CAP }

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("trammel-harness-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_preflight_failure_is_not_a_success() {
        let dir = scratch("preflight");
        std::fs::write(dir.join("ok.py"), "x = 1\n").unwrap();
        let broken = vec![("ok.py".to_string(), "def f(:\n    pass\n".to_string())];
        let verdict = verify(&VerifyRequest { edits: broken, ..VerifyRequest::new(dir.clone(), no_op_cmd(), Duration::from_secs(20)) });
        assert_eq!(crate::canon::get(&verdict, "success").and_then(crate::canon::as_bool), Some(false), "{}", crate::canon::stable(&verdict));
        assert!(crate::canon::stable(crate::canon::get(&verdict, "preflight").unwrap()).contains("ok.py: line 1: '(' is never closed"));
        // The same command passes once the edit is fixed.
        let fixed = vec![("ok.py".to_string(), "def f():\n    pass\n".to_string())];
        let verdict = verify(&VerifyRequest { edits: fixed, ..VerifyRequest::new(dir.clone(), no_op_cmd(), Duration::from_secs(20)) });
        assert_eq!(crate::canon::get(&verdict, "success").and_then(crate::canon::as_bool), Some(true), "{}", crate::canon::stable(&verdict));
        crate::workspace::forget(&dir);
        let _ = fs::remove_dir_all(&dir);
    }

    fn write(root: &Path, rel: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, rel).unwrap();
    }

    fn listed(root: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() { stack.push(path); } else { out.push(crate::walk::rel_slash(&path, root)); }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn verify_copies_what_git_would_commit_and_skips_ignored_files() {
        let src = scratch("gitcopy-src");
        let run = |args: &[&str]| assert!(Command::new("git").arg("-C").arg(&src).args(args).output().unwrap().status.success(), "git {args:?}");
        run(&["init", "-q"]);
        fs::write(src.join(".gitignore"), "gallery-full/\n*.log\n").unwrap();
        for rel in ["src/lib.rs", "gallery-full/shot_001.png", "run.log", "notes/new_untracked.md", ".trammel/store"] { write(&src, rel); }
        run(&["add", ".gitignore", "src/lib.rs"]);
        let files = crate::workspace::git_files(&src).expect("a git work tree");
        assert_eq!(files, vec![".gitignore", ".trammel/store", "notes/new_untracked.md", "src/lib.rs"]);
        let dst = scratch("gitcopy-dst");
        crate::workspace::copy_files(&src, &dst, &crate::workspace::project_files(&src)).unwrap();
        assert_eq!(listed(&dst), vec![".gitignore", "notes/new_untracked.md", "src/lib.rs"], "ignored files, .git and the store stay behind");
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dst);
    }

    #[test]
    fn outside_git_the_whole_tree_is_copied_minus_generated_dirs() {
        let src = scratch("walkcopy-src");
        for rel in ["src/lib.rs", "target/debug/big.bin", "node_modules/x/index.js", "data/table.csv", ".trammel/store"] { write(&src, rel); }
        let dst = scratch("walkcopy-dst");
        crate::workspace::copy_files(&src, &dst, &crate::workspace::project_files(&src)).unwrap();
        assert_eq!(listed(&dst), vec!["data/table.csv", "src/lib.rs"]);
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dst);
    }

    #[test]
    fn find_program_uses_pathext_and_skips_store_aliases() {
        let dir = scratch("path");
        let store = dir.join("Microsoft").join("WindowsApps");
        let real = dir.join("Python313");
        let node = dir.join("nodejs");
        for d in [&store, &real, &node] { fs::create_dir_all(d).unwrap(); }
        fs::write(store.join("python3.exe"), b"").unwrap();
        fs::write(store.join("python.exe"), b"").unwrap();
        fs::write(real.join("python.exe"), b"").unwrap();
        fs::write(node.join("npm.cmd"), b"").unwrap();
        let path = std::env::join_paths([&store, &real, &node]).unwrap();
        let ext = ".COM;.EXE;.BAT;.CMD";
        assert_eq!(find_program("npm", Some(&path), ext), Some((node.join("npm.cmd"), false)));
        // A real install later on PATH wins over the alias earlier on PATH.
        assert_eq!(find_program("python", Some(&path), ext), Some((real.join("python.exe"), false)));
        // Only the alias exists: it is returned and marked.
        assert_eq!(find_program("python3", Some(&path), ext), Some((store.join("python3.exe"), true)));
        assert_eq!(find_program("cargo", Some(&path), ext), None);
        assert_eq!(find_program("npm.cmd", Some(&path), ext), None);
        assert_eq!(find_program("bin/npm", Some(&path), ext), None);
        crate::workspace::forget(&dir);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn large_test_output_does_not_stall_until_the_timeout() {
        let dir = scratch("big");
        fs::write(dir.join("big.txt"), "x".repeat(2 << 20)).unwrap();
        let cat: Vec<String> = if cfg!(windows) { vec!["cmd".into(), "/C".into(), "type big.txt".into()] } else { vec!["cat".into(), "big.txt".into()] };
        let started = Instant::now();
        let verdict = verify(&VerifyRequest::new(dir.clone(), cat, Duration::from_secs(20)));
        assert!(started.elapsed() < Duration::from_secs(15), "verify stalled for {:?}", started.elapsed());
        assert_eq!(crate::canon::get(&verdict, "success").and_then(crate::canon::as_bool), Some(true), "{}", crate::canon::stable(&verdict));
        assert_eq!(crate::canon::get(&verdict, "truncated").and_then(crate::canon::as_bool), Some(true));
        crate::workspace::forget(&dir);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_commands_start_on_this_os() {
        let dir = scratch("noop");
        let verdict = verify(&VerifyRequest::new(dir.clone(), no_op_cmd(), Duration::from_secs(20)));
        assert_eq!(crate::canon::get(&verdict, "success").and_then(crate::canon::as_bool), Some(true), "{}", crate::canon::stable(&verdict));
        crate::workspace::forget(&dir);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn windows_python_prefers_a_real_install_over_the_store_alias() {
        let dir = scratch("pychoice");
        let store = dir.join("Microsoft").join("WindowsApps");
        let real = dir.join("Python313");
        let launcher = dir.join("Windows");
        for d in [&store, &real, &launcher] { fs::create_dir_all(d).unwrap(); }
        fs::write(store.join("python3.exe"), b"").unwrap();
        fs::write(store.join("python.exe"), b"").unwrap();
        let ext = ".COM;.EXE;.BAT;.CMD";
        let only_alias = std::env::join_paths([&store]).unwrap();
        assert_eq!(windows_python(Some(&only_alias), ext), vec!["python3"]);
        fs::write(launcher.join("py.exe"), b"").unwrap();
        let alias_and_launcher = std::env::join_paths([&store, &launcher]).unwrap();
        assert_eq!(windows_python(Some(&alias_and_launcher), ext), vec!["py", "-3"]);
        fs::write(real.join("python.exe"), b"").unwrap();
        let everything = std::env::join_paths([&store, &launcher, &real]).unwrap();
        assert_eq!(windows_python(Some(&everything), ext), vec!["python"]);
        crate::workspace::forget(&dir);
        let _ = fs::remove_dir_all(&dir);
    }

    /// A command that prints a line, then sleeps for about `secs` seconds.
    fn slow(secs: u32) -> Vec<String> {
        if cfg!(windows) { vec!["cmd".into(), "/C".into(), format!("echo started & ping -n {} 127.0.0.1 >nul", secs + 1)] } else { vec!["sh".into(), "-c".into(), format!("echo started; sleep {secs}")] }
    }

    #[test]
    fn a_timeout_says_so_with_the_time_and_how_to_raise_it() {
        let dir = scratch("timeout");
        let started = Instant::now();
        let verdict = verify(&VerifyRequest::new(dir.clone(), slow(20), Duration::from_secs(1)));
        assert!(started.elapsed() < Duration::from_secs(15), "{:?}", started.elapsed());
        let get = |k: &str| crate::canon::get(&verdict, k).cloned();
        assert_eq!(get("success").and_then(|v| crate::canon::as_bool(&v)), Some(false));
        assert_eq!(get("timed_out").and_then(|v| crate::canon::as_bool(&v)), Some(true), "{}", crate::canon::stable(&verdict));
        assert_eq!(get("timeout_s").and_then(|v| crate::canon::as_i64(&v)), Some(1));
        assert!(get("elapsed_s").and_then(|v| crate::canon::as_f64(&v)).unwrap() >= 1.0);
        let hint = get("hint").and_then(|v| crate::canon::as_str(&v).map(String::from)).unwrap();
        assert!(hint.contains("timeout_s") && hint.contains("verify_timeout_s") && hint.contains("background"), "{hint}");
        assert!(get("output").and_then(|v| crate::canon::as_str(&v).map(String::from)).unwrap().contains("started"), "output printed before the timeout is kept");
        crate::workspace::forget(&dir);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cancel_flag_stops_the_command() {
        let dir = scratch("cancel");
        let flag = Arc::new(AtomicBool::new(false));
        let request = VerifyRequest { cancel: Some(Arc::clone(&flag)), ..VerifyRequest::new(dir.clone(), slow(20), Duration::from_secs(60)) };
        let setter = std::thread::spawn(move || { std::thread::sleep(Duration::from_millis(500)); flag.store(true, Ordering::Relaxed); });
        let started = Instant::now();
        let verdict = verify(&request);
        setter.join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(15));
        assert_eq!(crate::canon::get(&verdict, "cancelled").and_then(crate::canon::as_bool), Some(true), "{}", crate::canon::stable(&verdict));
        assert!(crate::canon::get(&verdict, "timed_out").is_none());
        crate::workspace::forget(&dir);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cargo_builds_in_a_reused_target_dir() {
        let dir = scratch("target");
        fs::write(dir.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        let show: Vec<String> = if cfg!(windows) { vec!["cmd".into(), "/C".into(), "echo %CARGO_TARGET_DIR%".into()] } else { vec!["sh".into(), "-c".into(), "echo $CARGO_TARGET_DIR".into()] };
        let first = verify(&VerifyRequest::new(dir.clone(), show.clone(), Duration::from_secs(20)));
        let second = verify(&VerifyRequest::new(dir.clone(), show, Duration::from_secs(20)));
        let target = |v: &Value| crate::canon::get(v, "output").and_then(crate::canon::as_str).unwrap().trim().to_string();
        assert!(target(&first).ends_with("target"), "{}", crate::canon::stable(&first));
        assert_eq!(target(&first), target(&second), "the same target dir both times");
        assert_eq!(crate::canon::stable(crate::canon::get(&second, "copy").unwrap()), r#"{"files_written":0,"reused":true}"#);
        crate::workspace::forget(&dir);
        let _ = fs::remove_dir_all(&dir);
    }
}
