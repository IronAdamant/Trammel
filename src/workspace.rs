//! The copy of the project that `verify_step` runs tests in.
//!
//! The copy lives at a stable path per project, under
//! `<temp>/trammel-verify-cache/<hash of the project path>/slot<N>/tree`, and
//! is brought up to date before each run instead of being made from scratch.
//! A file is rewritten only when its bytes differ, so an unchanged file keeps
//! its old modification time and an edited or restored one gets a new one.
//! That is what lets a build tool's own change detection work across runs:
//! `CARGO_TARGET_DIR` points at `slot<N>/target`, so cargo rebuilds only the
//! crates whose files changed since the last verify.
//!
//! Up to four runs on one project can go at once, one per slot; each slot is
//! held by a lock file. When every slot is busy, or the cache is switched off
//! with `TRAMMEL_VERIFY_CACHE=0`, the run gets a fresh copy that is deleted
//! afterwards, as before 4.4. `TRAMMEL_VERIFY_CACHE=<dir>` moves the cache.
//! A project's cache is deleted when it has not been used for 14 days.

use crate::walk::{is_ignored_dir, rel_slash};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SLOTS: usize = 4;
/// A lock older than this belongs to a run that died: its owner's longest
/// allowed run (an hour) plus margin.
const STALE_LOCK: Duration = Duration::from_secs(3 * 3600);
const UNUSED_FOR: Duration = Duration::from_secs(14 * 24 * 3600);

pub struct Workspace {
    /// Where the tests run.
    pub tree: PathBuf,
    /// The build output directory to reuse, when the copy is cached.
    pub target: Option<PathBuf>,
    /// Files rewritten to bring the copy up to date.
    pub files_written: usize,
    lock: Option<PathBuf>,
    manifest_path: Option<PathBuf>,
    manifest: BTreeMap<String, Entry>,
}

/// What a copied file looked like when the copy matched its source.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Entry {
    src: (u64, u128),
    dst: (u64, u128),
}

impl Workspace {
    /// A copy of `root` ready for edits: a cached slot when one is free, else
    /// a fresh directory.
    pub fn prepare(root: &Path) -> Result<Workspace, String> {
        let files = project_files(root);
        if let Some(base) = cache_base() {
            if let Some(ws) = Workspace::cached(root, &base, &files) { return Ok(ws); }
        }
        let tree = fresh_dir();
        let _ = fs::remove_dir_all(&tree);
        copy_files(root, &tree, &files)?;
        Ok(Workspace { tree, target: None, files_written: files.len(), lock: None, manifest_path: None, manifest: BTreeMap::new() })
    }

    fn cached(root: &Path, base: &Path, files: &[String]) -> Option<Workspace> {
        let canonical = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let key = cache_key(&canonical);
        let dir = base.join(&key);
        fs::create_dir_all(&dir).ok()?;
        let _ = fs::write(dir.join("last_used"), now_secs().to_string());
        prune_once(base, &key);
        for n in 0..SLOTS {
            let lock = dir.join(format!("slot{n}.lock"));
            if !take_lock(&lock) { continue; }
            let slot = dir.join(format!("slot{n}"));
            let manifest_path = slot.join("manifest.tsv");
            let mut ws = Workspace {
                tree: slot.join("tree"),
                target: Some(slot.join("target")),
                files_written: 0,
                lock: Some(lock),
                manifest_path: Some(manifest_path.clone()),
                manifest: read_manifest(&manifest_path),
            };
            match ws.sync(root, files) {
                Ok(()) => return Some(ws),
                Err(_) => {
                    // Start this slot over next time.
                    let _ = fs::remove_dir_all(&slot);
                    ws.release();
                    return None;
                }
            }
        }
        None
    }

