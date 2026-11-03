//! Project root: nearest ancestor with `.git` or `.trammel.json`.

use std::path::{Path, PathBuf};

pub fn find_root(start: &Path) -> PathBuf {
    let mut current = if start.is_file() {
        start.parent().unwrap_or(start).to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        if current.join(".git").exists() || current.join(".trammel.json").is_file() {
            return current;
        }
        if !current.pop() {
            break;
        }
    }
    if start.is_dir() {
        start.to_path_buf()
    } else {
        start.parent().unwrap_or(start).to_path_buf()
    }
}

pub fn store_dir(project: &Path, explicit: Option<&Path>) -> PathBuf {
    if let Some(path) = explicit {
        if path.extension().and_then(|e| e.to_str()) == Some("db") {
            return path.parent().unwrap_or(path).join(".trammel");
        }
        return path.to_path_buf();
    }
    project.join(".trammel")
}

pub fn legacy_db(project: &Path, explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        if path.is_file() && looks_sqlite(path) {
            return Some(path.to_path_buf());
        }
    }
    let candidate = project.join("trammel.db");
    if candidate.is_file() && looks_sqlite(&candidate) {
        Some(candidate)
    } else {
        None
    }
}

fn looks_sqlite(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    bytes.len() >= 16 && &bytes[..16] == b"SQLite format 3\0"
}
