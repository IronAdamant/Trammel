//! Native snapshot plus append journal under `.trammel/`.
//! One exclusive writer, many shared readers, via `File::lock`.

use crate::canon::{self, canonical};
use crate::hash::sha256_hex;
use crate::json::{self, Value};
use crate::similarity::goal_similarity;
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA: i64 = 1;

pub struct Store {
    dir: PathBuf,
    _lock: File,
    doc: Value,
    dirty: bool,
}

impl Store {
    pub fn open(dir: &Path) -> Result<Store, String> {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let lock_path = dir.join("lock");
        let lock = OpenOptions::new().create(true).read(true).write(true).open(&lock_path).map_err(|e| e.to_string())?;
        lock.lock().map_err(|e| e.to_string())?;
        let mut doc = read_snapshot(&dir.join("store"));
        apply_journal(&dir.join("journal"), &mut doc);
        if canon::get(&doc, "schema_version").is_none() {
            doc = empty_doc();
        }
        Ok(Store { dir: dir.to_path_buf(), _lock: lock, doc, dirty: false })
    }

    pub fn open_shared(dir: &Path) -> Result<Store, String> {
        if !dir.join("store").is_file() && !dir.join("journal").is_file() {
            return Store::open(dir);
        }
        let lock_path = dir.join("lock");
        let lock = OpenOptions::new().create(true).read(true).write(true).open(&lock_path).map_err(|e| e.to_string())?;
        lock.lock_shared().map_err(|e| e.to_string())?;
        let mut doc = read_snapshot(&dir.join("store"));
        apply_journal(&dir.join("journal"), &mut doc);
        Ok(Store { dir: dir.to_path_buf(), _lock: lock, doc, dirty: false })
    }

    pub fn save(&mut self) -> Result<(), String> {
        if !self.dirty { return Ok(()); }
        write_snapshot(&self.dir.join("store"), &self.doc)?;
        let journal = self.dir.join("journal");
        if journal.exists() { let _ = fs::remove_file(&journal); }
        self.dirty = false;
        Ok(())
    }

    fn touch(&mut self) { self.dirty = true; }

    fn table_mut(&mut self, name: &str) -> &mut Vec<Value> {
        if canon::get(&self.doc, name).is_none() {
            canon::insert(&mut self.doc, name, Value::Array(vec![]));
        }
        match canon::get_mut(&mut self.doc, name) {
            Some(Value::Array(items)) => items,
            _ => unreachable!("table"),
        }
    }

    fn next_id(&mut self, name: &str) -> i64 {
        let key = format!("next_{name}");
        let id = canon::get(&self.doc, &key).and_then(canon::as_i64).unwrap_or(1);
        canon::insert(&mut self.doc, &key, Value::Int(id + 1));
        id
    }

    pub fn create_plan(&mut self, goal: &str, strategy: &Value) -> i64 {
        let id = self.next_id("plan");
        let now = now_secs();
        let steps = canon::get(strategy, "steps").and_then(canon::as_array).map(|items| items.to_vec()).unwrap_or_default();
        let total = steps.len() as i64;
        self.table_mut("plans").push(canon::obj(vec![
            ("id", Value::Int(id)),
            ("goal", Value::string(goal)),
            ("strategy", Value::string(canonical(strategy))),
            ("scaffold", Value::string("[]")),
            ("status", Value::string("pending")),
            ("current_step", Value::Int(0)),
            ("total_steps", Value::Int(total)),
            ("created", Value::Float(now)),
            ("updated", Value::Float(now)),
        ]));
        for (index, step) in steps.iter().enumerate() {
            let sid = self.next_id("step");
            self.table_mut("steps").push(canon::obj(vec![
                ("id", Value::Int(sid)),
                ("plan_id", Value::Int(id)),
                ("step_index", Value::Int(index as i64)),
                ("description", canon::get(step, "description").cloned().unwrap_or(Value::string(""))),
                ("rationale", canon::get(step, "rationale").cloned().unwrap_or(Value::string(""))),
                ("file", canon::get(step, "file").cloned().unwrap_or(Value::Null)),
                ("depends_on", Value::string(canonical(canon::get(step, "depends_on").unwrap_or(&Value::Array(vec![]))))),
                ("status", Value::string("pending")),
                ("symbols", canon::get(step, "symbols").cloned().unwrap_or(Value::Array(vec![]))),
                ("relevance", canon::get(step, "relevance").cloned().unwrap_or(Value::Null)),
                ("claimed_by", Value::Null),
                ("claimed_at", Value::Null),
            ]));
        }
        self.touch();
        id
    }

