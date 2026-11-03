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
    let tmp = std::env::temp_dir().join(format!("trammel-verify-{}-{}", std::process::id(), Instant::now().elapsed().as_nanos()));
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

struct Ran { success: bool, output: String, trace: String }

fn run_cmd(cwd: &Path, cmd: &[String], timeout: Duration) -> Ran {
    if cmd.is_empty() { return Ran { success: false, output: String::new(), trace: "empty test command".into() }; }
    let mut child = match Command::new(&cmd[0]).args(&cmd[1..]).current_dir(cwd).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(child) => child,
        Err(err) => return Ran { success: false, output: String::new(), trace: err.to_string() },
    };
    let start = Instant::now();
    loop {
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Ran { success: false, output: "timeout".into(), trace: String::new() };
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut output = String::new();
                let mut trace = String::new();
                if let Some(mut stdout) = child.stdout.take() { let _ = stdout.read_to_string(&mut output); }
                if let Some(mut stderr) = child.stderr.take() { let _ = stderr.read_to_string(&mut trace); }
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
