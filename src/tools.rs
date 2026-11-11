//! One implementation for every CLI subcommand and MCP tool.

use crate::config::{self, ProjectConfig};
use crate::contract::{self, Failure};
use crate::harness::{self, VerifyRequest};
use crate::json::Value;
use crate::lang::analyze;
use crate::planner::{self, Request};
use crate::root::{find_root, legacy_db, store_dir};
use crate::store::{self, ClaimError, Store};
use crate::strategies;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const PRIMARY: &[&str] = &[
    "start_plan", "decompose", "get_plan", "complete_plan", "verify_step", "get_recipe",
    "save_recipe", "status", "resume", "export_plan", "get_constraints", "add_constraint",
    "add_note", "job_status", "cancel_job",
];

pub fn all_names() -> Vec<&'static str> {
    let mut names = vec![
        "start_plan", "decompose", "explore", "get_plan", "resume", "merge_plans",
        "update_plan_status", "complete_plan", "export_plan", "verify_step", "verify_plan",
        "record_steps", "claim_step", "release_step", "available_steps", "save_recipe",
        "get_recipe", "list_recipes", "prune_recipes", "validate_recipes", "list_strategies",
        "add_constraint", "get_constraints", "deactivate_constraint", "failure_history", "add_note",
        "resolve_failure", "status", "estimate", "usage_stats", "history", "list_plans",
        "prune_plans", "job_status", "cancel_job",
    ];
    names.sort();
    names
}

pub fn call(name: &str, args: &Value, cwd: &Path, store_override: Option<&Path>, telemetry: bool) -> Result<Value, Failure> {
    match name {
        "record_step" => return Err(contract::tool_removed("record_step", "record_steps")),
        "create_plan" => return Err(contract::tool_removed("create_plan", "start_plan")),
        _ => {}
    }
    if !all_names().contains(&name) {
        return Err(Failure::new("unknown_tool", format!("unknown tool {name}"), "Call tools/list"));
    }
    let root = arg_path(args, "project_root").unwrap_or_else(|| find_root(cwd));
    let store_path = store_dir(&root, store_override.or(arg_path(args, "store").as_deref()));
    if let Some(db) = legacy_db(&root, store_override) {
        if !store_path.join("store").is_file() && name != "status" && name != "estimate" && name != "list_strategies" {
            let mut err = store::legacy_error();
            err.hint = format!("Run trammel migrate --from {}", db.display());
            return Err(err);
        }
    }
    // verify_plan reads the plan, so it needs the store.
    let needs_store = !matches!(name, "decompose" | "explore" | "estimate" | "list_strategies" | "verify_step" | "job_status" | "cancel_job");
    let mut store = if needs_store { Some(Store::open(&store_path).map_err(|e| Failure::new("store", e, "Check that the store directory is writable"))?) } else { None };
    if telemetry {
        if let Some(store) = store.as_mut() { store.log_usage(name, ""); }
    }
    let result = dispatch(name, args, &root, store.as_mut(), &store_path)?;
    if let Some(store) = store.as_mut() { store.save().map_err(|e| Failure::new("store", e, "Retry the write"))?; }
    Ok(result)
}