    pub fn get_plan(&self, id: i64) -> Option<Value> {
        let plan = self.rows("plans").into_iter().find(|row| canon::get(row, "id").and_then(canon::as_i64) == Some(id))?;
        let mut steps: Vec<Value> = self.rows("steps").into_iter().filter(|row| canon::get(row, "plan_id").and_then(canon::as_i64) == Some(id)).cloned().collect();
        steps.sort_by_key(|row| canon::get(row, "step_index").and_then(canon::as_i64).unwrap_or(0));
        let mut out = plan.clone();
        if let Some(text) = canon::get(&out, "strategy").and_then(canon::as_str) {
            if let Ok(parsed) = json::parse(text) { canon::insert(&mut out, "strategy", parsed); }
        }
        canon::insert(&mut out, "steps", Value::Array(steps));
        Some(out)
    }

    pub fn list_plans(&self, status: Option<&str>) -> Vec<Value> {
        self.rows("plans").into_iter().filter(|row| status.map(|s| canon::get(row, "status").and_then(canon::as_str) == Some(s)).unwrap_or(true)).cloned().collect()
    }

    pub fn update_plan_status(&mut self, id: i64, status: &str) -> bool {
        let plans = self.table_mut("plans");
        let Some(plan) = plans.iter_mut().find(|row| canon::get(row, "id").and_then(canon::as_i64) == Some(id)) else { return false };
        canon::insert(plan, "status", Value::string(status));
        canon::insert(plan, "updated", Value::Float(now_secs()));
        self.dirty = true;
        true
    }

    pub fn record_steps(&mut self, steps: &[Value]) -> usize {
        let mut n = 0;
        for incoming in steps {
            let Some(id) = canon::get(incoming, "step_id").and_then(canon::as_i64).or_else(|| canon::get(incoming, "id").and_then(canon::as_i64)) else { continue };
            let status = canon::get(incoming, "status").and_then(canon::as_str).unwrap_or("passed");
            let rows = self.table_mut("steps");
            if let Some(row) = rows.iter_mut().find(|row| canon::get(row, "id").and_then(canon::as_i64) == Some(id)) {
                canon::insert(row, "status", Value::string(status));
                n += 1;
            }
        }
        if n > 0 { self.dirty = true; }
        n
    }

    pub fn add_constraint(&mut self, kind: &str, description: &str, context: &Value, plan_id: Option<i64>, step_id: Option<i64>) -> i64 {
        let id = self.next_id("constraint");
        self.table_mut("constraints").push(canon::obj(vec![
            ("id", Value::Int(id)),
            ("plan_id", plan_id.map(Value::Int).unwrap_or(Value::Null)),
            ("step_id", step_id.map(Value::Int).unwrap_or(Value::Null)),
            ("constraint_type", Value::string(kind)),
            ("description", Value::string(description)),
            ("context", context.clone()),
            ("active", Value::Bool(true)),
            ("created", Value::Float(now_secs())),
        ]));
        self.touch();
        id
    }

    pub fn constraints(&self, kind: Option<&str>) -> Vec<Value> {
        self.rows("constraints").into_iter().filter(|row| canon::get(row, "active").and_then(canon::as_bool).unwrap_or(true)).filter(|row| kind.map(|k| canon::get(row, "constraint_type").and_then(canon::as_str) == Some(k)).unwrap_or(true)).cloned().collect()
    }

    pub fn deactivate_constraint(&mut self, id: i64) -> bool {
        let rows = self.table_mut("constraints");
        let Some(row) = rows.iter_mut().find(|row| canon::get(row, "id").and_then(canon::as_i64) == Some(id)) else { return false };
        canon::insert(row, "active", Value::Bool(false));
        self.dirty = true;
        true
    }

