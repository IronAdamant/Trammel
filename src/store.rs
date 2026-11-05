//! Native snapshot plus append journal under `.trammel/`.
//! One exclusive writer, many shared readers, via `File::lock`.

use crate::canon::{self, canonical};
use crate::hash::sha256_hex;
use crate::json::{self, Value};
use crate::similarity::{concept_similarity, concept_terms, goal_similarity};
use std::collections::BTreeSet;
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
        canon::insert(&mut out, "notes", Value::Array(self.notes(id)));
        Some(out)
    }

    /// A free-text handoff note on a plan, optionally tied to a step.
    pub fn add_note(&mut self, plan_id: i64, text: &str, agent: Option<&str>, step_id: Option<i64>) -> i64 {
        let id = self.next_id("note");
        self.table_mut("notes").push(canon::obj(vec![
            ("id", Value::Int(id)),
            ("plan_id", Value::Int(plan_id)),
            ("step_id", step_id.map(Value::Int).unwrap_or(Value::Null)),
            ("agent_id", agent.map(Value::string).unwrap_or(Value::Null)),
            ("text", Value::string(text)),
            ("created", Value::Float(now_secs())),
        ]));
        self.touch();
        id
    }

    /// A plan's notes, oldest first, without the repeated plan_id.
    pub fn notes(&self, plan_id: i64) -> Vec<Value> {
        self.rows("notes").into_iter().filter(|row| canon::get(row, "plan_id").and_then(canon::as_i64) == Some(plan_id)).map(|row| {
            let mut note = row.clone();
            if let Value::Object(fields) = &mut note { fields.retain(|(k, _)| k != "plan_id"); }
            note
        }).collect()
    }

    /// One beam's outcome from the goal command, as `history` returns it.
    pub fn log_trajectory(&mut self, plan_id: i64, beam_id: i64, variant: &str, steps_completed: i64, success: bool, failure_reason: Option<&str>) {
        let id = self.next_id("trajectory");
        self.table_mut("trajectories").push(canon::obj(vec![
            ("id", Value::Int(id)),
            ("plan_id", Value::Int(plan_id)),
            ("beam_id", Value::Int(beam_id)),
            ("variant", Value::string(variant)),
            ("steps_completed", Value::Int(steps_completed)),
            ("success", Value::Bool(success)),
            ("failure_reason", failure_reason.map(Value::string).unwrap_or(Value::Null)),
            ("created", Value::Float(now_secs())),
        ]));
        self.touch();
    }

    /// Trajectories recorded for a plan: by the goal command, or migrated
    /// from a 3.x `trammel.db`.
    pub fn trajectories(&self, plan_id: i64) -> Vec<Value> {
        self.rows("trajectories").into_iter().filter(|row| canon::get(row, "plan_id").and_then(canon::as_i64) == Some(plan_id)).cloned().collect()
    }

    /// Delete plans last updated more than `max_age_days` ago, with their
    /// steps, notes, trajectories and plan constraints. `status` None means
    /// any status. Returns how many plans went.
    pub fn prune_plans(&mut self, max_age_days: f64, status: Option<&str>) -> usize {
        let cutoff = now_secs() - max_age_days * 86400.0;
        let doomed: BTreeSet<i64> = self.rows("plans").into_iter().filter(|row| {
            let updated = canon::get(row, "updated").and_then(canon::as_f64).unwrap_or(0.0);
            updated < cutoff && status.map(|s| canon::get(row, "status").and_then(canon::as_str) == Some(s)).unwrap_or(true)
        }).filter_map(|row| canon::get(row, "id").and_then(canon::as_i64)).collect();
        if doomed.is_empty() { return 0; }
        self.table_mut("plans").retain(|row| !canon::get(row, "id").and_then(canon::as_i64).map(|id| doomed.contains(&id)).unwrap_or(false));
        for table in ["steps", "notes", "trajectories", "constraints"] {
            self.table_mut(table).retain(|row| !canon::get(row, "plan_id").and_then(canon::as_i64).map(|id| doomed.contains(&id)).unwrap_or(false));
        }
        self.touch();
        doomed.len()
    }

    /// Delete recipes last updated more than `max_age_days` ago whose success
    /// ratio is below `min_success_ratio` (or that never ran). Returns the count.
    pub fn prune_recipes(&mut self, max_age_days: f64, min_success_ratio: f64) -> usize {
        let cutoff = now_secs() - max_age_days * 86400.0;
        let before = self.rows("recipes").len();
        self.table_mut("recipes").retain(|row| {
            let updated = canon::get(row, "updated").and_then(canon::as_f64).unwrap_or(0.0);
            let ok = canon::get(row, "successes").and_then(canon::as_i64).unwrap_or(0) as f64;
            let bad = canon::get(row, "failures").and_then(canon::as_i64).unwrap_or(0) as f64;
            let weak = ok + bad == 0.0 || ok / (ok + bad) < min_success_ratio;
            !(updated < cutoff && weak)
        });
        let removed = before - self.rows("recipes").len();
        if removed > 0 { self.touch(); }
        removed
    }

    /// Check each recipe's files against `root`. A recipe whose files are all
    /// gone is deleted. Returns (recipes checked, files missing, recipes removed).
    pub fn validate_recipes(&mut self, root: &Path) -> (usize, usize, usize) {
        let mut missing_total = 0;
        let mut dead = BTreeSet::new();
        let rows: Vec<Value> = self.rows("recipes").into_iter().cloned().collect();
        for row in &rows {
            let files = recipe_files(row);
            let missing = files.iter().filter(|f| !root.join(f).is_file()).count();
            missing_total += missing;
            if !files.is_empty() && missing == files.len() {
                if let Some(sig) = canon::get(row, "sig").and_then(canon::as_str) { dead.insert(sig.to_string()); }
            }
        }
        if !dead.is_empty() {
            self.table_mut("recipes").retain(|row| !canon::get(row, "sig").and_then(canon::as_str).map(|s| dead.contains(s)).unwrap_or(false));
            self.touch();
        }
        (rows.len(), missing_total, dead.len())
    }

    /// Plan summaries, newest first: id, goal, status, current_step,
    /// total_steps, created, updated. get_plan has the rest.
    pub fn list_plans(&self, status: Option<&str>) -> Vec<Value> {
        let mut out: Vec<Value> = self.rows("plans").into_iter().filter(|row| status.map(|s| canon::get(row, "status").and_then(canon::as_str) == Some(s)).unwrap_or(true)).map(|row| {
            let mut summary = row.clone();
            if let Value::Object(fields) = &mut summary {
                fields.retain(|(k, _)| matches!(k.as_str(), "id" | "goal" | "status" | "current_step" | "total_steps" | "created" | "updated"));
            }
            summary
        }).collect();
        out.reverse();
        out
    }

    pub fn update_plan_status(&mut self, id: i64, status: &str) -> bool {
        let plans = self.table_mut("plans");
        let Some(plan) = plans.iter_mut().find(|row| canon::get(row, "id").and_then(canon::as_i64) == Some(id)) else { return false };
        canon::insert(plan, "status", Value::string(status));
        canon::insert(plan, "updated", Value::Float(now_secs()));
        self.dirty = true;
        true
    }

    /// Returns how many steps changed and the ids that matched no step.
    pub fn record_steps(&mut self, steps: &[Value]) -> (usize, Vec<i64>) {
        let mut n = 0;
        let mut unknown = Vec::new();
        for incoming in steps {
            let Some(id) = canon::get(incoming, "step_id").and_then(canon::as_i64).or_else(|| canon::get(incoming, "id").and_then(canon::as_i64)) else { continue };
            let status = canon::get(incoming, "status").and_then(canon::as_str).unwrap_or("passed");
            let rows = self.table_mut("steps");
            if let Some(row) = rows.iter_mut().find(|row| canon::get(row, "id").and_then(canon::as_i64) == Some(id)) {
                canon::insert(row, "status", Value::string(status));
                n += 1;
            } else {
                unknown.push(id);
            }
        }
        if n > 0 { self.dirty = true; }
        (n, unknown)
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

    /// The recipe closest to `goal`: the higher of the text blend and the
    /// concept-term overlap, where the recipe's files and symbols count as
    /// half-weight context.
    pub fn best_recipe(&self, goal: &str, min_similarity: f64) -> Option<Value> {
        let mut best: Option<(f64, Value)> = None;
        let query = concept_terms(goal);
        for row in self.rows("recipes") {
            let pattern = canon::get(row, "pattern").and_then(canon::as_str).unwrap_or("");
            let context = recipe_code_terms(row);
            let score = goal_similarity(goal, pattern).max(concept_similarity(&query, &concept_terms(pattern), &context));
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

    /// Steps an agent can work on now: pending or running, not held by another
    /// agent's live claim, and every step in `depends_on` (step indexes) done.
    pub fn available(&self, plan_id: i64, agent: &str) -> Vec<Value> {
        let now = now_secs();
        let steps: Vec<&Value> = self.rows("steps").into_iter().filter(|row| canon::get(row, "plan_id").and_then(canon::as_i64) == Some(plan_id)).collect();
        let done: BTreeSet<i64> = steps.iter().filter(|row| is_done(canon::get(row, "status").and_then(canon::as_str).unwrap_or(""))).filter_map(|row| canon::get(row, "step_index").and_then(canon::as_i64)).collect();
        steps.into_iter().filter(|row| {
            let status = canon::get(row, "status").and_then(canon::as_str).unwrap_or("pending");
            if status != "pending" && status != "running" { return false; }
            if !step_deps(row).iter().all(|dep| done.contains(dep)) { return false; }
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
            ("notes", Value::Int(self.rows("notes").len() as i64)),
            ("usage_events", Value::Int(self.rows("usage_events").len() as i64)),
        ])
    }

    pub fn rows(&self, name: &str) -> Vec<&Value> {
        canon::get(&self.doc, name).and_then(canon::as_array).map(|items| items.iter().collect()).unwrap_or_default()
    }

    pub fn document(&self) -> &Value { &self.doc }

    /// Take over a whole document (a migrated 3.x database). Id counters
    /// start after the highest id in each table, so new rows never reuse one.
    pub fn replace_document(&mut self, doc: Value) {
        self.doc = doc;
        for (table, key) in [("plans", "plan"), ("steps", "step"), ("constraints", "constraint"), ("failures", "failure"), ("trajectories", "trajectory"), ("notes", "note")] {
            let max = self.rows(table).iter().filter_map(|row| canon::get(row, "id").and_then(canon::as_i64)).max().unwrap_or(0);
            let counter = format!("next_{key}");
            let current = canon::get(&self.doc, &counter).and_then(canon::as_i64).unwrap_or(1);
            canon::insert(&mut self.doc, &counter, Value::Int(current.max(max + 1)));
        }
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

fn recipe_strategy(row: &Value) -> Value {
    match canon::get(row, "strategy") {
        Some(Value::String(text)) => json::parse(text).unwrap_or(Value::Null),
        Some(other) => other.clone(),
        None => Value::Null,
    }
}

/// The files a recipe's strategy steps name.
fn recipe_files(row: &Value) -> Vec<String> {
    let strategy = recipe_strategy(row);
    canon::get(&strategy, "steps").and_then(canon::as_array).unwrap_or(&[]).iter().filter_map(|s| canon::get(s, "file").and_then(canon::as_str).map(String::from)).collect()
}

/// Terms from the files and symbols a recipe's strategy touched.
fn recipe_code_terms(row: &Value) -> BTreeSet<String> {
    let strategy = recipe_strategy(row);
    let mut terms = BTreeSet::new();
    for step in canon::get(&strategy, "steps").and_then(canon::as_array).unwrap_or(&[]) {
        if let Some(file) = canon::get(step, "file").and_then(canon::as_str) { terms.extend(concept_terms(file)); }
        for symbol in canon::get(step, "symbols").and_then(canon::as_array).unwrap_or(&[]) {
            if let Some(name) = canon::as_str(symbol) { terms.extend(concept_terms(name)); }
        }
    }
    terms
}

/// Step statuses that satisfy a dependency.
pub fn is_done(status: &str) -> bool {
    matches!(status, "passed" | "skipped" | "completed" | "done")
}

/// A step's `depends_on` step indexes. Stored as JSON text; older rows may
/// hold a plain array.
pub fn step_deps(step: &Value) -> Vec<i64> {
    let parsed = match canon::get(step, "depends_on") {
        Some(Value::String(text)) => json::parse(text).ok(),
        Some(other) => Some(other.clone()),
        None => None,
    };
    parsed.as_ref().and_then(canon::as_array).map(|items| items.iter().filter_map(canon::as_i64).collect()).unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::*;

    pub fn temp_store(tag: &str) -> (PathBuf, Store) {
        let dir = std::env::temp_dir().join(format!("trammel-store-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir).unwrap();
        (dir, store)
    }

    fn step(file: &str, deps: &[i64]) -> Value {
        canon::obj(vec![("file", Value::string(file)), ("depends_on", Value::Array(deps.iter().map(|d| Value::Int(*d)).collect()))])
    }

    fn files(rows: &[Value]) -> Vec<String> {
        rows.iter().filter_map(|r| canon::get(r, "file").and_then(canon::as_str).map(String::from)).collect()
    }

    #[test]
    fn get_recipe_matches_a_paraphrase_but_not_other_work() {
        let (dir, mut store) = temp_store("recipe");
        let strategy = canon::obj(vec![("steps", Value::Array(vec![canon::obj(vec![("file", Value::string("inventory/report.py")), ("symbols", Value::Array(vec![Value::string("format_report")]))])]))]);
        store.save_recipe("add a low-stock report listing items whose quantity is below a threshold", &strategy, true);
        let hit = store.best_recipe("report items with low stock under a threshold", 0.55).expect("paraphrase should match");
        assert!(canon::get(&hit, "similarity").and_then(canon::as_f64).unwrap() >= 0.55);
        for other in ["fix the login timeout in the auth module", "add a report of sales by region", "remove the threshold check from the store"] {
            assert!(store.best_recipe(other, 0.55).is_none(), "{other}");
        }
        drop(store);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_plans_is_a_newest_first_summary() {
        let (dir, mut store) = temp_store("list");
        let first = store.create_plan("first", &canon::obj(vec![("steps", Value::Array(vec![step("a.py", &[])]))]));
        let second = store.create_plan("second", &canon::obj(vec![("steps", Value::Array(vec![step("b.py", &[])]))]));
        let listed = store.list_plans(None);
        let ids: Vec<i64> = listed.iter().filter_map(|p| canon::get(p, "id").and_then(canon::as_i64)).collect();
        assert_eq!(ids, vec![second, first]);
        let Value::Object(fields) = &listed[0] else { panic!() };
        let keys: Vec<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
        for key in ["id", "goal", "status", "current_step", "total_steps", "created", "updated"] { assert!(keys.contains(&key), "{key}"); }
        assert!(!keys.contains(&"strategy") && !keys.contains(&"scaffold"));
        drop(store);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn notes_ride_along_with_the_plan() {
        let (dir, mut store) = temp_store("notes");
        let plan = store.create_plan("goal", &canon::obj(vec![("steps", Value::Array(vec![step("a.py", &[])]))]));
        store.add_note(plan, "a.py done; b.py needs the new Item.stock field", Some("agent-a"), Some(1));
        store.add_note(plan, "tests pass on Windows", None, None);
        let got = store.get_plan(plan).unwrap();
        let notes = canon::get(&got, "notes").and_then(canon::as_array).unwrap();
        assert_eq!(notes.len(), 2);
        assert_eq!(canon::get(&notes[0], "agent_id").and_then(canon::as_str), Some("agent-a"));
        assert!(canon::get(&notes[0], "plan_id").is_none());
        drop(store);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_and_validate_remove_what_they_report() {
        let (dir, mut store) = temp_store("prune");
        let old = store.create_plan("old", &canon::obj(vec![("steps", Value::Array(vec![step("a.py", &[])]))]));
        let fresh = store.create_plan("fresh", &canon::obj(vec![("steps", Value::Array(vec![step("a.py", &[])]))]));
        store.add_note(old, "stale", None, None);
        for row in store.table_mut("plans") {
            if canon::get(row, "id").and_then(canon::as_i64) == Some(old) { canon::insert(row, "updated", Value::Float(0.0)); }
        }
        assert_eq!(store.prune_plans(7.0, Some("completed")), 0, "status filter");
        assert_eq!(store.prune_plans(7.0, Some("pending")), 1);
        assert!(store.get_plan(old).is_none() && store.get_plan(fresh).is_some());
        assert!(store.rows("steps").iter().all(|s| canon::get(s, "plan_id").and_then(canon::as_i64) == Some(fresh)));
        assert!(store.rows("notes").is_empty());

        fs::write(dir.join("kept.py"), "x = 1\n").unwrap();
        fs::write(dir.join("other.py"), "x = 2\n").unwrap();
        let with = |file: &str| canon::obj(vec![("steps", Value::Array(vec![step(file, &[])]))]);
        store.save_recipe("keep me", &with("kept.py"), true);
        store.save_recipe("files gone", &with("gone.py"), true);
        store.save_recipe("old failure", &with("other.py"), false);
        assert_eq!(store.validate_recipes(&dir), (3, 1, 1));
        for row in store.table_mut("recipes") { canon::insert(row, "updated", Value::Float(0.0)); }
        assert_eq!(store.prune_recipes(90.0, 0.1), 1, "only the never-successful old recipe");
        assert_eq!(store.rows("recipes").len(), 1);
        drop(store);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn available_steps_wait_for_their_dependencies() {
        let (dir, mut store) = temp_store("deps");
        let strategy = canon::obj(vec![("steps", Value::Array(vec![step("a.py", &[]), step("b.py", &[0]), step("c.py", &[0, 1]), step("d.py", &[])]))]);
        let plan = store.create_plan("goal", &strategy);
        assert_eq!(files(&store.available(plan, "x")), vec!["a.py", "d.py"]);
        let a = canon::get(&store.available(plan, "x")[0], "id").and_then(canon::as_i64).unwrap();
        store.record_steps(&[canon::obj(vec![("step_id", Value::Int(a)), ("status", Value::string("passed"))])]);
        assert_eq!(files(&store.available(plan, "x")), vec!["b.py", "d.py"]);
        // A failed dependency does not unblock its dependents.
        let b = a + 1;
        store.record_steps(&[canon::obj(vec![("step_id", Value::Int(b)), ("status", Value::string("failed"))])]);
        assert_eq!(files(&store.available(plan, "x")), vec!["d.py"]);
        store.record_steps(&[canon::obj(vec![("step_id", Value::Int(b)), ("status", Value::string("skipped"))])]);
        assert_eq!(files(&store.available(plan, "x")), vec!["c.py", "d.py"]);
        drop(store);
        let _ = fs::remove_dir_all(&dir);
    }
}
