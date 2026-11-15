//! A clean checkout of one commit for `verify_step` with `ref`, made the way
//! `git checkout` would make it (line endings and other `.gitattributes`
//! filters applied, with this repository's config) without touching the
//! project's index, HEAD or worktree list: the commit is read into a
//! temporary index file and `git checkout-index` writes its files under a
//! staging directory.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn git(dir: &Path) -> Command {
    let mut cmd = Command::new(crate::harness::program("git"));
    cmd.arg("-C").arg(dir).stdin(Stdio::null());
    cmd
}

fn text(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// The full commit id `reference` names in the repository holding `root`.
pub fn resolve(root: &Path, reference: &str) -> Result<String, String> {
    if reference.trim().is_empty() || reference.starts_with('-') {
        return Err(format!("ref {reference:?} is not a commit name"));
    }
    let out = git(root).args(["rev-parse", "--verify", "--quiet", &format!("{reference}^{{commit}}")]).output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if !out.status.success() {
        let inside = git(root).args(["rev-parse", "--is-inside-work-tree"]).output().map(|o| o.status.success()).unwrap_or(false);
        if !inside { return Err(format!("{} is not in a git work tree, so ref {reference:?} cannot be checked out", root.display())); }
        return Err(format!("ref {reference:?} names no commit in this repository"));
    }
    Ok(text(&out))
}

/// Write the files of `commit` that lie under `root` into `staging`. Returns
/// the directory that corresponds to `root` inside `staging` and the files,
/// relative to it with `/`.
pub fn checkout(root: &Path, commit: &str, staging: &Path) -> Result<(PathBuf, Vec<String>), String> {
    let top = git(root).args(["rev-parse", "--show-toplevel"]).output().map_err(|e| e.to_string())?;
    if !top.status.success() { return Err(format!("{} is not in a git work tree", root.display())); }
    let top = PathBuf::from(text(&top));
    let prefix = git(root).args(["rev-parse", "--show-prefix"]).output().map(|o| text(&o)).unwrap_or_default();
    let _ = std::fs::remove_dir_all(staging);
    std::fs::create_dir_all(staging).map_err(|e| format!("{}: {e}", staging.display()))?;
    let index = staging.with_extension("index");
    let _ = std::fs::remove_file(&index);
    let result = (|| {
        let read = git(&top).env("GIT_INDEX_FILE", &index).args(["read-tree", commit]).output().map_err(|e| e.to_string())?;
        if !read.status.success() { return Err(format!("git read-tree {commit}: {}", String::from_utf8_lossy(&read.stderr).trim())); }
        let mut list = git(&top);
        list.env("GIT_INDEX_FILE", &index).args(["ls-files", "-z", "--"]);
        if !prefix.is_empty() { list.arg(&prefix); }
        let listed = list.output().map_err(|e| e.to_string())?;
        if !listed.status.success() { return Err(format!("git ls-files: {}", String::from_utf8_lossy(&listed.stderr).trim())); }
        let paths: Vec<String> = String::from_utf8_lossy(&listed.stdout).split('\0').filter(|p| !p.is_empty()).map(String::from).collect();
        let dest = format!("{}/", staging.display().to_string().replace('\\', "/").trim_end_matches('/'));
        let mut child = git(&top).env("GIT_INDEX_FILE", &index)
            .args(["checkout-index", "-f", "-z", "--stdin"]).arg(format!("--prefix={dest}"))
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped())
            .spawn().map_err(|e| e.to_string())?;
        if let Some(mut input) = child.stdin.take() {
            for path in &paths { input.write_all(path.as_bytes()).and_then(|_| input.write_all(b"\0")).map_err(|e| e.to_string())?; }
        }
        let done = child.wait_with_output().map_err(|e| e.to_string())?;
        if !done.status.success() { return Err(format!("git checkout-index: {}", String::from_utf8_lossy(&done.stderr).trim())); }
        let files = paths.iter().filter_map(|p| p.strip_prefix(prefix.as_str()).map(String::from)).collect();
        Ok(files)
    })();
    let _ = std::fs::remove_file(&index);
    let files = result?;
    let origin = if prefix.is_empty() { staging.to_path_buf() } else { staging.join(prefix.trim_end_matches('/')) };
    Ok((origin, files))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        let out = Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "core.autocrlf=false"]).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn a_checkout_applies_gitattributes_and_leaves_the_repository_alone() {
        let dir = std::env::temp_dir().join(format!("trammel-gitref-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        run(&dir, &["init", "-q"]);
        std::fs::write(dir.join(".gitattributes"), "*.txt text eol=crlf\n").unwrap();
        std::fs::write(dir.join("sub/guard.txt"), "a\nb\n").unwrap();
        std::fs::write(dir.join("top.rs"), "fn main() {}\n").unwrap();
        run(&dir, &["add", "."]);
        run(&dir, &["commit", "-q", "-m", "one"]);
        std::fs::write(dir.join("untracked.rs"), "x").unwrap();
        let index_before = std::fs::read(dir.join(".git/index")).unwrap();
        let commit = resolve(&dir, "HEAD").unwrap();
        assert_eq!(commit.len(), 40);
        assert!(resolve(&dir, "no-such-branch").unwrap_err().contains("names no commit"));
        assert!(resolve(&dir, "--all").is_err(), "an option is not a ref");
        let staging = dir.with_extension("stage");
        let (origin, files) = checkout(&dir, &commit, &staging).unwrap();
        assert_eq!(files, vec![".gitattributes", "sub/guard.txt", "top.rs"]);
        assert_eq!(std::fs::read(origin.join("sub/guard.txt")).unwrap(), b"a\r\nb\r\n", "eol=crlf applied as a checkout would");
        assert_eq!(std::fs::read(dir.join("sub/guard.txt")).unwrap(), b"a\nb\n", "the working tree is untouched");
        assert_eq!(std::fs::read(dir.join(".git/index")).unwrap(), index_before, "the project's index is untouched");
        // From a subdirectory only its files come back, relative to it.
        let (origin, files) = checkout(&dir.join("sub"), &commit, &staging).unwrap();
        assert_eq!(files, vec!["guard.txt"]);
        assert!(origin.join("guard.txt").is_file());
        let _ = std::fs::remove_dir_all(&staging);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