fn dispatch(name: &str, args: &Value, root: &Path, store: Option<&mut Store>, store_path: &Path) -> Result<Value, Failure> {
    match name {
        "decompose" | "explore" | "start_plan" => plan_tool(name, args, root, store),
        "get_plan" | "resume" | "history" => {
            let id = required_i64(args, "plan_id")?;
            let store = store.ok_or_else(|| Failure::new("store", "store is not open", "Check that the store directory is writable"))?;
            let plan = store.get_plan(id).ok_or_else(|| Failure::new("plan_not_found", format!("plan {id} was not found"), "Call list_plans for valid plan ids"))?;
            if name == "history" { return Ok(crate::canon::obj(vec![("plan_id", Value::Int(id)), ("trajectories", Value::Array(store.trajectories(id)))])); }
            let plan = lean_plan(plan);
            Ok(if wants_full(args) { plan } else { compact_plan(&plan) })
        }
        "list_plans" => {
            if crate::canon::get(args, "db_path").is_some() {
                return Err(Failure::new("unknown_argument", "list_plans no longer accepts db_path", "Set --store or TRAMMEL_DB_PATH"));
            }
            let status = crate::canon::get(args, "status").and_then(crate::canon::as_str);
            Ok(Value::Array(store.unwrap().list_plans(status)))
        }
        "update_plan_status" => {
            let id = required_i64(args, "plan_id")?;
            let status = required_str(args, "status")?;
            if !store.unwrap().update_plan_status(id, &status) { return Err(Failure::new("plan_not_found", format!("plan {id} was not found"), "Call list_plans for valid plan ids")); }
            Ok(crate::canon::obj(vec![("plan_id", Value::Int(id)), ("status", Value::string(status))]))
        }
        "complete_plan" => complete(args, store.unwrap()),
        "record_steps" => {
            let steps = crate::canon::get(args, "steps").and_then(crate::canon::as_array).ok_or_else(|| missing("steps", "an array of {step_id, status}"))?;
            let (n, unknown) = store.unwrap().record_steps(steps);
            let mut out = crate::canon::obj(vec![("updated", Value::Int(n as i64))]);
            if !unknown.is_empty() {
                crate::canon::insert(&mut out, "not_found", Value::Array(unknown.into_iter().map(Value::Int).collect()));
                crate::canon::insert(&mut out, "hint", Value::string("step_id is the step `id` from get_plan, not step_index"));
            }
            Ok(out)
        }
        "add_constraint" => {
            let kind = required_str(args, "constraint_type")?;
            let description = required_str(args, "description")?;
            let context = crate::canon::get(args, "context").cloned().unwrap_or(Value::Object(vec![]));
            let id = store.unwrap().add_constraint(&kind, &description, &context, crate::canon::get(args, "plan_id").and_then(crate::canon::as_i64), crate::canon::get(args, "step_id").and_then(crate::canon::as_i64));
            Ok(crate::canon::obj(vec![("constraint_id", Value::Int(id))]))
        }
        "get_constraints" => Ok(Value::Array(store.unwrap().constraints(crate::canon::get(args, "constraint_type").and_then(crate::canon::as_str)))),
        "deactivate_constraint" => {
            let id = required_i64(args, "constraint_id")?;
            if !store.unwrap().deactivate_constraint(id) { return Err(Failure::new("not_found", format!("constraint {id} was not found"), "Call get_constraints for active constraint ids")); }
            Ok(crate::canon::obj(vec![("constraint_id", Value::Int(id)), ("active", Value::Bool(false))]))
        }
        "save_recipe" => {
            let goal = required_str(args, "goal")?;
            let strategy = crate::canon::get(args, "strategy").cloned().unwrap_or(Value::Object(vec![]));
            let outcome = crate::canon::get(args, "outcome").and_then(crate::canon::as_bool).unwrap_or(true);
            let sig = store.unwrap().save_recipe(&goal, &strategy, outcome);
            Ok(crate::canon::obj(vec![("signature", Value::string(sig))]))
        }
        "get_recipe" => {
            let goal = required_str(args, "goal")?;
            match store.unwrap().best_recipe(&goal, 0.55) {
                Some(row) => Ok(if wants_full(args) { row } else { compact_recipe(&row) }),
                None => Ok(crate::canon::obj(vec![("recipe", Value::Null)])),
            }
        }
        "list_recipes" => Ok(Value::Array(store.unwrap().list_recipes(crate::canon::get(args, "limit").and_then(crate::canon::as_i64).unwrap_or(50) as usize))),
        "add_note" => {
            let plan = required_i64(args, "plan_id")?;
            let text = required_str(args, "text")?;
            let store = store.unwrap();
            if store.get_plan(plan).is_none() { return Err(Failure::new("plan_not_found", format!("plan {plan} was not found"), "Call list_plans for valid plan ids")); }
            let agent = crate::canon::get(args, "agent_id").and_then(crate::canon::as_str);
            let step = crate::canon::get(args, "step_id").and_then(crate::canon::as_i64);
            Ok(crate::canon::obj(vec![("note_id", Value::Int(store.add_note(plan, &text, agent, step)))]))
        }
        "prune_plans" => {
            let days = crate::canon::get(args, "max_age_days").and_then(crate::canon::as_f64).unwrap_or(7.0);
            let status = match crate::canon::get(args, "status") {
                Some(Value::Null) => None,
                Some(value) => crate::canon::as_str(value).map(String::from),
                None => Some("pending".to_string()),
            };
            Ok(crate::canon::obj(vec![("removed", Value::Int(store.unwrap().prune_plans(days, status.as_deref()) as i64))]))
        }
        "prune_recipes" => {
            let days = crate::canon::get(args, "max_age_days").and_then(crate::canon::as_f64).unwrap_or(90.0);
            let ratio = crate::canon::get(args, "min_success_ratio").and_then(crate::canon::as_f64).unwrap_or(0.1);
            Ok(crate::canon::obj(vec![("removed", Value::Int(store.unwrap().prune_recipes(days, ratio) as i64))]))
        }
        "validate_recipes" => {
            let (checked, missing, removed) = store.unwrap().validate_recipes(root);
            Ok(crate::canon::obj(vec![("recipes_checked", Value::Int(checked as i64)), ("files_missing", Value::Int(missing as i64)), ("removed", Value::Int(removed as i64))]))
        }
        "list_strategies" => Ok(Value::Array(strategies::builtin().iter().map(|s| crate::canon::obj(vec![("name", Value::string(s.name)), ("description", Value::string(s.description)), ("successes", Value::Int(0)), ("failures", Value::Int(0))])).collect())),
        "estimate" => {
            let cfg = config::load(root);
            let language = crate::canon::get(args, "language").and_then(crate::canon::as_str).map(|s| s.to_string()).unwrap_or_else(|| analyze::detect(root, &cfg));
            let files = analyze::estimate(root, &language);
            Ok(crate::canon::obj(vec![
                ("language", Value::string(language)),
                ("scope", crate::canon::get(args, "scope").cloned().unwrap_or(Value::Null)),
                ("matching_files", Value::Int(files as i64)),
                ("recommendation", Value::string(if files > 5000 { "use scope" } else { "full analysis OK" })),
            ]))
        }
        "status" => {
            // Use the store `call` already holds. Opening it again deadlocks:
            // a second `flock` in this process waits on the first.
            let counts = match store {
                Some(open) => open.counts(),
                None => crate::canon::obj(vec![]),
            };
            Ok(crate::canon::obj(vec![
                ("store", Value::string(store_path.display().to_string())),
                ("counts", counts),
                ("primary_tools", Value::Array(PRIMARY.iter().copied().map(Value::string).collect())),
                ("advanced_tools", Value::Array(all_names().into_iter().filter(|n| !PRIMARY.contains(n)).map(Value::string).collect())),
                ("tool_surface_default", Value::string("primary")),
                ("tool_surface_env", Value::string("TRAMMEL_MCP_SURFACE")),
            ]))
        }
        "usage_stats" => usage(store.unwrap()),
        "failure_history" => Ok(Value::Array(store.unwrap().failures(crate::canon::get(args, "file_path").and_then(crate::canon::as_str), crate::canon::get(args, "limit").and_then(crate::canon::as_i64).unwrap_or(50) as usize))),
        "resolve_failure" => {
            let file = required_str(args, "file_path")?;
            let error_type = required_str(args, "error_type")?;
            let resolution = required_str(args, "resolution")?;
            let id = store.unwrap().record_failure(&file, &error_type, &resolution, crate::canon::get(args, "plan_id").and_then(crate::canon::as_i64), crate::canon::get(args, "step_id").and_then(crate::canon::as_i64));
            Ok(crate::canon::obj(vec![("failure_id", Value::Int(id))]))
        }
        "claim_step" => {
            let plan = required_i64(args, "plan_id")?;
            let step = required_i64(args, "step_id")?;
            let agent = required_str(args, "agent_id")?;
            store.unwrap().claim(plan, step, &agent).map_err(|e| match e {
                ClaimError::NotFound => step_failure("step not found".into(), "step_not_found", ""),
                ClaimError::Held(owner) => Failure::new("claim_held", format!("claimed by {owner}"), "Pick another step from available_steps, or wait for the 10 minute expiry"),
                ClaimError::Blocked(ids) => Failure::new(
                    "step_blocked",
                    format!("step {step} depends on steps {} that are not passed or skipped", ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", ")),
                    format!("Finish or record steps {:?} first, or call available_steps for steps you can claim now", ids),
                ),
            })?;
            Ok(crate::canon::obj(vec![("claimed", Value::Bool(true)), ("step_id", Value::Int(step))]))
        }
        "release_step" => {
            let step = required_i64(args, "step_id")?;
            let agent = required_str(args, "agent_id")?;
            store.unwrap().release(step, &agent).map_err(|e| step_failure(e, "claim_mismatch", "Pass the agent_id that holds the claim"))?;
            Ok(crate::canon::obj(vec![("released", Value::Bool(true))]))
        }
        "available_steps" => {
            let plan = required_i64(args, "plan_id")?;
            let agent = required_str(args, "agent_id")?;
            Ok(Value::Array(store.unwrap().available(plan, &agent)))
        }
        "merge_plans" => merge(args, store.unwrap()),
        "verify_step" => verify_step(args, root),
        "job_status" => {
            let id = required_str(args, "job_id")?;
            let wait = crate::canon::get(args, "wait_s").and_then(crate::canon::as_f64).unwrap_or(0.0);
            Ok(crate::jobs::status(&id, wait))
        }
        "cancel_job" => Ok(crate::jobs::cancel(&required_str(args, "job_id")?)),
        "verify_plan" => verify_plan(args, root, store.unwrap()),
        "export_plan" => export_plan(args, store.unwrap()),
        other => Err(Failure::new("unknown_tool", format!("unknown tool {other}"), "Call tools/list")),
    }
}

