//! Check the vendored parser files against scripts/syntax_core_manifest.txt.
//!
//! The manifest is the byte digest of the stele-context parser core copied
//! from commit 244561a491ac4002b2c4bc94a3884091870a6d11 (pull/5), plus the
//! newline index in `src/lang/python_symbols/`. A second mode,
//! `--against DIR`, compares those paths byte for byte with a stele checkout
//! and stops at the first file that differs.
//!
//! Run from the repo root: `cargo run --example check_syntax_core [-- --against DIR]`.
//! Same checks and exit codes as the `scripts/check_syntax_core.py` it
//! replaced. The ok line counts manifest entries (Python counted blank lines).

#[path = "support/py.rs"]
mod py;

use std::path::Path;
use std::process::ExitCode;

const MANIFEST: &str = "scripts/syntax_core_manifest.txt";

/// Each non-blank manifest line as `(digest, path)`.
fn entries(lines: &[String]) -> Result<Vec<(String, String)>, ExitCode> {
    let mut out = Vec::new();
    for (n, line) in lines.iter().enumerate() {
        if py::strip(line).is_empty() { continue; }
        let Some((digest, path)) = py::split_once_ws(line) else {
            eprintln!("{MANIFEST} line {}: expected \"<sha256>  <path>\"", n + 1);
            return Err(ExitCode::from(1));
        };
        out.push((digest.to_string(), path.to_string()));
    }
    Ok(out)
}

/// Problems in manifest order: `missing PATH` or `changed PATH`. Windows
/// checkouts may rewrite LF as CRLF; the manifest is the LF digest.
fn problems(root: &Path, entries: &[(String, String)]) -> Vec<String> {
    let mut bad = Vec::new();
    for (digest, path) in entries {
        let file = root.join(path);
        if !file.is_file() {
            bad.push(format!("missing {path}"));
            continue;
        }
        let bytes = std::fs::read(&file).unwrap_or_default();
        let mut lf = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') { i += 1; continue; }
            lf.push(bytes[i]);
            i += 1;
        }
        if trammel::hash::sha256_hex(&lf) != *digest {
            bad.push(format!("changed {path}"));
        }
    }
    bad
}

fn run(root: &Path, args: &[String]) -> ExitCode {
    let text = match py::read_text(&root.join(MANIFEST).to_string_lossy()) {
        Ok(text) => text,
        Err(_) => return py::not_found(MANIFEST),
    };
    let lines = py::splitlines(&text);
    let entries = match entries(&lines) { Ok(e) => e, Err(code) => return code };
    let bad = problems(root, &entries);
    if !bad.is_empty() {
        println!("{}", bad.join("\n"));
        return ExitCode::from(1);
    }
    println!("syntax core manifest ok ({} files)", entries.len());
    if args.len() == 2 && args[0] == "--against" {
        let other = Path::new(&args[1]);
        for (_, path) in &entries {
            let Ok(theirs) = std::fs::read(other.join(path)) else {
                eprintln!("{} has no {path}; pass the root of a stele-context checkout", py::path_str(&args[1]));
                return ExitCode::from(1);
            };
            let ours = std::fs::read(root.join(path)).unwrap_or_default();
            if theirs != ours {
                println!("differs from stele: {path}");
                return ExitCode::from(1);
            }
        }
        println!("syntax core matches {}", py::path_str(&args[1]));
    }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    run(Path::new(""), &args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("trammel-check-core-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/a.rs"), "fn a() {}\r\n").unwrap();
        let digest = trammel::hash::sha256_hex(b"fn a() {}\n");
        std::fs::write(dir.join(MANIFEST), format!("{digest}  src/a.rs\n\n")).unwrap();
        dir
    }

    #[test]
    fn crlf_checkouts_match_the_lf_digest() {
        let dir = tree("ok");
        let lines = py::splitlines(&std::fs::read_to_string(dir.join(MANIFEST)).unwrap());
        assert_eq!(lines.len(), 2);
        let found = entries(&lines).unwrap();
        assert_eq!(found.len(), 1, "blank lines are not entries");
        assert!(problems(&dir, &found).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn changed_and_missing_files_are_reported_in_order() {
        let dir = tree("bad");
        std::fs::write(dir.join("src/a.rs"), "fn a() { 1 }\n").unwrap();
        let mut manifest = std::fs::read_to_string(dir.join(MANIFEST)).unwrap();
        manifest.push_str("00  src/gone.rs\n");
        let lines = py::splitlines(&manifest);
        assert_eq!(problems(&dir, &entries(&lines).unwrap()), vec!["changed src/a.rs", "missing src/gone.rs"]);
        assert!(entries(&["lonely".to_string()]).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
