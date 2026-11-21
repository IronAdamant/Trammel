//! The copy of the project that `verify_step` runs tests in.
//!
//! The copy lives at a stable path per project, under
//! `<temp>/trv/<hash of the project path>/<N>/t` (`<N>` is the slot), and
//! is brought up to date before each run instead of being made from scratch.
//! The layout is short on purpose: Windows cannot open a path past 259
//! characters, and cargo's own files under the target dir add about 120.
//! A file is rewritten only when its bytes differ, so an unchanged file keeps
//! its old modification time and an edited or restored one gets a new one.
//! That is what lets a build tool's own change detection work across runs:
//! `CARGO_TARGET_DIR` points at `<N>/b`, so cargo rebuilds only the crates
//! whose files changed since the last verify. A file is skipped unread only
//! when its size and modification time match the manifest and that time is
//! more than two seconds older than when the stamps were taken; any other is
//! compared byte for byte (git's racy-timestamp rule), so a same-size rewrite
//! within one clock tick is still seen.
//!
//! The copy holds the working tree (what git would commit) or, with a
//! `ref`, a clean checkout of one commit, staged in `<N>/c` by
//! `crate::gitref` and synced into the tree the same way.
//!
//! Up to four runs on one project can go at once, one per slot; each slot is
//! held by a lock file. When every slot is busy, or the cache is switched off
//! with `TRAMMEL_VERIFY_CACHE=0`, the run gets a fresh copy that is deleted
//! afterwards, as before 4.4. `TRAMMEL_VERIFY_CACHE=<dir>` moves the cache.
//! A project's cache is deleted when it has not been used for 14 days; so is
//! one left by 4.4 under `<temp>/trammel-verify-cache`.
//!
//! After the run, `left_behind` compares the copy with what was put there.

use crate::pathlen::{self, PathLen};
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
/// Paths listed in `left_behind`.
const LEFT_LISTED: usize = 5;
/// A stamp is trusted only when the file's modification time is at least
/// this much older than the moment the stamp was recorded (git's "racy
/// timestamp" rule). A file written in the same clock tick as the stamp was
/// taken, then rewritten with the same length, keeps an equal stamp: such a
/// file is compared byte for byte instead. Two seconds covers coarse clocks
/// (Windows' 15.6 ms tick, FAT's 2 s mtimes).
const RACY_MARGIN_NS: u128 = 2_000_000_000;

/// What the copy holds.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// The project's files as they are now: tracked plus untracked ones no
    /// .gitignore excludes.
    WorkingTree,
    /// A clean checkout of this commit (full id): tracked files only.
    Commit(String),
}

#[derive(Debug)]
pub enum PrepareError {
    Failed(String),
    /// The longest path a build would create passes Windows' limit.
    PathTooLong { tree: PathBuf, len: PathLen },
}

pub struct Workspace {
    /// Where the tests run.
    pub tree: PathBuf,
    /// The build output directory to reuse, when the copy is cached.
    pub target: Option<PathBuf>,
    /// Files rewritten to bring the copy up to date.
    pub files_written: usize,
    /// The longest path a build would create under the copy.
    pub path_len: PathLen,
    /// Where the copied bytes come from: the project, or the staged checkout.
    origin: PathBuf,
    /// The staged checkout, deleted when the run is over.
    staging: Option<PathBuf>,
    /// What each file looked like when the test command started.
    put: BTreeMap<String, Put>,
    lock: Option<PathBuf>,
    manifest_path: Option<PathBuf>,
    manifest: BTreeMap<String, Entry>,
    /// When the stamps in `manifest` and `put` were taken (nanoseconds since
    /// the epoch, read just before the sync started). 0: unknown.
    recorded: u128,
    /// Files read to compare their bytes during the sync.
    #[cfg(test)]
    compared: usize,
}

/// What a copied file looked like when the copy matched its source.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Entry {
    src: (u64, u128),
    dst: (u64, u128),
}

/// A file as it was put into the copy.
struct Put {
    dst: (u64, u128),
    /// The source's stamp then; None for an edit.
    src: Option<(u64, u128)>,
    /// An edit's bytes (its source is the call, not a file).
    edit: Option<Vec<u8>>,
}