    /// Make `tree` hold exactly `files` of `root`, rewriting only what differs.
    fn sync(&mut self, root: &Path, files: &[String]) -> Result<(), String> {
        fs::create_dir_all(&self.tree).map_err(|e| e.to_string())?;
        let mut next = BTreeMap::new();
        let wanted: BTreeSet<&str> = files.iter().map(|s| s.as_str()).collect();
        for rel in files {
            let from = root.join(rel);
            let to = self.tree.join(rel);
            let Some(src) = stamp(&from) else { continue };
            if let (Some(old), Some(dst)) = (self.manifest.get(rel), stamp(&to)) {
                if old.src == src && old.dst == dst {
                    next.insert(rel.clone(), *old);
                    continue;
                }
            }
            let bytes = fs::read(&from).map_err(|e| format!("{rel}: {e}"))?;
            if fs::read(&to).ok().as_deref() != Some(bytes.as_slice()) {
                if let Some(parent) = to.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
                // Write the bytes, not fs::copy: Windows' copy keeps the
                // source's modification time, which would hide the change.
                fs::write(&to, &bytes).map_err(|e| format!("{rel}: {e}"))?;
                self.files_written += 1;
            }
            if let Some(dst) = stamp(&to) { next.insert(rel.clone(), Entry { src, dst }); }
        }
        // Whatever the last run left behind that the project does not have.
        for stale in list_tree(&self.tree) {
            if !wanted.contains(stale.as_str()) { let _ = fs::remove_file(self.tree.join(&stale)); }
        }
        self.manifest = next;
        Ok(())
    }

    /// Write one edit into the copy. Its manifest entry is dropped, so the
    /// next run compares the file with the project again and restores it.
    pub fn write_edit(&mut self, rel: &str, content: &str) -> Result<(), String> {
        let clean = crate::walk::norm_slash(rel);
        let escapes = Path::new(rel).is_absolute() || rel.replace('\\', "/").split('/').any(|p| p == "..") || clean.is_empty();
        if escapes { return Err(format!("{rel}: edit paths must be relative to the project and stay inside it")); }
        let dest = self.tree.join(&clean);
        if let Some(parent) = dest.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
        self.manifest.remove(&clean);
        fs::write(&dest, content).map_err(|e| format!("{rel}: {e}"))
    }

    /// Save what the copy now matches and free the slot (cached), or delete
    /// the copy (fresh).
    pub fn finish(mut self) {
        match &self.manifest_path {
            Some(path) => {
                // A test may rewrite files in the copy; the stored stamps
                // catch that next time, because they no longer match.
                let _ = write_manifest(path, &self.manifest);
            }
            None => { let _ = fs::remove_dir_all(&self.tree); }
        }
        self.release();
    }

    pub fn cached_copy(&self) -> bool {
        self.lock.is_some()
    }

    fn release(&mut self) {
        if let Some(lock) = self.lock.take() { let _ = fs::remove_file(lock); }
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        self.release();
    }
}

/// Delete the cached copies of `root` (tests clean up with this).
pub fn forget(root: &Path) {
    let Some(base) = cache_base() else { return };
    let canonical = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let _ = fs::remove_dir_all(base.join(cache_key(&canonical)));
}

fn cache_key(canonical: &Path) -> String {
    crate::hash::sha256_hex(canonical.to_string_lossy().as_bytes())[..16].to_string()
}

/// `TRAMMEL_VERIFY_CACHE`: unset or `1` uses the temp directory, `0` turns
/// the cache off, anything else is the directory to use.
fn cache_base() -> Option<PathBuf> {
    match std::env::var("TRAMMEL_VERIFY_CACHE").ok().as_deref().map(str::trim) {
        Some("0") | Some("off") | Some("false") => None,
        Some("") | Some("1") | None => Some(std::env::temp_dir().join("trammel-verify-cache")),
        Some(dir) => Some(PathBuf::from(dir)),
    }
}

fn take_lock(lock: &Path) -> bool {
    let create = || fs::OpenOptions::new().write(true).create_new(true).open(lock).map(|mut f| {
        use std::io::Write;
        let _ = write!(f, "{} {}", std::process::id(), now_secs());
    });
    if create().is_ok() { return true; }
    let stale = fs::metadata(lock).and_then(|m| m.modified()).ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .map(|age| age > STALE_LOCK)
        .unwrap_or(false);
    stale && fs::remove_file(lock).is_ok() && create().is_ok()
}