fn plan_tool(name: &str, args: &Value, root: &Path, store: Option<&mut Store>) -> Result<Value, Failure> {
    let goal = required_str(args, "goal")?;
    let project = arg_path(args, "project_root").unwrap_or_else(|| root.to_path_buf());
    let mut request = Request::new(goal.clone(), project);
    request.scope = crate::canon::get(args, "scope").and_then(crate::canon::as_str).map(|s| s.to_string());
    request.language = crate::canon::get(args, "language").and_then(crate::canon::as_str).map(|s| s.to_string());
    request.relevant_only = crate::canon::get(args, "relevant_only").and_then(crate::canon::as_bool).unwrap_or(false);
    request.summary_only = crate::canon::get(args, "summary_only").and_then(crate::canon::as_bool).unwrap_or(false);
    request.max_steps = crate::canon::get(args, "max_steps").and_then(crate::canon::as_i64).map(|n| n as usize);
    request.max_files = crate::canon::get(args, "max_files").and_then(crate::canon::as_i64);
    request.num_beams = crate::canon::get(args, "num_beams").and_then(crate::canon::as_i64).unwrap_or(3) as usize;
    request.strict_greenfield = crate::canon::get(args, "strict_greenfield").and_then(crate::canon::as_bool).unwrap_or(false);
    request.suppress_creation_hints = crate::canon::get(args, "suppress_creation_hints").and_then(crate::canon::as_bool).unwrap_or(false);
    request.expand_repo = crate::canon::get(args, "expand_repo").and_then(crate::canon::as_bool);
    request.focus_keywords = string_list(args, "focus_keywords");
    request.focus_globs = string_list(args, "focus_globs");
    request.files = string_list(args, "files");
    if name == "explore" {
        return planner::explore(&request).map_err(|e| Failure::new("decompose_rejected", e, "Name the files to create in the goal (for example src/report.py), or set strict_greenfield to false"));
    }
    let persist = name == "start_plan" && crate::canon::get(args, "persist").and_then(crate::canon::as_bool).unwrap_or(true);
    // A saved plan always keeps its steps; summary_only only trims the reply.
    let summary_only = request.summary_only;
    if persist { request.summary_only = false; }
    let full = planner::decompose(&request).map_err(|e| Failure::new("decompose_rejected", e, "Name the files to create in the goal (for example src/report.py), or set strict_greenfield to false"))?;
    let strategy = if persist && summary_only { planner::summary(&full) } else { full.clone() };
    // The reply is compact unless the caller asks for everything; the saved
    // plan always keeps the full strategy.
    let strategy = if summary_only || wants_full(args) { strategy } else { compact_strategy(&strategy) };
    if name == "decompose" { return Ok(strategy); }
    if !persist {
        return Ok(crate::canon::obj(vec![("strategy", strategy), ("plan_id", Value::Null), ("persisted", Value::Bool(false)), ("workflow", Value::string("start_plan"))]));
    }
    let forced = crate::canon::get(args, "strategy").and_then(crate::canon::as_str).map(|s| s.to_string());
    let mut stored = full;
    if let Some(name) = forced { crate::canon::insert(&mut stored, "strategy_name", Value::string(name)); }
    let id = store.ok_or_else(|| Failure::new("store", "store is not open", "Check that the store directory is writable"))?.create_plan(&goal, &stored);
    Ok(crate::canon::obj(vec![
        ("strategy", strategy),
        ("plan_id", Value::Int(id)),
        ("persisted", Value::Bool(true)),
        ("workflow", Value::string("start_plan")),
    ]))
}

