//! Time the Rust binary and fail if a tracked command is slower than the
//! recorded Python median. Also self-checks the regression rule by calling the
//! Rust benchstat tests (A/A passes, an injected slowdown fails).
//!
//! Usage, from the repo root:
//!   cargo run --release --example bench_gate -- target/release/trammel
//!   cargo run --release --example bench_gate -- --self-test
//!
//! Same checks, report file, and pass/fail exit codes as the
//! `scripts/bench_gate.py` it replaced; usage errors exit 2.

#[path = "support/py.rs"]
mod py;

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::Instant;

const FLOORS: [(&str, f64); 3] = [("version", 78.1), ("help", 78.7), ("mcp-initialize", 306.6)];
const RUNS: usize = 10;
const SELF_TEST: [&str; 8] = ["cargo", "test", "--offline", "--lib", "benchstat", "--", "--test-threads", "8"];

/// `raise SystemExit(message)`: the message on stderr, exit 1.
fn exit_with(message: &str) -> ExitCode {
    eprintln!("{message}");
    ExitCode::from(1)
}

fn time_ms(argv: &[String]) -> Result<Vec<f64>, String> {
    let mut samples = Vec::new();
    for _ in 0..RUNS {
        let t0 = Instant::now();
        let out = Command::new(&argv[0]).args(&argv[1..]).stdin(Stdio::inherit()).output().map_err(|e| e.to_string())?;
        samples.push(t0.elapsed().as_secs_f64() * 1000.0);
        let code = out.status.code().unwrap_or(-1);
        if code != 0 && code != 2 {
            return Err(format!("{} exited with status {code}; it must exit 0 (or 2 for usage)", argv.join(" ")));
        }
    }
    Ok(samples)
}

fn mcp_init(argv: &[String]) -> Result<Vec<f64>, String> {
    let mut samples = Vec::new();
    for _ in 0..RUNS {
        let t0 = Instant::now();
        let mut child = Command::new(&argv[0]).args(&argv[1..]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
        let mut stdin = child.stdin.take().unwrap();
        let _ = stdin.write_all(br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"bench","version":"0"}}}"#);
        let _ = stdin.write_all(b"\n");
        let _ = stdin.flush();
        let mut line = Vec::new();
        let _ = BufReader::new(child.stdout.take().unwrap()).read_until(b'\n', &mut line);
        let elapsed = t0.elapsed().as_secs_f64() * 1000.0;
        let _ = child.kill();
        let _ = child.wait();
        if !line.windows(7).any(|w| w == b"trammel") {
            return Err("mcp initialize did not answer".into());
        }
        samples.push(elapsed);
    }
    Ok(samples)
}

/// Accept the documented `target/release/trammel` on Windows, where the file
/// is `trammel.exe`.
fn resolve_binary(arg: &str) -> Result<String, String> {
    let mut path = PathBuf::from(arg);
    if !path.is_file() && path.extension().is_none() && path.with_extension("exe").is_file() {
        path = path.with_extension("exe");
    }
    if !path.is_file() {
        return Err(format!("{arg}: no such binary; run `cargo build --release` first"));
    }
    Ok(std::path::absolute(&path).unwrap_or(path).display().to_string())
}

fn median(samples: &[f64]) -> f64 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = sorted.len();
    if n % 2 == 1 { sorted[n / 2] } else { (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0 }
}

struct Row { name: &'static str, median: f64, min: f64, max: f64, floor: f64 }

impl Row {
    fn new(name: &'static str, floor: f64, samples: &[f64]) -> Row {
        let min = samples.iter().copied().fold(f64::INFINITY, f64::min);
        let max = samples.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        Row { name, median: median(samples), min, max, floor }
    }

    fn beats(&self) -> bool { self.median < self.floor }