/// What a run left in the copy that was not there before it.
#[derive(Debug, Default, PartialEq)]
pub struct LeftBehind {
    pub created: Vec<String>,
    /// Path and, for a change of line endings only, what they are now.
    pub modified: Vec<(String, Option<&'static str>)>,
    pub deleted: Vec<String>,
}

impl LeftBehind {
    pub fn value(&self) -> crate::json::Value {
        use crate::json::Value;
        let row = |path: &str, change: &str| crate::canon::obj(vec![("path", Value::string(path)), ("change", Value::string(change))]);
        let eol_only = self.modified.iter().filter(|(_, eol)| eol.is_some()).count();
        let mut files: Vec<Value> = self.created.iter().map(|p| row(p, "created")).collect();
        for (path, eol) in &self.modified {
            let mut one = row(path, if eol.is_some() { "line_endings" } else { "modified" });
            if let Some(now) = eol { crate::canon::insert(&mut one, "now", Value::string(*now)); }
            files.push(one);
        }
        files.extend(self.deleted.iter().map(|p| row(p, "deleted")));
        let total = files.len();
        files.truncate(LEFT_LISTED);
        let mut out = crate::canon::obj(vec![
            ("created", Value::Int(self.created.len() as i64)),
            ("modified", Value::Int(self.modified.len() as i64)),
            ("deleted", Value::Int(self.deleted.len() as i64)),
        ]);
        if eol_only > 0 { crate::canon::insert(&mut out, "line_endings_only", Value::Int(eol_only as i64)); }
        if total > 0 { crate::canon::insert(&mut out, "files", Value::Array(files)); }
        if total > LEFT_LISTED { crate::canon::insert(&mut out, "files_more", Value::Int((total - LEFT_LISTED) as i64)); }
        out
    }
}

impl Workspace {
    /// A copy of `source` ready for edits: a cached slot when one is free,
    /// else a fresh directory.
    pub fn prepare(root: &Path, source: &Source) -> Result<Workspace, PrepareError> {
        if let Some(base) = cache_base() {
            if let Some(ws) = Workspace::cached(root, &base, source)? { return Ok(ws); }
        }
        let tree = fresh_dir();
        let staging = matches!(source, Source::Commit(_)).then(|| suffixed(&tree, "c"));
        let (origin, files) = gather(root, source, staging.as_deref()).map_err(PrepareError::Failed)?;
        let path_len = pathlen::estimate(&tree, &tree.join("target"), &files, &origin);
        let mut ws = Workspace {
            tree, target: None, files_written: 0, path_len, origin, staging, put: BTreeMap::new(), lock: None, manifest_path: None, manifest: BTreeMap::new(), recorded: 0,
            #[cfg(test)]
            compared: 0,
        };
        ws.refuse_if_too_long()?;
        let _ = fs::remove_dir_all(&ws.tree);
        ws.recorded = now_nanos();
        copy_files(&ws.origin, &ws.tree, &files).map_err(PrepareError::Failed)?;
        for rel in &files {
            if let (Some(src), Some(dst)) = (stamp(&ws.origin.join(rel)), stamp(&ws.tree.join(rel))) {
                ws.put.insert(rel.clone(), Put { dst, src: Some(src), edit: None });
            }
        }
        ws.files_written = ws.put.len();
        Ok(ws)
    }

    /// A cached slot, or None when every slot is busy or the slot could not
    /// be brought up to date (the caller then makes a fresh copy).
    fn cached(root: &Path, base: &Path, source: &Source) -> Result<Option<Workspace>, PrepareError> {
        let canonical = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let key = cache_key(&canonical);
        let dir = base.join(&key);
        if fs::create_dir_all(&dir).is_err() { return Ok(None); }
        let _ = fs::write(dir.join("last_used"), now_secs().to_string());
        prune_once(base, &key);
        for n in 0..SLOTS {
            let lock = dir.join(format!("{n}.lock"));
            if !take_lock(&lock) { continue; }
            let slot = dir.join(n.to_string());
            let staging = matches!(source, Source::Commit(_)).then(|| slot.join("c"));
            let manifest_path = slot.join("manifest.tsv");
            let (origin, files) = match gather(root, source, staging.as_deref()) {
                Ok(found) => found,
                Err(err) => {
                    if let Some(staging) = &staging { let _ = fs::remove_dir_all(staging); }
                    let _ = fs::remove_file(&lock);
                    return Err(PrepareError::Failed(err));
                }
            };
            let tree = slot.join("t");
            let target = slot.join("b");
            let path_len = pathlen::estimate(&tree, &target, &files, &origin);
            let (manifest, recorded) = read_manifest(&manifest_path);
            let mut ws = Workspace {
                tree,
                target: Some(target),
                files_written: 0,
                path_len,
                origin,
                staging,
                put: BTreeMap::new(),
                lock: Some(lock),
                manifest_path: Some(manifest_path.clone()),
                manifest,
                recorded,
                #[cfg(test)]
                compared: 0,
            };
            ws.refuse_if_too_long()?;
            match ws.sync(&files) {
                Ok(()) => return Ok(Some(ws)),
                Err(_) => {
                    // Start this slot over next time.
                    let _ = fs::remove_dir_all(&slot);
                    ws.release();
                    return Ok(None);
                }
            }
        }
        Ok(None)
    }

