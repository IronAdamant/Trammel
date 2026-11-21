//! verify_step's memory of passing runs. When the same command passed on the
//! same files (same edits, same commit for `ref`) and nothing has changed
//! since, the stored result is returned instead of running the tests again.
//!
//! "Nothing changed" means: the same list of files a verify would copy, each
//! with the same size and modification time. A file modified within two
//! seconds of the run, where a second write could keep both, is also
//! compared by content hash. The cache lives beside the verify copies
//! (`<temp>/trv/<8 hex>/verified.json`, or under `TRAMMEL_VERIFY_CACHE`),
//! never in the project, and keeps the last `KEEP` passes. Only passing runs
//! are kept. What the files cannot show (installed packages, environment
//! variables, the network) is not checked: `rerun: true` runs the tests anyway.

use crate::canon;
use crate::hash::sha256_hex;
use crate::json::Value;
use std::path::{Path, PathBuf};

const KEEP: usize = 8;
const RACY_NS: u128 = 2_000_000_000;

/// The state of one verify request before it runs.
pub struct Probe {
    file: PathBuf,
    key: String,
    stamps: String,
    racy: Vec<(String, String)>,
}

/// What a verify would run against, or None when the cache is off.
/// `commit` is the resolved `ref`; then the commit stands for the files.
pub fn probe(root: &Path, test_cmd: &[String], edits: &[(String, String)], full_output: bool, commit: Option<&str>) -> Option<Probe> {
    let file = crate::workspace::state_dir(root)?.join("verified.json");
    let mut key = format!("v1\0{}\0{}\0", test_cmd.join("\u{1f}"), full_output);
    for (path, content) in edits { key.push_str(&format!("{path}\u{1f}{}\0", sha256_hex(content.as_bytes()))); }
    let key = sha256_hex(key.as_bytes());
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let (stamps, racy) = match commit {
        Some(id) => (format!("commit {id}"), Vec::new()),
        None => {
            let mut text = String::new();
            let mut racy = Vec::new();
            for rel in crate::workspace::project_files(root) {
                let Some((len, mtime)) = crate::workspace::file_stamp(&root.join(&rel)) else { continue };
                text.push_str(&format!("{rel}\t{len}\t{mtime}\n"));
                if mtime.saturating_add(RACY_NS) >= now {
                    racy.push((rel.clone(), content_hash(&root.join(&rel))));
                }
            }
            (sha256_hex(text.as_bytes()), racy)
        }
    };
    Some(Probe { file, key, stamps, racy })
}

fn content_hash(path: &Path) -> String {
    std::fs::read(path).map(|b| sha256_hex(&b)).unwrap_or_default()
}

fn entries(file: &Path) -> Vec<Value> {
    std::fs::read_to_string(file).ok().and_then(|t| crate::json::parse(&t).ok()).and_then(|v| canon::as_array(&v).map(|a| a.to_vec())).unwrap_or_default()
}

/// The stored passing reply for this probe, when nothing changed since, with
/// `cached` saying when it ran.
pub fn lookup(probe: &Probe, root: &Path) -> Option<Value> {
    let text = |v: &Value, k: &str| canon::get(v, k).and_then(canon::as_str).unwrap_or("").to_string();
    let entry = entries(&probe.file).into_iter().find(|e| text(e, "key") == probe.key && text(e, "stamps") == probe.stamps)?;
    // Files that were new when the pass ran: their bytes must match too.
    for pair in canon::get(&entry, "racy").and_then(canon::as_array).unwrap_or(&[]) {
        let items = canon::as_array(pair)?;
        let (rel, hash) = (items.first().and_then(canon::as_str)?, items.get(1).and_then(canon::as_str)?);
        if content_hash(&root.join(rel)) != hash { return None; }
    }
    let at = canon::get(&entry, "at").and_then(canon::as_f64).unwrap_or(0.0);
    let mut reply = canon::get(&entry, "reply").cloned()?;
    canon::insert(&mut reply, "cached", canon::obj(vec![
        ("at", Value::Float(at)),
        ("note", Value::string("Nothing changed since this command passed on these files, so the tests were not run again and this is that run's result. Pass rerun: true to run them anyway.")),
    ]));
    Some(reply)
}

/// Keep a passing reply for later calls. A failure is not kept.
pub fn remember(probe: &Probe, reply: &Value) {
    if canon::get(reply, "success").and_then(canon::as_bool) != Some(true) { return; }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let mut kept: Vec<Value> = entries(&probe.file).into_iter().filter(|e| canon::get(e, "key").and_then(canon::as_str) != Some(probe.key.as_str())).collect();
    let mut stored = reply.clone();
    // Fields about this call, not the run.
    if let Value::Object(fields) = &mut stored { fields.retain(|(k, _)| !matches!(k.as_str(), "recorded" | "record_error" | "timeout_note" | "cached")); }
    kept.insert(0, canon::obj(vec![
        ("key", Value::string(probe.key.clone())),
        ("stamps", Value::string(probe.stamps.clone())),
        ("racy", Value::Array(probe.racy.iter().map(|(r, h)| Value::Array(vec![Value::string(r.clone()), Value::string(h.clone())])).collect())),
        ("at", Value::Float(now)),
        ("reply", stored),
    ]));
    kept.truncate(KEEP);
    if let Some(dir) = probe.file.parent() { let _ = std::fs::create_dir_all(dir); }
    let tmp = probe.file.with_extension(format!("json.{}", std::process::id()));
    if std::fs::write(&tmp, canon::stable(&Value::Array(kept))).is_ok() && std::fs::rename(&tmp, &probe.file).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pass_is_reused_until_a_file_changes() {
        let dir = std::env::temp_dir().join(format!("trammel-vcache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.py"), "x = 1\n").unwrap();
        let cmd = vec!["python".to_string(), "-m".into(), "unittest".into()];
        let pass = canon::obj(vec![("success", Value::Bool(true)), ("recorded", Value::Bool(true))]);
        let first = probe(&dir, &cmd, &[], false, None).unwrap();
        assert!(lookup(&first, &dir).is_none());
        remember(&first, &pass);
        let hit = lookup(&probe(&dir, &cmd, &[], false, None).unwrap(), &dir).expect("same files, same command");
        assert!(canon::get(&hit, "cached").is_some());
        assert!(canon::get(&hit, "recorded").is_none(), "call fields are not stored");
        // Another command, other edits, or a commit are other keys.
        assert!(lookup(&probe(&dir, &["pytest".to_string()], &[], false, None).unwrap(), &dir).is_none());
        assert!(lookup(&probe(&dir, &cmd, &[("a.py".into(), "x = 2\n".into())], false, None).unwrap(), &dir).is_none());
        // The same size, rewritten at once: the racy content hash catches it.
        std::fs::write(dir.join("a.py"), "x = 3\n").unwrap();
        assert!(lookup(&probe(&dir, &cmd, &[], false, None).unwrap(), &dir).is_none());
        // A new file changes the list.
        let again = probe(&dir, &cmd, &[], false, None).unwrap();
        remember(&again, &pass);
        std::fs::write(dir.join("b.py"), "").unwrap();
        assert!(lookup(&probe(&dir, &cmd, &[], false, None).unwrap(), &dir).is_none());
        // A failure is never kept.
        let failing = probe(&dir, &cmd, &[], false, None).unwrap();
        remember(&failing, &canon::obj(vec![("success", Value::Bool(false))]));
        assert!(lookup(&failing, &dir).is_none());
        crate::workspace::forget(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