fn wants_full(args: &Value) -> bool {
    crate::canon::get(args, "full").and_then(crate::canon::as_bool).unwrap_or(false)
}

const OMITTED: &str = "symbols, rationale, relevance parts, dependency_graph and analysis details; pass full: true for them";

/// Copy only `keys` of an object, in that order, skipping absent and null ones.
fn pick(value: &Value, keys: &[&str]) -> Value {
    Value::Object(keys.iter().filter_map(|k| crate::canon::get(value, k).filter(|v| !matches!(v, Value::Null)).map(|v| (k.to_string(), v.clone()))).collect())
}

fn compact_meta(meta: Option<&Value>) -> Value {
    meta.map(|m| pick(m, &["language", "scope", "anchors", "files_analyzed", "max_files_truncated", "plan_source", "named_files", "unresolved_files", "ambiguous_files", "named_files_hint"])).unwrap_or(Value::Object(vec![]))
}

/// The default start_plan / decompose reply: what an agent acts on.
/// Each step keeps its file, description (which names its main symbols),
/// dependencies and relevance.
fn compact_strategy(full: &Value) -> Value {
    let steps: Vec<Value> = crate::canon::get(full, "steps").and_then(crate::canon::as_array).unwrap_or(&[]).iter()
        .map(|s| pick(s, &["step_index", "file", "role", "description", "depends_on", "relevance"]))
        .collect();
    let mut out = pick(full, &["goal", "goal_fingerprint"]);
    crate::canon::insert(&mut out, "steps", Value::Array(steps));
    crate::canon::insert(&mut out, "analysis_meta", compact_meta(crate::canon::get(full, "analysis_meta")));
    if let Some(files) = crate::canon::get(full, "creation_hints").and_then(|h| crate::canon::get(h, "suggested_files")).and_then(crate::canon::as_array) {
        let paths: Vec<Value> = files.iter().filter_map(|f| crate::canon::get(f, "path").cloned()).collect();
        if !paths.is_empty() { crate::canon::insert(&mut out, "suggested_files", Value::Array(paths)); }
    }
    crate::canon::insert(&mut out, "omitted", Value::string(OMITTED));
    out
}

/// The default get_plan / resume reply: plan status, step rows and notes.
fn compact_plan(plan: &Value) -> Value {
    let mut out = pick(plan, &["id", "goal", "status", "current_step", "total_steps", "created", "updated"]);
    let steps: Vec<Value> = crate::canon::get(plan, "steps").and_then(crate::canon::as_array).unwrap_or(&[]).iter()
        .map(|s| pick(s, &["id", "step_index", "file", "role", "description", "status", "depends_on", "relevance", "claimed_by", "claimed_at"]))
        .collect();
    crate::canon::insert(&mut out, "steps", Value::Array(steps));
    crate::canon::insert(&mut out, "notes", crate::canon::get(plan, "notes").cloned().unwrap_or(Value::Array(vec![])));
    crate::canon::insert(&mut out, "analysis_meta", compact_meta(crate::canon::get(plan, "strategy").and_then(|s| crate::canon::get(s, "analysis_meta"))));
    crate::canon::insert(&mut out, "omitted", Value::string(OMITTED));
    out
}

/// The default get_recipe reply: the match and the files its strategy touched.
fn compact_recipe(row: &Value) -> Value {
    let mut out = pick(row, &["sig", "pattern", "similarity", "successes", "failures", "updated"]);
    let files: Vec<Value> = crate::canon::get(row, "strategy").and_then(|s| crate::canon::get(s, "steps")).and_then(crate::canon::as_array).unwrap_or(&[]).iter()
        .filter_map(|s| crate::canon::get(s, "file").cloned())
        .collect();
    crate::canon::insert(&mut out, "files", Value::Array(files));
    crate::canon::insert(&mut out, "omitted", Value::string("the stored strategy; pass full: true for it"));
    out
}

/// The reply for get_plan and resume. The stored strategy repeats every step
/// (about half the bytes) without status or claims, so the reply leaves that
/// copy out; `steps` is the live list. `depends_on` is stored as JSON text and
/// goes out as an array of step indexes.
fn lean_plan(mut plan: Value) -> Value {
    // `scaffold` is always the text "[]": a 3.x column with no 4.x writer.
    if let Value::Object(fields) = &mut plan { fields.retain(|(k, _)| k != "scaffold"); }
    if let Some(strategy) = crate::canon::get(&plan, "strategy").cloned() {
        if let Value::Object(fields) = strategy {
            let kept = fields.into_iter().filter(|(k, _)| k != "steps").collect();
            crate::canon::insert(&mut plan, "strategy", Value::Object(kept));
        }
    }
    if let Some(Value::Array(steps)) = crate::canon::get(&plan, "steps").cloned() {
        let steps = steps.into_iter().map(|mut step| {
            if let Some(parsed) = crate::canon::get(&step, "depends_on").and_then(crate::canon::as_str).and_then(|t| crate::json::parse(t).ok()) {
                crate::canon::insert(&mut step, "depends_on", parsed);
            }
            step
        }).collect();
        crate::canon::insert(&mut plan, "steps", Value::Array(steps));
    }
    plan
}