    fn refuse_if_too_long(&self) -> Result<(), PrepareError> {
        if pathlen::classify(self.path_len.longest, cfg!(windows)) == pathlen::Check::Over {
            return Err(PrepareError::PathTooLong { tree: self.tree.clone(), len: self.path_len.clone() });
        }
        Ok(())
    }

    /// Make `tree` hold exactly `files` of the origin, rewriting only what
    /// differs. A file whose stamps match the manifest and are not racy is
    /// trusted without reading it; any other is compared byte for byte.
    fn sync(&mut self, files: &[String]) -> Result<(), String> {
        let started = now_nanos();
        let before = self.recorded;
        fs::create_dir_all(&self.tree).map_err(|e| e.to_string())?;
        let mut next = BTreeMap::new();
        let wanted: BTreeSet<&str> = files.iter().map(|s| s.as_str()).collect();
        for rel in files {
            let from = self.origin.join(rel);
            let to = self.tree.join(rel);
            let Some(src) = stamp(&from) else { continue };
            if let (Some(old), Some(dst)) = (self.manifest.get(rel), stamp(&to)) {
                if old.src == src && old.dst == dst && trusted(src, before) && trusted(dst, before) {
                    next.insert(rel.clone(), *old);
                    continue;
                }
            }
            #[cfg(test)]
            { self.compared += 1; }
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
        // Whatever the last run left behind that the source does not have.
        for stale in list_tree(&self.tree) {
            if wanted.contains(stale.as_str()) { continue; }
            let path = self.tree.join(&stale);
            let _ = fs::remove_file(&path);
            // And the folders it leaves empty, so the tree looks like a checkout.
            let mut dir = path.parent();
            while let Some(d) = dir.filter(|d| *d != self.tree.as_path()) {
                if fs::remove_dir(d).is_err() { break; }
                dir = d.parent();
            }
        }
        self.put = next.iter().map(|(rel, e)| (rel.clone(), Put { dst: e.dst, src: Some(e.src), edit: None })).collect();
        self.manifest = next;
        self.recorded = started;
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
        fs::write(&dest, content).map_err(|e| format!("{rel}: {e}"))?;
        if let Some(dst) = stamp(&dest) { self.put.insert(clean, Put { dst, src: None, edit: Some(content.as_bytes().to_vec()) }); }
        Ok(())
    }

    /// Files the run created, changed or removed in the copy. Files under a
    /// generated directory (`target`, `node_modules`, `__pycache__`, ...)
    /// that the source does not have are build output and do not count.
    pub fn left_behind(&self) -> LeftBehind {
        let mut out = LeftBehind::default();
        let now = self.walk_for_left();
        for rel in &now {
            let Some(put) = self.put.get(rel) else {
                let generated = rel.rsplit_once('/').map(|(dirs, _)| dirs.split('/').any(is_ignored_dir)).unwrap_or(false);
                if !generated { out.created.push(rel.clone()); }
                continue;
            };
            let path = self.tree.join(rel);
            if stamp(&path) == Some(put.dst) && trusted(put.dst, self.recorded) { continue; }
            let expected = match &put.edit {
                Some(bytes) => Some(bytes.clone()),
                // A source changed since the sync cannot say what was put there.
                None => {
                    let from = self.origin.join(rel);
                    if put.src.is_some() && stamp(&from) != put.src { None } else { fs::read(&from).ok() }
                }
            };
            let Some(expected) = expected else { continue };
            let actual = fs::read(&path).unwrap_or_default();
            if actual == expected { continue; }
            let eol = (lf_only(&actual) == lf_only(&expected)).then(|| eol_kind(&actual));
            out.modified.push((rel.clone(), eol));
        }
        out.deleted = self.put.keys().filter(|rel| !now.contains(*rel)).cloned().collect();
        out
    }

    /// Every file in the tree, without descending into a generated directory
    /// the source has no files in.
    fn walk_for_left(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut stack = vec![self.tree.clone()];
        while let Some(current) = stack.pop() {
            let Ok(entries) = fs::read_dir(&current) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                let rel = rel_slash(&path, &self.tree);
                if path.is_dir() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let prefix = format!("{rel}/");
                    let source_has = self.put.range(prefix.clone()..).next().map(|(k, _)| k.starts_with(&prefix)).unwrap_or(false);
                    if !is_ignored_dir(&name) || source_has { stack.push(path); }
                } else {
                    out.insert(rel);
                }
            }
        }
        out
    }