    pub fn save_recipe(&mut self, goal: &str, strategy: &Value, outcome: bool) -> String {
        let sig = sha256_hex(canonical(strategy).as_bytes());
        let pattern = goal.chars().take(400).collect::<String>();
        let now = now_secs();
        let recipes = self.table_mut("recipes");
        if let Some(row) = recipes.iter_mut().find(|row| canon::get(row, "sig").and_then(canon::as_str) == Some(sig.as_str())) {
            let field = if outcome { "successes" } else { "failures" };
            let n = canon::get(row, field).and_then(canon::as_i64).unwrap_or(0) + 1;
            canon::insert(row, field, Value::Int(n));
            canon::insert(row, "pattern", Value::string(pattern));
            canon::insert(row, "updated", Value::Float(now));
        } else {
            recipes.push(canon::obj(vec![
                ("sig", Value::string(sig.clone())),
                ("pattern", Value::string(pattern)),
                ("strategy", Value::string(canonical(strategy))),
                ("successes", Value::Int(if outcome { 1 } else { 0 })),
                ("failures", Value::Int(if outcome { 0 } else { 1 })),
                ("created", Value::Float(now)),
                ("updated", Value::Float(now)),
            ]));
        }
        self.touch();
        sig
    }

    pub fn best_recipe(&self, goal: &str, min_similarity: f64) -> Option<Value> {
        let mut best: Option<(f64, Value)> = None;
        for row in self.rows("recipes") {
            let pattern = canon::get(row, "pattern").and_then(canon::as_str).unwrap_or("");
            let score = goal_similarity(goal, pattern);
            if score + 1e-12 >= min_similarity && best.as_ref().map(|(s, _)| score > *s).unwrap_or(true) {
                best = Some((score, row.clone()));
            }
        }
        best.map(|(score, mut row)| {
            canon::insert(&mut row, "similarity", Value::Float(score));
            if let Some(text) = canon::get(&row, "strategy").and_then(canon::as_str) {
                if let Ok(parsed) = json::parse(text) { canon::insert(&mut row, "strategy", parsed); }
            }
            row
        })
    }

    pub fn list_recipes(&self, limit: usize) -> Vec<Value> {
        let mut rows: Vec<Value> = self.rows("recipes").into_iter().cloned().collect();
        rows.sort_by(|a, b| canon::as_f64(canon::get(b, "updated").unwrap_or(&Value::Float(0.0))).unwrap_or(0.0).partial_cmp(&canon::as_f64(canon::get(a, "updated").unwrap_or(&Value::Float(0.0))).unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal));
        rows.truncate(limit);
        rows
    }

    pub fn claim(&mut self, plan_id: i64, step_id: i64, agent: &str) -> Result<(), String> {
        let now = now_secs();
        let rows = self.table_mut("steps");
        let row = rows.iter_mut().find(|row| canon::get(row, "id").and_then(canon::as_i64) == Some(step_id) && canon::get(row, "plan_id").and_then(canon::as_i64) == Some(plan_id)).ok_or("step not found")?;
        if let (Some(owner), Some(at)) = (canon::get(row, "claimed_by").and_then(canon::as_str), canon::get(row, "claimed_at").and_then(canon::as_f64)) {
            if owner != agent && now - at < 600.0 {
                return Err(format!("claimed by {owner}"));
            }
        }
        canon::insert(row, "claimed_by", Value::string(agent));
        canon::insert(row, "claimed_at", Value::Float(now));
        canon::insert(row, "status", Value::string("running"));
        self.dirty = true;
        Ok(())
    }

    pub fn release(&mut self, step_id: i64, agent: &str) -> Result<(), String> {
        let rows = self.table_mut("steps");
        let row = rows.iter_mut().find(|row| canon::get(row, "id").and_then(canon::as_i64) == Some(step_id)).ok_or("step not found")?;
        if canon::get(row, "claimed_by").and_then(canon::as_str) != Some(agent) {
            return Err("claim is held by another agent".into());
        }
        canon::insert(row, "claimed_by", Value::Null);
        canon::insert(row, "claimed_at", Value::Null);
        if canon::get(row, "status").and_then(canon::as_str) == Some("running") {
            canon::insert(row, "status", Value::string("pending"));
        }
        self.dirty = true;
        Ok(())
    }

    pub fn available(&self, plan_id: i64, agent: &str) -> Vec<Value> {
        let now = now_secs();
        self.rows("steps").into_iter().filter(|row| canon::get(row, "plan_id").and_then(canon::as_i64) == Some(plan_id)).filter(|row| {
            let status = canon::get(row, "status").and_then(canon::as_str).unwrap_or("pending");
            if status != "pending" && status != "running" { return false; }
            match (canon::get(row, "claimed_by").and_then(canon::as_str), canon::get(row, "claimed_at").and_then(canon::as_f64)) {
                (Some(owner), Some(at)) if owner != agent && now - at < 600.0 => false,
                _ => true,
            }
        }).cloned().collect()
    }