fn complete(args: &Value, store: &mut Store) -> Result<Value, Failure> {
    let id = required_i64(args, "plan_id")?;
    let outcome = crate::canon::get(args, "outcome").and_then(crate::canon::as_bool).unwrap_or(true);
    let plan = store.get_plan(id).ok_or_else(|| Failure::new("plan_not_found", format!("plan {id} was not found"), "Call list_plans for valid plan ids"))?;
    let goal = crate::canon::get(&plan, "goal").and_then(crate::canon::as_str).unwrap_or("").to_string();
    let strategy = crate::canon::get(&plan, "strategy").cloned().unwrap_or(Value::Object(vec![]));
    let sig = store.save_recipe(&goal, &strategy, outcome);
    store.update_plan_status(id, if outcome { "completed" } else { "failed" });
    Ok(crate::canon::obj(vec![("plan_id", Value::Int(id)), ("recipe", Value::string(sig)), ("outcome", Value::Bool(outcome))]))
}

/// The verify time limit: `timeout_s`, else `verify_timeout_s` from the
/// project config, else 300 s, capped at 600 s for a call that waits and
/// 3600 s for a background job. The second value explains a cap.
fn verify_timeout(args: &Value, root: &Path, background: bool) -> (Duration, Option<String>) {
    let asked = crate::canon::get(args, "timeout_s").and_then(crate::canon::as_f64)
        .or_else(|| config::load(root).verify_timeout_s.map(|s| s as f64))
        .unwrap_or(harness::DEFAULT_TIMEOUT_S as f64);
    let max = if background { harness::MAX_JOB_TIMEOUT_S } else { harness::MAX_SYNC_TIMEOUT_S };
    let secs = asked.round().clamp(1.0, max as f64) as u64;
    let note = (asked > max as f64).then(|| if background {
        format!("timeout_s {asked} was capped at {max} for a background job")
    } else {
        format!("timeout_s {asked} was capped at {max} for a call that waits; pass background: true for up to {}", harness::MAX_JOB_TIMEOUT_S)
    });
    (Duration::from_secs(secs), note)
}

fn verify_step(args: &Value, fallback: &Path) -> Result<Value, Failure> {
    let root = arg_path(args, "project_root").unwrap_or_else(|| fallback.to_path_buf());
    let edits = edits_of(args);
    let full = crate::canon::get(args, "full_output").and_then(crate::canon::as_bool).unwrap_or(false);
    let background = crate::canon::get(args, "background").and_then(crate::canon::as_bool).unwrap_or(false);
    if background && !crate::jobs::enabled() {
        return Err(Failure::new("background_unavailable", "background jobs need the MCP server; this process exits after one call", "Drop background (the CLI waits for the result), or call verify_step through trammel mcp"));
    }
    // Only work out a test command when it will run: a failed preflight
    // answers at once, and an explicit or configured command needs no
    // language detection.
    let test_cmd = if !harness::preflight_ok(&edits) {
        Vec::new()
    } else {
        match crate::canon::get(args, "test_cmd") {
            Some(Value::Array(items)) => items.iter().filter_map(crate::canon::as_str).map(|s| s.to_string()).collect(),
            Some(Value::String(text)) => config::split_cmd(text),
            _ => analyze::default_test_cmd(&root, crate::canon::get(args, "language").and_then(crate::canon::as_str)),
        }
    };
    let (timeout, note) = verify_timeout(args, &root, background);
    let request = VerifyRequest { edits, full_output: full, ..VerifyRequest::new(root, test_cmd, timeout) };
    let mut reply = if background {
        crate::jobs::start(request).map_err(|e| Failure::new("too_many_jobs", e, "Wait for a job with job_status, or stop one with cancel_job"))?
    } else {
        harness::verify(&request)
    };
    if let Some(note) = note { crate::canon::insert(&mut reply, "timeout_note", Value::string(note)); }
    Ok(reply)
}

/// verify_step for each step with content, in order. The whole call stays
/// within 600 s: each step gets what is left of that budget at most.
fn verify_plan(args: &Value, fallback: &Path, store: &mut Store) -> Result<Value, Failure> {
    let id = required_i64(args, "plan_id")?;
    let plan = store.get_plan(id).ok_or_else(|| Failure::new("plan_not_found", format!("plan {id} was not found"), "Call list_plans for valid plan ids"))?;
    let steps = crate::canon::get(&plan, "steps").and_then(crate::canon::as_array).map(|items| items.to_vec()).unwrap_or_default();
    let root = arg_path(args, "project_root").unwrap_or_else(|| fallback.to_path_buf());
    let (per_step, _) = verify_timeout(args, &root, false);
    let budget = Duration::from_secs(harness::MAX_SYNC_TIMEOUT_S);
    let started = std::time::Instant::now();
    let mut completed = 0i64;
    for step in &steps {
        let content = crate::canon::get(step, "content").and_then(crate::canon::as_str);
        let file = crate::canon::get(step, "file").and_then(crate::canon::as_str);
        if let (Some(file), Some(content)) = (file, content) {
            let left = budget.saturating_sub(started.elapsed());
            if left < Duration::from_secs(1) {
                return Ok(crate::canon::obj(vec![
                    ("success", Value::Bool(false)),
                    ("timed_out", Value::Bool(true)),
                    ("steps_completed", Value::Int(completed)),
                    ("hint", Value::string(format!("verify_plan stops after {}s in one call. Run verify_step for the remaining steps, with background: true for long test runs", harness::MAX_SYNC_TIMEOUT_S))),
                ]));
            }
            let mut one = args.clone();
            crate::canon::insert(&mut one, "edits", Value::Array(vec![crate::canon::obj(vec![("path", Value::string(file)), ("content", Value::string(content))])]));
            crate::canon::insert(&mut one, "timeout_s", Value::Int(per_step.min(left).as_secs() as i64));
            crate::canon::insert(&mut one, "background", Value::Bool(false));
            let verdict = verify_step(&one, fallback)?;
            if crate::canon::get(&verdict, "success").and_then(crate::canon::as_bool) != Some(true) {
                return Ok(crate::canon::obj(vec![("success", Value::Bool(false)), ("steps_completed", Value::Int(completed)), ("failed_step", step.clone()), ("verdict", verdict)]));
            }
        }
        completed += 1;
    }
    Ok(crate::canon::obj(vec![("success", Value::Bool(true)), ("steps_completed", Value::Int(completed))]))
}

