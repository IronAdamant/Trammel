//! Isolated edit verification. The project is copied, edits are applied, every
//! language gets a preflight, then the test command runs with a timeout.

use crate::json::Value;
use crate::lang::code::bracket_errors;
use crate::walk::is_ignored_dir;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const DEFAULT_CAP: usize = 4096;

pub struct VerifyRequest {
    pub edits: Vec<(String, String)>,
    pub root: PathBuf,
    pub test_cmd: Vec<String>,
    pub timeout: Duration,
    pub full_output: bool,
    pub cap: usize,
}

pub fn verify(request: &VerifyRequest) -> Value {
    let static_analysis = static_analysis(&request.edits);
    let preflight = preflight(&request.edits);
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
    let tmp = scratch_dir();
    let _ = fs::remove_dir_all(&tmp);
    if let Err(err) = copy_tree(&request.root, &tmp) {
        return fail(&err, static_analysis, preflight);
    }
    for (path, content) in &request.edits {
        let dest = tmp.join(path);
        if let Some(parent) = dest.parent() { let _ = fs::create_dir_all(parent); }
        if let Err(err) = fs::write(&dest, content) {
            let _ = fs::remove_dir_all(&tmp);
            return fail(&err.to_string(), static_analysis, preflight);
        }
    }
    let outcome = run_cmd(&tmp, &request.test_cmd, request.timeout);
    let _ = fs::remove_dir_all(&tmp);
    let (trace, truncated) = cap_text(&outcome.trace, request.cap, request.full_output);
    let (output, out_trunc) = cap_text(&outcome.output, request.cap, request.full_output);
    crate::canon::obj(vec![
        ("success", Value::Bool(outcome.success)),
        ("score", Value::Float(if outcome.success { 1.0 } else { 0.0 })),
        ("output", Value::string(output)),
        ("trace", Value::string(trace)),
        ("truncated", Value::Bool(truncated || out_trunc)),
        ("static_analysis", static_analysis),
        ("preflight", preflight),
    ])
}

/// A fresh copy target. Two `verify` calls in one process must never share it,
/// or one run deletes the other's tree mid-test.
fn scratch_dir() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    std::env::temp_dir().join(format!("trammel-verify-{}-{nanos}-{n}", std::process::id()))
}

struct Ran { success: bool, output: String, trace: String }

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
            if fs::symlink_metadata(&candidate).is_err() { continue; }
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
fn program(name: &str) -> PathBuf {
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
    for name in ["python3", "python"] {
        if let Some((_, false)) = windows_lookup(name) { return vec![name.into()]; }
    }
    if windows_lookup("py").is_some() { return vec!["py".into(), "-3".into()]; }
    vec!["python3".into()]
}

/// A command that succeeds and does nothing, for languages without a runner.
pub fn no_op_cmd() -> Vec<String> {
    if cfg!(windows) { vec!["cmd".into(), "/C".into(), "exit 0".into()] } else { vec!["true".into()] }
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe { let _ = pipe.read_to_end(&mut bytes); }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn run_cmd(cwd: &Path, cmd: &[String], timeout: Duration) -> Ran {
    if cmd.is_empty() { return Ran { success: false, output: String::new(), trace: "empty test command".into() }; }
    let mut child = match Command::new(program(&cmd[0])).args(&cmd[1..]).current_dir(cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(child) => child,
        Err(err) => return Ran { success: false, output: String::new(), trace: format!("could not start {}: {err}. Pass test_cmd or set it in .trammel.json", cmd[0]) },
    };
    // Read both pipes while the test runs. Reading after exit deadlocks once
    // the output fills the pipe buffer, and the run then looks like a timeout.
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let start = Instant::now();
    loop {
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            // Do not join the readers: a grandchild may still hold the pipes.
            drop((stdout, stderr));
            return Ran { success: false, output: "timeout".into(), trace: format!("test command exceeded {}s and was killed", timeout.as_secs()) };
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = stdout.join().unwrap_or_default();
                let trace = stderr.join().unwrap_or_default();
                return Ran { success: status.success(), output, trace };
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(err) => return Ran { success: false, output: String::new(), trace: err.to_string() },
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

fn preflight(edits: &[(String, String)]) -> Value {
    let mut issues = Vec::new();
    for (path, content) in edits {
        let ext = path.rsplit('.').next().unwrap_or("");
        let tree = crate::syntax::parse(ext, content);
        if tree.render(content) != *content {
            issues.push(format!("{path}: parser did not round-trip"));
        }
        for err in bracket_errors(content) {
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

fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    let entries = fs::read_dir(src).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_ignored_dir(&name) || name == ".trammel" { continue; }
        let from = entry.path();
        let to = dst.join(&name);
        if from.is_dir() { copy_tree(&from, &to)?; } else if from.is_file() { fs::copy(&from, &to).map_err(|e| e.to_string())?; }
    }
    Ok(())
}

pub fn default_cap() -> usize { DEFAULT_CAP }

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("trammel-harness-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn concurrent_verifies_get_their_own_copy() {
        let a = scratch_dir();
        let b = scratch_dir();
        assert_ne!(a, b);
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
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn large_test_output_does_not_stall_until_the_timeout() {
        let dir = scratch("big");
        fs::write(dir.join("big.txt"), "x".repeat(2 << 20)).unwrap();
        let cat: Vec<String> = if cfg!(windows) { vec!["cmd".into(), "/C".into(), "type big.txt".into()] } else { vec!["cat".into(), "big.txt".into()] };
        let started = Instant::now();
        let verdict = verify(&VerifyRequest { edits: vec![], root: dir.clone(), test_cmd: cat, timeout: Duration::from_secs(20), full_output: false, cap: DEFAULT_CAP });
        assert!(started.elapsed() < Duration::from_secs(15), "verify stalled for {:?}", started.elapsed());
        assert_eq!(crate::canon::get(&verdict, "success").and_then(crate::canon::as_bool), Some(true), "{}", crate::canon::stable(&verdict));
        assert_eq!(crate::canon::get(&verdict, "truncated").and_then(crate::canon::as_bool), Some(true));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_commands_start_on_this_os() {
        let dir = scratch("noop");
        let verdict = verify(&VerifyRequest { edits: vec![], root: dir.clone(), test_cmd: no_op_cmd(), timeout: Duration::from_secs(20), full_output: false, cap: DEFAULT_CAP });
        assert_eq!(crate::canon::get(&verdict, "success").and_then(crate::canon::as_bool), Some(true), "{}", crate::canon::stable(&verdict));
        let mut version = python_cmd();
        version.push("--version".into());
        let ran = run_cmd(&dir, &version, Duration::from_secs(20));
        assert!(ran.success, "{version:?} did not run: {} {}", ran.output, ran.trace);
        let _ = fs::remove_dir_all(&dir);
    }
}