    /// Save what the copy now matches and free the slot (cached), or delete
    /// the copy (fresh).
    pub fn finish(mut self) {
        match &self.manifest_path {
            Some(path) => {
                // A test may rewrite files in the copy; the stored stamps
                // catch that next time, because they no longer match.
                let _ = write_manifest(path, &self.manifest, self.recorded);
            }
            None => { let _ = fs::remove_dir_all(&self.tree); }
        }
        self.release();
    }

    pub fn cached_copy(&self) -> bool {
        self.lock.is_some()
    }

    fn release(&mut self) {
        if let Some(staging) = self.staging.take() { let _ = fs::remove_dir_all(staging); }
        if let Some(lock) = self.lock.take() { let _ = fs::remove_file(lock); }
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        self.release();
    }
}

/// The source's files and where to read them: the project itself, or a
/// checkout of the commit staged in `staging`.
fn gather(root: &Path, source: &Source, staging: Option<&Path>) -> Result<(PathBuf, Vec<String>), String> {
    match (source, staging) {
        (Source::Commit(commit), Some(staging)) => crate::gitref::checkout(root, commit, staging),
        _ => Ok((root.to_path_buf(), project_files(root))),
    }
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// The bytes with every CRLF turned into LF.
fn lf_only(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'\r' && bytes.get(i + 1) == Some(&b'\n') { continue; }
        out.push(*b);
    }
    out
}

fn eol_kind(bytes: &[u8]) -> &'static str {
    let crlf = bytes.windows(2).filter(|w| *w == b"\r\n").count();
    let lf = bytes.iter().filter(|b| **b == b'\n').count();
    if crlf == 0 { "LF" } else if crlf == lf { "CRLF" } else { "mixed" }
}

/// Delete the cached copies of `root` (tests clean up with this).
pub fn forget(root: &Path) {
    let Some(base) = cache_base() else { return };
    let canonical = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let _ = fs::remove_dir_all(base.join(cache_key(&canonical)));
}

/// The per-project directory beside the cached copies, for state such as
/// the verify cache. None when `TRAMMEL_VERIFY_CACHE` turns caching off.
pub fn state_dir(root: &Path) -> Option<PathBuf> {
    let canonical = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    Some(cache_base()?.join(cache_key(&canonical)))
}

/// Size and modification time (nanoseconds) of a file, for change checks.
pub fn file_stamp(path: &Path) -> Option<(u64, u128)> {
    stamp(path)
}

/// Eight hex digits of the project path's hash. Two projects that share a
/// key share a cache: each run still syncs the copy to its own files.
fn cache_key(canonical: &Path) -> String {
    crate::hash::sha256_hex(canonical.to_string_lossy().as_bytes())[..8].to_string()
}

/// `TRAMMEL_VERIFY_CACHE`: unset or `1` uses the temp directory, `0` turns
/// the cache off, anything else is the directory to use.
fn cache_base() -> Option<PathBuf> {
    match std::env::var("TRAMMEL_VERIFY_CACHE").ok().as_deref().map(str::trim) {
        Some("0") | Some("off") | Some("false") => None,
        Some("") | Some("1") | None => Some(default_base()),
        Some(dir) => Some(PathBuf::from(dir)),
    }
}