fn export_plan(args: &Value, store: &Store) -> Result<Value, Failure> {
    let id = required_i64(args, "plan_id")?;
    let plan = store.get_plan(id).ok_or_else(|| Failure::new("plan_not_found", format!("plan {id} was not found"), "Call list_plans for valid plan ids"))?;
    let doc = crate::export::from_plan(&plan);
    if let Some(path) = crate::canon::get(args, "path").and_then(crate::canon::as_str) {
        std::fs::write(path, crate::canon::stable_pretty(&doc)).map_err(|e| Failure::new("export", e.to_string(), "Check the path is writable"))?;
    }
    Ok(doc)
}

fn merge(args: &Value, store: &mut Store) -> Result<Value, Failure> {
    let a = required_i64(args, "plan_a_id")?;
    let b = required_i64(args, "plan_b_id")?;
    let left = store.get_plan(a).ok_or_else(|| Failure::new("plan_not_found", format!("plan {a} was not found"), "Call list_plans for valid plan ids"))?;
    let right = store.get_plan(b).ok_or_else(|| Failure::new("plan_not_found", format!("plan {b} was not found"), "Call list_plans for valid plan ids"))?;
    let strategy = crate::canon::get(args, "strategy").and_then(crate::canon::as_str).unwrap_or("sequential");
    let mut steps = crate::canon::get(&left, "steps").and_then(crate::canon::as_array).map(|items| items.to_vec()).unwrap_or_default();
    let mut more = crate::canon::get(&right, "steps").and_then(crate::canon::as_array).map(|items| items.to_vec()).unwrap_or_default();
    if strategy == "interleave" {
        let mut mixed = Vec::new();
        let mut i = 0;
        while i < steps.len() || i < more.len() {
            if i < steps.len() { mixed.push(steps[i].clone()); }
            if i < more.len() { mixed.push(more[i].clone()); }
            i += 1;
        }
        steps = mixed;
    } else {
        steps.append(&mut more);
    }
    for (i, step) in steps.iter_mut().enumerate() { crate::canon::insert(step, "step_index", Value::Int(i as i64)); }
    let goal = format!("{} + {}", crate::canon::get(&left, "goal").and_then(crate::canon::as_str).unwrap_or(""), crate::canon::get(&right, "goal").and_then(crate::canon::as_str).unwrap_or(""));
    let merged = crate::canon::obj(vec![("goal", Value::string(goal.clone())), ("steps", Value::Array(steps)), ("merge_strategy", Value::string(strategy))]);
    let id = store.create_plan(&goal, &merged);
    Ok(crate::canon::obj(vec![("plan_id", Value::Int(id)), ("strategy", Value::string(strategy))]))
}

fn usage(store: &Store) -> Result<Value, Failure> {
    let n = store.usage_count();
    if n == 0 {
        return Ok(crate::canon::obj(vec![
            ("enabled", Value::Bool(false)),
            ("events", Value::Int(0)),
            ("hint", Value::string("Usage tracking is off. Set telemetry in .trammel.json or TRAMMEL_TELEMETRY=1. Events stay on this machine.")),
        ]));
    }
    Ok(crate::canon::obj(vec![("enabled", Value::Bool(true)), ("events", Value::Int(n as i64))]))
}

fn edits_of(args: &Value) -> Vec<(String, String)> {
    crate::canon::get(args, "edits").and_then(crate::canon::as_array).map(|items| {
        items.iter().filter_map(|item| {
            let path = crate::canon::get(item, "path").or_else(|| crate::canon::get(item, "file")).and_then(crate::canon::as_str)?;
            let content = crate::canon::get(item, "content").and_then(crate::canon::as_str).unwrap_or("");
            Some((path.to_string(), content.to_string()))
        }).collect()
    }).unwrap_or_default()
}

fn string_list(args: &Value, key: &str) -> Vec<String> {
    crate::canon::get(args, key).and_then(crate::canon::as_array).map(|items| items.iter().filter_map(crate::canon::as_str).map(|s| s.to_string()).collect()).unwrap_or_default()
}

fn required_str(args: &Value, key: &str) -> Result<String, Failure> {
    crate::canon::get(args, key).and_then(crate::canon::as_str).map(|s| s.to_string()).ok_or_else(|| missing(key, "a string"))
}

fn required_i64(args: &Value, key: &str) -> Result<i64, Failure> {
    crate::canon::get(args, key).and_then(crate::canon::as_i64).ok_or_else(|| missing(key, "an integer"))
}