    pub fn record_failure(&mut self, file: &str, error_type: &str, resolution: &str, plan_id: Option<i64>, step_id: Option<i64>) -> i64 {
        let id = self.next_id("failure");
        self.table_mut("failures").push(canon::obj(vec![
            ("id", Value::Int(id)),
            ("file_path", Value::string(file)),
            ("error_type", Value::string(error_type)),
            ("last_resolution", Value::string(resolution)),
            ("plan_id", plan_id.map(Value::Int).unwrap_or(Value::Null)),
            ("step_id", step_id.map(Value::Int).unwrap_or(Value::Null)),
            ("created", Value::Float(now_secs())),
        ]));
        self.touch();
        id
    }

    pub fn failures(&self, file: Option<&str>, limit: usize) -> Vec<Value> {
        let mut rows: Vec<Value> = self.rows("failures").into_iter().filter(|row| file.map(|f| canon::get(row, "file_path").and_then(canon::as_str) == Some(f)).unwrap_or(true)).cloned().collect();
        rows.truncate(limit);
        rows
    }

    pub fn log_usage(&mut self, event: &str, detail: &str) {
        self.table_mut("usage_events").push(canon::obj(vec![
            ("event_type", Value::string(event)),
            ("detail", Value::string(detail)),
            ("created", Value::Float(now_secs())),
        ]));
        self.touch();
    }

    pub fn usage_count(&self) -> usize { self.rows("usage_events").len() }

    pub fn counts(&self) -> Value {
        canon::obj(vec![
            ("plans", Value::Int(self.rows("plans").len() as i64)),
            ("steps", Value::Int(self.rows("steps").len() as i64)),
            ("recipes", Value::Int(self.rows("recipes").len() as i64)),
            ("constraints", Value::Int(self.rows("constraints").len() as i64)),
            ("failures", Value::Int(self.rows("failures").len() as i64)),
            ("usage_events", Value::Int(self.rows("usage_events").len() as i64)),
        ])
    }

    pub fn rows(&self, name: &str) -> Vec<&Value> {
        canon::get(&self.doc, name).and_then(canon::as_array).map(|items| items.iter().collect()).unwrap_or_default()
    }

    pub fn document(&self) -> &Value { &self.doc }

    pub fn replace_document(&mut self, doc: Value) {
        self.doc = doc;
        self.touch();
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        let _ = self.save();
        let _ = self._lock.unlock();
    }
}

fn empty_doc() -> Value {
    canon::obj(vec![
        ("schema_version", Value::Int(SCHEMA)),
        ("plans", Value::Array(vec![])),
        ("steps", Value::Array(vec![])),
        ("constraints", Value::Array(vec![])),
        ("recipes", Value::Array(vec![])),
        ("failures", Value::Array(vec![])),
        ("usage_events", Value::Array(vec![])),
        ("trajectories", Value::Array(vec![])),
    ])
}

fn read_snapshot(path: &Path) -> Value {
    let Ok(text) = fs::read_to_string(path) else { return empty_doc() };
    json::parse(&text).unwrap_or_else(|_| empty_doc())
}

fn write_snapshot(path: &Path, doc: &Value) -> Result<(), String> {
    let tmp = path.with_extension("store.tmp");
    let mut file = File::create(&tmp).map_err(|e| e.to_string())?;
    file.write_all(canonical(doc).as_bytes()).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn apply_journal(path: &Path, doc: &mut Value) {
    let Ok(text) = fs::read_to_string(path) else { return };
    for line in text.lines() {
        if let Ok(value) = json::parse(line) {
            if canon::get(&value, "schema_version").is_some() { *doc = value; }
        }
    }
}

pub fn append_journal(dir: &Path, doc: &Value) -> Result<(), String> {
    let mut file = OpenOptions::new().create(true).append(true).open(dir.join("journal")).map_err(|e| e.to_string())?;
    writeln!(file, "{}", canonical(doc)).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())
}

fn now_secs() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

pub fn legacy_error() -> crate::contract::Failure {
    crate::contract::Failure::new(
        "legacy_sqlite",
        "This project still has a Python trammel.db",
        "Run trammel migrate to copy it into .trammel and leave the old file untouched",
    )
}

pub fn read_lock_shared_ok(dir: &Path) -> bool {
    Store::open_shared(dir).is_ok()
}

pub fn journal_len(dir: &Path) -> u64 {
    let mut file = match File::open(dir.join("journal")) { Ok(f) => f, Err(_) => return 0 };
    let _ = file.seek(SeekFrom::End(0));
    file.stream_position().unwrap_or(0)
}