fn default_base() -> PathBuf {
    std::env::temp_dir().join("trv")
}

/// Where 4.4 kept its cache.
fn legacy_base() -> PathBuf {
    std::env::temp_dir().join("trammel-verify-cache")
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

/// Delete other projects' caches unused for 14 days, once per process, and
/// 4.4's caches under the old directory by the same rule.
fn prune_once(base: &Path, keep: &str) {
    static DONE: std::sync::Once = std::sync::Once::new();
    DONE.call_once(|| {
        prune_unused(base, Some(keep), |dir, n| dir.join(format!("{n}.lock")));
        if base == default_base() {
            prune_unused(&legacy_base(), None, |dir, n| dir.join(format!("slot{n}.lock")));
        }
    });
}

fn prune_unused(base: &Path, keep: Option<&str>, lock_of: impl Fn(&Path, usize) -> PathBuf) {
    let Ok(entries) = fs::read_dir(base) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if Some(name.as_str()) == keep { continue; }
        let dir = entry.path();
        let unused = fs::metadata(dir.join("last_used")).and_then(|m| m.modified()).ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .map(|age| age > UNUSED_FOR)
            .unwrap_or(false);
        let busy = (0..SLOTS).any(|n| lock_of(&dir, n).exists());
        if unused && !busy { let _ = fs::remove_dir_all(&dir); }
    }
}

/// Size and modification time in nanoseconds.
fn stamp(path: &Path) -> Option<(u64, u128)> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() { return None; }
    let mtime = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos();
    Some((meta.len(), mtime))
}

/// Whether a stamp can stand for the file's bytes: its modification time is
/// safely older than `recorded`, when the stamp was taken. Otherwise a change
/// within the same clock tick could have kept the same size and time.
fn trusted(stamp: (u64, u128), recorded: u128) -> bool {
    stamp.1.saturating_add(RACY_MARGIN_NS) < recorded
}