/// Delete other projects' caches unused for 14 days, once per process.
fn prune_once(base: &Path, keep: &str) {
    static DONE: std::sync::Once = std::sync::Once::new();
    DONE.call_once(|| {
        let Ok(entries) = fs::read_dir(base) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == keep { continue; }
            let dir = entry.path();
            let unused = fs::metadata(dir.join("last_used")).and_then(|m| m.modified()).ok()
                .and_then(|t| SystemTime::now().duration_since(t).ok())
                .map(|age| age > UNUSED_FOR)
                .unwrap_or(false);
            let busy = (0..SLOTS).any(|n| dir.join(format!("slot{n}.lock")).exists());
            if unused && !busy { let _ = fs::remove_dir_all(&dir); }
        }
    });
}

/// Size and modification time in nanoseconds.
fn stamp(path: &Path) -> Option<(u64, u128)> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() { return None; }
    let mtime = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos();
    Some((meta.len(), mtime))
}

fn read_manifest(path: &Path) -> BTreeMap<String, Entry> {
    let mut out = BTreeMap::new();
    let Ok(text) = fs::read_to_string(path) else { return out };
    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() != 5 { continue; }
        let nums: Vec<u128> = parts[1..].iter().filter_map(|p| p.parse().ok()).collect();
        if nums.len() != 4 { continue; }
        out.insert(parts[0].to_string(), Entry { src: (nums[0] as u64, nums[1]), dst: (nums[2] as u64, nums[3]) });
    }
    out
}

fn write_manifest(path: &Path, manifest: &BTreeMap<String, Entry>) -> std::io::Result<()> {
    let mut text = String::new();
    for (rel, e) in manifest {
        if rel.contains('\t') || rel.contains('\n') { continue; }
        text.push_str(&format!("{rel}\t{}\t{}\t{}\t{}\n", e.src.0, e.src.1, e.dst.0, e.dst.1));
    }
    fs::write(path, text)
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A fresh copy target. Two runs in one process must never share it, or one
/// run deletes the other's tree mid-test.
pub fn fresh_dir() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    std::env::temp_dir().join(format!("trammel-verify-{}-{nanos}-{n}", std::process::id()))
}

/// The files a verify copies, relative and `/`-separated: what git would
/// commit (tracked plus untracked-not-ignored, submodules expanded), or
/// outside git every file except generated directories. The Trammel store is
/// never copied.
pub fn project_files(root: &Path) -> Vec<String> {
    let listed = match git_files(root) {
        Some(files) => {
            let mut out = Vec::new();
            for rel in files {
                let path = root.join(&rel);
                if path.is_dir() {
                    // A submodule: git lists its path, not its files.
                    out.extend(walk_files(&path).into_iter().map(|inner| format!("{rel}/{inner}")));
                } else {
                    out.push(rel);
                }
            }
            out
        }
        None => walk_files(root),
    };
    let mut out: Vec<String> = listed.into_iter().filter(|rel| rel.split('/').next() != Some(".trammel")).collect();
    out.sort();
    out.dedup();
    out
}

/// The files git would commit under `root`: tracked ones plus untracked ones
/// that no .gitignore excludes. None when git is missing or `root` is not in
/// a work tree.
pub fn git_files(root: &Path) -> Option<Vec<String>> {
    let out = Command::new(crate::harness::program("git"))
        .arg("-C").arg(root)
        .args(["ls-files", "-z", "--cached", "--others", "--exclude-standard"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() { return None; }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut files: Vec<String> = text.split('\0').filter(|p| !p.is_empty()).map(String::from).collect();
    files.sort();
    files.dedup();
    Some(files)
}

/// Every file under `dir` except generated directories and the store.
fn walk_files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if path.is_dir() {
                if !is_ignored_dir(&name) && name != ".trammel" { stack.push(path); }
            } else if path.is_file() {
                out.push(rel_slash(&path, dir));
            }
        }
    }
    out.sort();
    out
}