fn missing(key: &str, kind: &str) -> Failure {
    Failure::new("missing_argument", format!("{key} is required"), format!("Pass {key} as {kind}; tools/list has the full inputSchema"))
}

/// The store reports an unknown step as "step not found". Agents usually pass
/// `step_index` there, so say which number is wanted.
fn step_failure(message: String, code: &'static str, hint: &str) -> Failure {
    if message == "step not found" {
        return Failure::new("step_not_found", message, "step_id is the step `id` from get_plan or available_steps, not step_index");
    }
    Failure::new(code, message, hint)
}

fn arg_path(args: &Value, key: &str) -> Option<PathBuf> {
    crate::canon::get(args, key).and_then(crate::canon::as_str).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_surface_is_35_tools_including_verify_plan_and_jobs() {
        let names = all_names();
        assert_eq!(names.len(), 35, "{names:?}");
        assert!(names.contains(&"verify_plan"));
        assert!(!names.contains(&"record_step"));
        assert!(!names.contains(&"create_plan"));
        assert_eq!(PRIMARY.len(), 15);
        assert!(PRIMARY.contains(&"job_status") && PRIMARY.contains(&"cancel_job"), "a background verify needs its poll on the default surface");
        assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn summary_only_start_plan_still_saves_the_steps() {
        let dir = std::env::temp_dir().join(format!("trammel-summary-plan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("models.py"), "class Item:\n    pass\n").unwrap();
        std::fs::write(dir.join("store.py"), "from models import Item\n\nclass Store:\n    pass\n").unwrap();
        let store = dir.join(".trammel");
        let root = ("project_root", Value::string(dir.display().to_string()));
        let args = crate::canon::obj(vec![root.clone(), ("goal", Value::string("add stock to Item")), ("summary_only", Value::Bool(true))]);
        let reply = call("start_plan", &args, &dir, Some(&store), false).unwrap();
        let strategy = crate::canon::get(&reply, "strategy").unwrap();
        assert!(crate::canon::get(strategy, "steps").is_none(), "the reply stays a summary");
        assert_eq!(crate::canon::get(strategy, "step_count").and_then(crate::canon::as_i64), Some(2));
        let id = crate::canon::get(&reply, "plan_id").cloned().unwrap();
        let plan = call("get_plan", &crate::canon::obj(vec![root, ("plan_id", id)]), &dir, Some(&store), false).unwrap();
        assert_eq!(crate::canon::get(&plan, "steps").and_then(crate::canon::as_array).map(|s| s.len()), Some(2));
        // The reply has one list of steps, with depends_on as an array.
        assert!(crate::canon::get(&plan, "strategy").and_then(|s| crate::canon::get(s, "steps")).is_none());
        let second = &crate::canon::get(&plan, "steps").and_then(crate::canon::as_array).unwrap()[1];
        assert_eq!(crate::canon::stable(crate::canon::get(second, "depends_on").unwrap()), "[0]");
        // complete_plan still saves the full strategy as the recipe.
        let id = crate::canon::get(&plan, "id").cloned().unwrap();
        call("complete_plan", &crate::canon::obj(vec![("project_root", Value::string(dir.display().to_string())), ("plan_id", id)]), &dir, Some(&store), false).unwrap();
        let recipe = call("get_recipe", &crate::canon::obj(vec![("project_root", Value::string(dir.display().to_string())), ("goal", Value::string("add stock to Item")), ("full", Value::Bool(true))]), &dir, Some(&store), false).unwrap();
        assert_eq!(crate::canon::get(&recipe, "strategy").and_then(|s| crate::canon::get(s, "steps")).and_then(crate::canon::as_array).map(|s| s.len()), Some(2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plan_replies_are_compact_unless_full_is_asked() {
        let dir = std::env::temp_dir().join(format!("trammel-compact-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("models.py"), "class Item:\n    pass\n").unwrap();
        std::fs::write(dir.join("store.py"), "from models import Item\n\nclass Store:\n    def add(self):\n        pass\n").unwrap();
        let store = dir.join(".trammel");
        let root = ("project_root", Value::string(dir.display().to_string()));
        let ask = |name: &str, extra: Vec<(&str, Value)>| {
            let mut fields = vec![root.clone()];
            fields.extend(extra);
            call(name, &crate::canon::obj(fields), &dir, Some(&store), false).unwrap()
        };
        let compact = ask("start_plan", vec![("goal", Value::string("add stock to Item in store"))]);
        let text = crate::canon::stable(&compact);
        assert!(!text.contains("\"symbols\"") && !text.contains("\"dependency_graph\":") && text.contains("\"omitted\""), "{text}");
        let step = &crate::canon::get(&compact, "strategy").and_then(|s| crate::canon::get(s, "steps")).and_then(crate::canon::as_array).unwrap()[1];
        assert_eq!(crate::canon::stable(crate::canon::get(step, "depends_on").unwrap()), "[0]");
        let full = ask("decompose", vec![("goal", Value::string("add stock to Item in store")), ("full", Value::Bool(true))]);
        assert!(crate::canon::get(&full, "dependency_graph").is_some());
        assert!(crate::canon::stable(&full).len() > crate::canon::stable(crate::canon::get(&compact, "strategy").unwrap()).len());
        let id = crate::canon::get(&compact, "plan_id").cloned().unwrap();
        let plan = ask("get_plan", vec![("plan_id", id.clone())]);
        assert!(crate::canon::get(&plan, "strategy").is_none() && crate::canon::get(&plan, "notes").is_some());
        assert!(!crate::canon::stable(&plan).contains("\"symbols\""));
        let plan_full = ask("get_plan", vec![("plan_id", id.clone()), ("full", Value::Bool(true))]);
        assert!(crate::canon::get(&plan_full, "strategy").is_some());
        ask("complete_plan", vec![("plan_id", id)]);
        let recipe = ask("get_recipe", vec![("goal", Value::string("add stock to Item in store"))]);
        assert_eq!(crate::canon::stable(crate::canon::get(&recipe, "files").unwrap()), r#"["models.py","store.py"]"#);
        assert!(crate::canon::get(&recipe, "strategy").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn strict_greenfield_says_what_to_do() {
        let dir = std::env::temp_dir().join(format!("trammel-greenfield-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let args = crate::canon::obj(vec![("project_root", Value::string(dir.display().to_string())), ("goal", Value::string("create a dashboard")), ("strict_greenfield", Value::Bool(true))]);
        let err = call("decompose", &args, &dir, Some(&dir.join(".trammel")), false).unwrap_err();
        assert_eq!(err.code, "decompose_rejected");
        assert!(!err.message.contains("scaffold") && !err.hint.contains("scaffold"), "{} / {}", err.message, err.hint);
        assert!(err.hint.contains("strict_greenfield to false"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_timeouts_come_from_the_call_then_config_and_are_capped() {
        let dir = std::env::temp_dir().join(format!("trammel-verify-timeout-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let none = crate::canon::obj(vec![]);
        assert_eq!(verify_timeout(&none, &dir, false), (Duration::from_secs(harness::DEFAULT_TIMEOUT_S), None));
        std::fs::write(dir.join(".trammel.json"), r#"{"verify_timeout_s": 900}"#).unwrap();
        let (secs, note) = verify_timeout(&none, &dir, false);
        assert_eq!(secs, Duration::from_secs(600));
        assert!(note.unwrap().contains("background: true"));
        assert_eq!(verify_timeout(&none, &dir, true), (Duration::from_secs(900), None));
        let asked = crate::canon::obj(vec![("timeout_s", Value::Int(45))]);
        assert_eq!(verify_timeout(&asked, &dir, false).0, Duration::from_secs(45));
        let huge = crate::canon::obj(vec![("timeout_s", Value::Int(99_999))]);
        assert_eq!(verify_timeout(&huge, &dir, true).0, Duration::from_secs(harness::MAX_JOB_TIMEOUT_S));
        // The reply carries the limit it ran with, and the note when capped.
        let root = ("project_root", Value::string(dir.display().to_string()));
        let cmd = ("test_cmd", Value::Array(harness::no_op_cmd().into_iter().map(Value::String).collect()));
        let reply = call("verify_step", &crate::canon::obj(vec![root.clone(), cmd.clone()]), &dir, None, false).unwrap();
        assert_eq!(crate::canon::get(&reply, "timeout_s").and_then(crate::canon::as_i64), Some(600));
        assert!(crate::canon::get(&reply, "timeout_note").is_some());
        // Outside the MCP server a background job could not outlive the call.
        if !crate::jobs::enabled() {
            let err = call("verify_step", &crate::canon::obj(vec![root, cmd, ("background", Value::Bool(true))]), &dir, None, false).unwrap_err();
            assert_eq!(err.code, "background_unavailable");
            assert!(err.hint.contains("trammel mcp"));
        }
        crate::workspace::forget(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_files_argument_reaches_the_planner() {
        let dir = std::env::temp_dir().join(format!("trammel-files-arg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("pkg")).unwrap();
        std::fs::write(dir.join("pkg/models.py"), "class Item:\n    pass\n").unwrap();
        std::fs::write(dir.join("pkg/store.py"), "from pkg.models import Item\n\nclass Store:\n    pass\n").unwrap();
        std::fs::write(dir.join("pkg/other.py"), "def other():\n    return 1\n").unwrap();
        let args = crate::canon::obj(vec![
            ("project_root", Value::string(dir.display().to_string())),
            ("goal", Value::string("rework it")),
            ("files", Value::Array(vec![Value::string("pkg/store.py")])),
        ]);
        let reply = call("decompose", &args, &dir, Some(&dir.join(".trammel")), false).unwrap();
        let text = crate::canon::stable(&reply);
        assert!(text.contains(r#""named_files":["pkg/store.py"]"#), "{text}");
        assert!(text.contains(r#""role":"named""#) && text.contains(r#""role":"dependency""#), "{text}");
        assert!(!text.contains("other.py"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn status_returns_without_relocking_the_store() {
        let dir = std::env::temp_dir().join(format!("trammel-status-lock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = dir.join("state");
        let args = crate::canon::obj(vec![("project_root", Value::string(dir.display().to_string()))]);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = call("status", &args, &dir, Some(&store), false);
            let _ = tx.send(result.is_ok());
        });
        match rx.recv_timeout(std::time::Duration::from_secs(2)) {
            Ok(true) => {}
            Ok(false) => panic!("status failed"),
            Err(_) => panic!("status deadlocked on the store lock"),
        }
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join(format!("trammel-status-lock-{}", std::process::id())));
    }
}

pub fn surface(name: &str) -> &'static str {
    if PRIMARY.contains(&name) { "primary" } else { "advanced" }
}

pub fn hints(name: &str) -> (bool, bool, bool, bool) {
    let read_only = matches!(name, "job_status" | "decompose" | "explore" | "get_plan" | "resume" | "get_recipe" | "list_recipes" | "list_strategies" | "get_constraints" | "status" | "estimate" | "usage_stats" | "history" | "list_plans" | "failure_history" | "available_steps" | "export_plan" | "validate_recipes");
    let _ = ProjectConfig::default();
    (read_only, !read_only, false, false)
}