/// The manifest and when its stamps were taken. A manifest from 4.5.0 or
/// older has no `#recorded` line: 0, so every entry is compared once.
fn read_manifest(path: &Path) -> (BTreeMap<String, Entry>, u128) {
    let mut out = BTreeMap::new();
    let mut recorded = 0;
    let Ok(text) = fs::read_to_string(path) else { return (out, recorded) };
    for line in text.lines() {
        if let Some(when) = line.strip_prefix("#recorded\t") {
            recorded = when.trim().parse().unwrap_or(0);
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() != 5 { continue; }
        let nums: Vec<u128> = parts[1..].iter().filter_map(|p| p.parse().ok()).collect();
        if nums.len() != 4 { continue; }
        out.insert(parts[0].to_string(), Entry { src: (nums[0] as u64, nums[1]), dst: (nums[2] as u64, nums[3]) });
    }
    (out, recorded)
}

fn write_manifest(path: &Path, manifest: &BTreeMap<String, Entry>, recorded: u128) -> std::io::Result<()> {
    let mut text = format!("#recorded\t{recorded}\n");
    for (rel, e) in manifest {
        if rel.contains('\t') || rel.contains('\n') { continue; }
        text.push_str(&format!("{rel}\t{}\t{}\t{}\t{}\n", e.src.0, e.src.1, e.dst.0, e.dst.1));
    }
    fs::write(path, text)
}

fn now_nanos() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A fresh copy target, short like the cached layout. Two runs in one
/// process must never share it, or one run deletes the other's tree mid-test.
pub fn fresh_dir() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("trv-{}-{n}", std::process::id()))
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

    fn slot(src: &Path, cache: &Path) -> Workspace {
        Workspace::cached(src, cache, &Source::WorkingTree).expect("no error").expect("a free slot")
    }

    #[test]
    fn the_copy_is_reused_and_only_changed_files_are_rewritten() {
        let src = scratch("reuse-src");
        let cache = scratch("reuse-cache");
        for rel in ["src/lib.rs", "src/a.rs", "src/b.rs", "README.md"] { write(&src, rel, rel); }
        let mut first = slot(&src, &cache);
        assert_eq!(first.files_written, 4);
        let tree = first.tree.clone();
        let stamp_b = stamp(&tree.join("src/b.rs")).unwrap();
        first.write_edit("src/a.rs", "edited").unwrap();
        assert!(first.write_edit("../outside.rs", "x").is_err(), "edits stay inside the copy");
        // A second run while the first holds slot 0 gets another slot.
        let second = slot(&src, &cache);
        assert_ne!(second.tree, tree);
        second.finish();
        first.finish();
        // The next run in slot 0 restores the edit and touches nothing else.
        write(&src, "src/new.rs", "new");
        fs::remove_file(src.join("README.md")).unwrap();
        let again = slot(&src, &cache);
        assert_eq!(again.tree, tree);
        assert_eq!(again.files_written, 2, "the restored edit and the new file");
        assert_eq!(fs::read_to_string(tree.join("src/a.rs")).unwrap(), "src/a.rs");
        assert_eq!(stamp(&tree.join("src/b.rs")).unwrap(), stamp_b, "an unchanged file keeps its modification time");
        assert!(!tree.join("README.md").exists(), "a file the project lost is removed");
        again.finish();
        fs::remove_file(src.join("src/new.rs")).unwrap();
        write(&src, "docs/deep/only.md", "x");
        let again = slot(&src, &cache);
        again.finish();
        fs::remove_file(src.join("docs/deep/only.md")).unwrap();
        let again = slot(&src, &cache);
        assert!(!tree.join("docs").exists(), "folders a removed file leaves empty go too");
        again.finish();
        write(&src, "src/new.rs", "new");
        let again = slot(&src, &cache);
        assert!(tree.join("src/new.rs").exists());
        assert_eq!(again.target.as_deref(), Some(tree.parent().unwrap().join("b").as_path()));
        again.finish();
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn the_cached_layout_adds_few_characters_to_the_base() {
        let src = scratch("layout-src");
        let cache = scratch("layout-cache");
        write(&src, "a.rs", "a");
        let ws = slot(&src, &cache);
        // /<8 hex>/0/t and /<8 hex>/0/b: 13 characters past the base (4.4
        // used /<16 hex>/slot0/tree, 28).
        let past = |p: &Path| pathlen::chars(p) - pathlen::chars(&cache);
        assert_eq!(past(&ws.tree), 13, "{}", ws.tree.display());
        assert_eq!(past(ws.target.as_deref().unwrap()), 13);
        assert_eq!(ws.path_len.longest, pathlen::chars(&ws.tree.join("a.rs")), "no Cargo.toml: the deepest copied file");
        ws.finish();
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn a_file_a_test_rewrote_in_the_copy_is_restored() {
        let src = scratch("rewrite-src");
        let cache = scratch("rewrite-cache");
        write(&src, "golden.txt", "want");
        let ws = slot(&src, &cache);
        let tree = ws.tree.clone();
        ws.finish();
        // Same size, different bytes, written by "the test".
        fs::write(tree.join("golden.txt"), "oops").unwrap();
        let ws = slot(&src, &cache);
        assert_eq!(fs::read_to_string(tree.join("golden.txt")).unwrap(), "want");
        ws.finish();
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&cache);
    }

    /// Rewrite `rel` in the copy with `text` (same length as before) and put
    /// its modification time back, so its stamp equals the recorded one: what
    /// a rewrite within one clock tick of the sync looks like.
    fn rewrite_keeping_stamp(tree: &Path, rel: &str, text: &str) {
        let path = tree.join(rel);
        let before = fs::metadata(&path).unwrap().modified().unwrap();
        fs::write(&path, text).unwrap();
        fs::OpenOptions::new().write(true).open(&path).unwrap().set_modified(before).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
    }

    #[test]
    fn a_same_size_rewrite_with_an_equal_stamp_is_still_restored() {
        let src = scratch("racy-src");
        let cache = scratch("racy-cache");
        write(&src, "golden.txt", "want");
        let ws = slot(&src, &cache);
        let tree = ws.tree.clone();
        ws.finish();
        rewrite_keeping_stamp(&tree, "golden.txt", "oops");
        let ws = slot(&src, &cache);
        assert_eq!(fs::read_to_string(tree.join("golden.txt")).unwrap(), "want", "the stamp was taken in the same tick as the write, so it is not trusted");
        ws.finish();
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn left_behind_sees_a_same_size_rewrite_with_an_equal_stamp() {
        let src = scratch("racy-left-src");
        let cache = scratch("racy-left-cache");
        write(&src, "golden.txt", "want");
        let mut ws = slot(&src, &cache);
        ws.write_edit("edit.txt", "aaaa").unwrap();
        let tree = ws.tree.clone();
        rewrite_keeping_stamp(&tree, "golden.txt", "oops");
        rewrite_keeping_stamp(&tree, "edit.txt", "bbbb");
        let left = ws.left_behind();
        assert_eq!(left.modified, vec![("edit.txt".to_string(), None), ("golden.txt".to_string(), None)]);
        ws.finish();
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn only_racy_stamps_pay_for_a_byte_compare() {
        assert!(!trusted((4, 1_000), 1_000), "same tick");
        assert!(!trusted((4, 1_000), 1_000 + RACY_MARGIN_NS), "inside the margin");
        assert!(trusted((4, 1_000), 1_001 + RACY_MARGIN_NS));
        let src = scratch("racy-cost-src");
        let cache = scratch("racy-cost-cache");
        for rel in ["a.txt", "b.txt", "c.txt"] { write(&src, rel, rel); }
        let ws = slot(&src, &cache);
        assert_eq!(ws.compared, 3, "a new slot reads everything");
        let tree = ws.tree.clone();
        ws.finish();
        // Files whose times are an hour old, in the source and the copy, as
        // after a sync long ago.
        let old = SystemTime::now() - Duration::from_secs(3600);
        for rel in ["a.txt", "b.txt", "c.txt"] {
            for root in [&src, &tree] { fs::OpenOptions::new().write(true).open(root.join(rel)).unwrap().set_modified(old).unwrap(); }
        }
        let ws = slot(&src, &cache);
        assert_eq!(ws.compared, 3, "new stamps are compared once");
        assert_eq!(ws.files_written, 0, "equal bytes are not rewritten");
        ws.finish();
        let ws = slot(&src, &cache);
        assert_eq!(ws.compared, 0, "old stamps are trusted without reading the files");
        ws.finish();
        // The manifest carries when its stamps were taken.
        let manifest = fs::read_to_string(tree.parent().unwrap().join("manifest.tsv")).unwrap();
        assert!(manifest.starts_with("#recorded\t"), "{manifest}");
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn left_behind_names_what_a_run_created_changed_and_removed() {
        let src = scratch("left-src");
        let cache = scratch("left-cache");
        write(&src, "guard.txt", "a\nb\n");
        write(&src, "data.txt", "keep");
        write(&src, "gone.txt", "bye");
        write(&src, "same.txt", "same");
        let mut ws = slot(&src, &cache);
        ws.write_edit("edited.rs", "fn x() {}\n").unwrap();
        assert_eq!(ws.left_behind(), LeftBehind::default(), "nothing ran yet");
        // What a test might do.
        let tree = ws.tree.clone();
        fs::write(tree.join("guard.txt"), "a\r\nb\r\n").unwrap();
        fs::write(tree.join("data.txt"), "kept but changed").unwrap();
        fs::write(tree.join("edited.rs"), "fn x() { 1 }\n").unwrap();
        fs::remove_file(tree.join("gone.txt")).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        fs::write(tree.join("same.txt"), "same").unwrap();
        write(&tree, "made.txt", "new");
        write(&tree, "target/debug/out.bin", "build output");
        write(&tree, "__pycache__/x.pyc", "build output");
        let left = ws.left_behind();
        assert_eq!(left.created, vec!["made.txt"]);
        assert_eq!(left.modified, vec![("data.txt".to_string(), None), ("edited.rs".to_string(), None), ("guard.txt".to_string(), Some("CRLF"))]);
        assert_eq!(left.deleted, vec!["gone.txt"]);
        let text = crate::canon::stable(&left.value());
        assert!(text.contains(r#""line_endings_only":1"#) && text.contains(r#"{"change":"line_endings","now":"CRLF","path":"guard.txt"}"#), "{text}");
        ws.finish();
        // The next sync puts the project's bytes back and reports nothing.
        let ws = slot(&src, &cache);
        assert_eq!(fs::read(tree.join("guard.txt")).unwrap(), b"a\nb\n");
        assert!(!tree.join("made.txt").exists());
        assert_eq!(ws.left_behind(), LeftBehind::default());
        ws.finish();
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn a_stale_lock_is_taken_over_and_a_live_one_is_not() {
        let dir = scratch("locks");
        let lock = dir.join("0.lock");
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
