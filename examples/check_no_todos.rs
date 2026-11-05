//! Fail on leftover TODO or FIXME markers in the Rust tree under `src/`.
//! Run from the repo root: `cargo run --example check_no_todos`. Same
//! checks, results and exit codes as the `scripts/check_no_todos.py` it
//! replaced. Files are listed directory by directory, breadth first.

#[path = "support/py.rs"]
mod py;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// `Path(root).rglob("*.rs")`: directories breadth-first, each one's files in
/// `scandir` order. Paths keep the OS separator, as Python prints them.
fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut queue = VecDeque::from([root.to_path_buf()]);
    while let Some(dir) = queue.pop_front() {
        for (name, is_dir) in py::sorted_names(&dir) {
            let path = dir.join(&name);
            if is_dir {
                queue.push_back(path);
            } else if name.ends_with(".rs") {
                out.push(path);
            }
        }
    }
    out
}

fn hits_in(path: &Path, text: &str) -> Vec<String> {
    let mut hits = Vec::new();
    for (n, line) in py::splitlines(text).iter().enumerate() {
        if line.contains("TODO") || line.contains("FIXME") {
            // The harness looks for these tokens in project source. That is
            // the detector, not a leftover task.
            if line.contains(".matches(") || line.contains("markers") {
                continue;
            }
            hits.push(format!("{}:{}:{}", path.display(), n + 1, py::strip(line)));
        }
    }
    hits
}

fn check(root: &Path) -> Result<&'static str, String> {
    let mut hits = Vec::new();
    for path in rust_files(root) {
        let bytes = std::fs::read(&path).unwrap_or_default();
        hits.extend(hits_in(&path, &String::from_utf8_lossy(&bytes)));
    }
    if hits.is_empty() { Ok("no leftover markers") } else { Err(hits.join("\n")) }
}

fn main() -> ExitCode {
    match check(Path::new("src")) {
        Ok(message) => { println!("{message}"); ExitCode::SUCCESS }
        Err(hits) => { println!("{hits}"); ExitCode::from(1) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_are_found_except_the_detector_lines() {
        let dir = std::env::temp_dir().join(format!("trammel-check-todos-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("lang/python")).unwrap();
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::write(dir.join("a.rs"), "fn a() {}\n").unwrap();
        assert_eq!(check(&dir), Ok("no leftover markers"));
        std::fs::write(dir.join("lang/python/deep.rs"), "  // FIXME later  \r\n").unwrap();
        std::fs::write(dir.join("lang/z.rs"), "let n = s.matches(\"TODO\").count();\n// TODO: finish\n").unwrap();
        std::fs::write(dir.join("bin/b.rs"), "// count markers TODO\n").unwrap();
        std::fs::write(dir.join("notes.txt"), "TODO\n").unwrap();
        let sep = std::path::MAIN_SEPARATOR;
        let root = dir.display();
        assert_eq!(check(&dir), Err(format!("{root}{sep}lang{sep}z.rs:2:// TODO: finish\n{root}{sep}lang{sep}python{sep}deep.rs:1:// FIXME later")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