    /// `(key, value)` pairs in the report's key order.
    fn fields(&self) -> [(&'static str, String); 5] {
        [
            ("median_ms", py::float(py::round2(self.median))),
            ("min_ms", py::float(py::round2(self.min))),
            ("max_ms", py::float(py::round2(self.max))),
            ("python_median_ms", py::float(self.floor)),
            ("beats_python", self.beats().to_string()),
        ]
    }

    /// `json.dumps({"name": name, **report[name]})`.
    fn line(&self) -> String {
        let body: Vec<String> = self.fields().iter().map(|(k, v)| format!("\"{k}\": {v}")).collect();
        format!("{{\"name\": \"{}\", {}}}", self.name, body.join(", "))
    }
}

/// `json.dumps(report, indent=1) + "\n"`, LF on every OS.
fn report_json(rows: &[Row]) -> String {
    let blocks: Vec<String> = rows.iter().map(|row| {
        let body: Vec<String> = row.fields().iter().map(|(k, v)| format!("  \"{k}\": {v}")).collect();
        format!(" \"{}\": {{\n{}\n }}", row.name, body.join(",\n"))
    }).collect();
    format!("{{\n{}\n}}\n", blocks.join(",\n"))
}

fn write_report(path: &Path, rows: &[Row]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::write(path, report_json(rows))
}

/// The helper checks `--self-test` runs before the benchstat tests.
fn check_helpers() {
    let tmp = std::env::temp_dir().join(format!("trammel-bench-gate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let exe = tmp.join("trammel.exe");
    std::fs::write(&exe, b"").unwrap();
    let want = std::path::absolute(&exe).unwrap().display().to_string();
    assert_eq!(resolve_binary(&tmp.join("trammel").display().to_string()), Ok(want.clone()));
    assert_eq!(resolve_binary(&exe.display().to_string()), Ok(want));
    let missing = resolve_binary(&tmp.join("missing").display().to_string()).unwrap_err();
    assert!(missing.contains("cargo build --release"), "a missing binary must fail");
    let out = tmp.join("bench").join("report.json");
    write_report(&out, &[Row::new("version", 78.1, &[1.0])]).unwrap();
    assert!(!std::fs::read(&out).unwrap().contains(&b'\r'));
    let _ = std::fs::remove_dir_all(&tmp);
}

fn self_test() -> ExitCode {
    check_helpers();
    let status = Command::new(SELF_TEST[0]).args(&SELF_TEST[1..]).status();
    match status {
        Ok(status) if status.success() => {
            println!("A/A passes and injected slowdown fails");
            ExitCode::SUCCESS
        }
        Err(_) => exit_with("cargo not found on PATH; install Rust or fix PATH"),
        other => {
            let code = other.ok().and_then(|s| s.code()).unwrap_or(1);
            exit_with(&format!("`{}` failed with exit status {code}; the benchstat tests above say which", SELF_TEST.join(" ")))
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--self-test") {
        return self_test();
    }
    let Some(arg) = args.first() else {
        eprintln!("usage: cargo run --release --example bench_gate -- <path to trammel binary> | --self-test");
        return ExitCode::from(2);
    };
    let binary = match resolve_binary(arg) { Ok(b) => b, Err(message) => return exit_with(&message) };
    let argv = |extra: &str| vec![binary.clone(), extra.to_string()];
    let mut rows = Vec::new();
    // One command at a time, stopping at the first failure, as the script did.
    for (name, floor) in FLOORS {
        let samples = match name {
            "version" => time_ms(&argv("--version")),
            "help" => time_ms(&argv("--help")),
            _ => mcp_init(&argv("mcp")),
        };
        match samples {
            Ok(samples) => rows.push(Row::new(name, floor, &samples)),
            Err(message) => return exit_with(&message),
        }
    }
    for row in &rows { println!("{}", row.line()); }
    if let Err(err) = write_report(Path::new("bench/rust_measured.json"), &rows) {
        return exit_with(&err.to_string());
    }
    if rows.iter().all(Row::beats) { ExitCode::SUCCESS } else { ExitCode::from(1) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers_pass_their_own_checks() {
        check_helpers();
    }

    #[test]
    fn output_matches_python_json_dumps() {
        let rows = [Row::new("version", 78.1, &[7.0, 9.0, 8.0, 7.966]), Row::new("mcp-initialize", 1.5, &[2.0])];
        assert_eq!(rows[0].line(), r#"{"name": "version", "median_ms": 7.98, "min_ms": 7.0, "max_ms": 9.0, "python_median_ms": 78.1, "beats_python": true}"#);
        assert_eq!(
            report_json(&rows),
            "{\n \"version\": {\n  \"median_ms\": 7.98,\n  \"min_ms\": 7.0,\n  \"max_ms\": 9.0,\n  \"python_median_ms\": 78.1,\n  \"beats_python\": true\n },\n \"mcp-initialize\": {\n  \"median_ms\": 2.0,\n  \"min_ms\": 2.0,\n  \"max_ms\": 2.0,\n  \"python_median_ms\": 1.5,\n  \"beats_python\": false\n }\n}\n"
        );
    }

    #[test]
    fn median_is_statistics_median() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), 2.5);
    }
}
