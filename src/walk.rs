//! Project walk. Ignored directory names match Python `utils._IGNORED_DIRS`,
//! plus `.claude`, where Claude Code keeps whole-repo agent worktrees.
//! Relative paths always use `/`, including on Windows.

use std::fs;
use std::path::{Path, PathBuf};

const IGNORED: &[&str] = &[
    ".git",
    "__pycache__",
    ".pytest_cache",
    "venv",
    ".venv",
    "node_modules",
    ".tox",
    ".mypy_cache",
    ".ruff_cache",
    ".chisel",
    ".stele-context",
    "dist",
    "build",
    ".next",
    ".nuxt",
    "coverage",
    ".turbo",
    ".parcel-cache",
    "sample_file_test",
    "target",
    ".claude",
];

pub fn is_ignored_dir(name: &str) -> bool {
    IGNORED.contains(&name) || name.ends_with(".egg-info")
}

pub fn to_slash(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn rel_slash(path: &Path, root: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    to_slash(rel)
}

pub fn norm_slash(path: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();
    let normalized = path.replace('\\', "/");
    for part in normalized.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            stack.pop();
        } else {
            stack.push(part);
        }
    }
    stack.join("/")
}

pub fn read_lossy(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

pub struct SourceFile {
    pub rel: String,
    pub source: String,
}

pub fn walk_sources(root: &Path, extensions: &[&str]) -> Vec<SourceFile> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut children: Vec<PathBuf> = Vec::new();
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        let mut files = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if !is_ignored_dir(&name) {
                    children.push(path);
                }
            } else if extensions.iter().any(|ext| name.ends_with(ext)) {
                files.push(path);
            }
        }
        files.sort();
        children.sort();
        for path in files {
            if let Some(source) = read_lossy(&path) {
                out.push(SourceFile {
                    rel: rel_slash(&path, root),
                    source,
                });
            }
        }
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }
    out
}

pub fn collect_files(root: &Path, extensions: &[&str]) -> Vec<String> {
    walk_sources(root, extensions)
        .into_iter()
        .map(|file| file.rel)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_worktrees_under_dot_claude_are_not_project_files() {
        let dir = std::env::temp_dir().join(format!("trammel-walk-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".claude/worktrees/agent-1/pkg")).unwrap();
        fs::create_dir_all(dir.join("pkg")).unwrap();
        fs::write(dir.join("pkg/app.py"), "x = 1\n").unwrap();
        fs::write(dir.join(".claude/worktrees/agent-1/pkg/app.py"), "x = 1\n").unwrap();
        assert_eq!(collect_files(&dir, &[".py"]), vec!["pkg/app.py".to_string()]);
        let _ = fs::remove_dir_all(&dir);
    }
}