/// Every file in a copy, nothing skipped.
fn list_tree(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() { stack.push(path); } else { out.push(rel_slash(&path, dir)); }
        }
    }
    out
}

/// Copy `files` of `src` into a new directory `dst`.
pub fn copy_files(src: &Path, dst: &Path, files: &[String]) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for rel in files {
        let from = src.join(rel);
        if !from.is_file() { continue; }
        let to = dst.join(rel);
        if let Some(parent) = to.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
        fs::copy(&from, &to).map_err(|e| format!("{rel}: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("trammel-ws-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn the_copy_is_reused_and_only_changed_files_are_rewritten() {
        let src = scratch("reuse-src");
        let cache = scratch("reuse-cache");
        for rel in ["src/lib.rs", "src/a.rs", "src/b.rs", "README.md"] { write(&src, rel, rel); }
        let files = walk_files(&src);
        let mut first = Workspace::cached(&src, &cache, &files).expect("a free slot");
        assert_eq!(first.files_written, 4);
        let tree = first.tree.clone();
        let stamp_b = stamp(&tree.join("src/b.rs")).unwrap();
        first.write_edit("src/a.rs", "edited").unwrap();
        assert!(first.write_edit("../outside.rs", "x").is_err(), "edits stay inside the copy");
        // A second run while the first holds slot 0 gets another slot.
        let second = Workspace::cached(&src, &cache, &files).expect("slot 1");
        assert_ne!(second.tree, tree);
        second.finish();
        first.finish();
        // The next run in slot 0 restores the edit and touches nothing else.
        write(&src, "src/new.rs", "new");
        fs::remove_file(src.join("README.md")).unwrap();
        let files = walk_files(&src);
        let again = Workspace::cached(&src, &cache, &files).unwrap();
        assert_eq!(again.tree, tree);
        assert_eq!(again.files_written, 2, "the restored edit and the new file");
        assert_eq!(fs::read_to_string(tree.join("src/a.rs")).unwrap(), "src/a.rs");
        assert_eq!(stamp(&tree.join("src/b.rs")).unwrap(), stamp_b, "an unchanged file keeps its modification time");
        assert!(!tree.join("README.md").exists(), "a file the project lost is removed");
        assert!(tree.join("src/new.rs").exists());
        assert_eq!(again.target.as_deref(), Some(tree.parent().unwrap().join("target").as_path()));
        again.finish();
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn a_file_a_test_rewrote_in_the_copy_is_restored() {
        let src = scratch("rewrite-src");
        let cache = scratch("rewrite-cache");
        write(&src, "golden.txt", "want");
        let files = walk_files(&src);
        let ws = Workspace::cached(&src, &cache, &files).unwrap();
        let tree = ws.tree.clone();
        ws.finish();
        // Same size, different bytes, written by "the test".
        fs::write(tree.join("golden.txt"), "oops").unwrap();
        let ws = Workspace::cached(&src, &cache, &files).unwrap();
        assert_eq!(fs::read_to_string(tree.join("golden.txt")).unwrap(), "want");
        ws.finish();
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn a_stale_lock_is_taken_over_and_a_live_one_is_not() {
        let dir = scratch("locks");
        let lock = dir.join("slot0.lock");
        assert!(take_lock(&lock));
        assert!(!take_lock(&lock), "held");
        fs::remove_file(&lock).unwrap();
        assert!(take_lock(&lock));
        let old = SystemTime::now() - STALE_LOCK - Duration::from_secs(60);
        fs::OpenOptions::new().write(true).open(&lock).unwrap().set_modified(old).unwrap();
        assert!(take_lock(&lock), "a lock left by a dead run expires");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_fresh_copies_get_their_own_directory() {
        assert_ne!(fresh_dir(), fresh_dir());
    }
}
