//! One implementation for every CLI subcommand and MCP tool.

use crate::config::{self, ProjectConfig};
use crate::contract::{self, Failure};
use crate::harness::{self, VerifyRequest};
use crate::json::Value;
use crate::lang::analyze;
use crate::planner::{self, Request};
use crate::root::{find_root, legacy_db, store_dir};
use crate::store::{self, Store};
use crate::strategies;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const PRIMARY: &[&str] = &[
    "start_plan", "decompose", "get_plan", "complete_plan", "verify_step", "get_recipe",
    "save_recipe", "status", "resume", "export_plan", "get_constraints", "add_constraint",
];

pub fn all_names() -> Vec<&'static str> {
    let mut names = vec![
        "start_plan", "decompose", "explore", "get_plan", "resume", "merge_plans",
        "update_plan_status", "complete_plan", "export_plan", "verify_step", "verify_plan",
        "record_steps", "claim_step", "release_step", "available_steps", "save_recipe",
        "get_recipe", "list_recipes", "prune_recipes", "validate_recipes", "list_strategies",
        "add_constraint", "get_constraints", "deactivate_constraint", "failure_history",
        "resolve_failure", "status", "estimate", "usage_stats", "history", "list_plans",
        "prune_plans",
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
    let needs_store = !matches!(name, "decompose" | "explore" | "estimate" | "list_strategies" | "verify_step" | "verify_plan");
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
            let store = store.ok_or_else(|| Failure::new("store", "store is not open", ""))?;
            let plan = store.get_plan(id).ok_or_else(|| Failure::new("plan_not_found", format!("plan {id} was not found"), "Check list_plans"))?;
            if name == "history" { Ok(crate::canon::obj(vec![("plan_id", Value::Int(id)), ("trajectories", Value::Array(vec![]))])) } else { Ok(plan) }
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
            if !store.unwrap().update_plan_status(id, &status) { return Err(Failure::new("plan_not_found", format!("plan {id} was not found"), "")); }
            Ok(crate::canon::obj(vec![("plan_id", Value::Int(id)), ("status", Value::string(status))]))
        }
        "complete_plan" => complete(args, store.unwrap()),
        "record_steps" => {
            let steps = crate::canon::get(args, "steps").and_then(crate::canon::as_array).ok_or_else(|| Failure::new("missing_argument", "steps is required", ""))?;
            let n = store.unwrap().record_steps(steps);
            Ok(crate::canon::obj(vec![("updated", Value::Int(n as i64))]))
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
            if !store.unwrap().deactivate_constraint(id) { return Err(Failure::new("not_found", format!("constraint {id} was not found"), "")); }
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
                Some(row) => Ok(row),
                None => Ok(crate::canon::obj(vec![("recipe", Value::Null)])),
            }
        }
        "list_recipes" => Ok(Value::Array(store.unwrap().list_recipes(crate::canon::get(args, "limit").and_then(crate::canon::as_i64).unwrap_or(50) as usize))),
        "prune_recipes" | "prune_plans" | "validate_recipes" => Ok(crate::canon::obj(vec![("removed", Value::Int(0))])),
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
            let counts = store.map(|s| s.counts()).unwrap_or_else(|| crate::canon::obj(vec![]));
            let value = contract::ok(vec![
                ("store", Value::string(store_path.display().to_string())),
                ("counts", counts),
                ("primary_tools", Value::Array(PRIMARY.iter().copied().map(Value::string).collect())),
                ("tool_count", Value::Int(all_names().len() as i64)),
                ("primary_tool_count", Value::Int(PRIMARY.len() as i64)),
            ]);
            let _ = value;
            Ok(crate::canon::obj(vec![
                ("store", Value::string(store_path.display().to_string())),
                ("counts", store_counts(store_path)),
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
            store.unwrap().claim(plan, step, &agent).map_err(|e| Failure::new("claim_held", e, "Wait for the 10 minute expiry or release it"))?;
            Ok(crate::canon::obj(vec![("claimed", Value::Bool(true)), ("step_id", Value::Int(step))]))
        }
        "release_step" => {
            let step = required_i64(args, "step_id")?;
            let agent = required_str(args, "agent_id")?;
            store.unwrap().release(step, &agent).map_err(|e| Failure::new("claim_mismatch", e, "Pass the agent_id that holds the claim"))?;
            Ok(crate::canon::obj(vec![("released", Value::Bool(true))]))
        }
        "available_steps" => {
            let plan = required_i64(args, "plan_id")?;
            let agent = required_str(args, "agent_id")?;
            Ok(Value::Array(store.unwrap().available(plan, &agent)))
        }
        "merge_plans" => merge(args, store.unwrap()),
        "verify_step" => Ok(verify_step(args, root)),
        "verify_plan" => verify_plan(args, root, store.unwrap()),
        "export_plan" => export_plan(args, store.unwrap()),
        other => Err(Failure::new("unknown_tool", format!("unknown tool {other}"), "")),
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
    if name == "explore" {
        return planner::explore(&request).map_err(|e| Failure::new("decompose_rejected", e, "Relax strict_greenfield or pass a scaffold"));
    }
    let strategy = planner::decompose(&request).map_err(|e| Failure::new("decompose_rejected", e, "Relax strict_greenfield or pass a scaffold"))?;
    if name == "decompose" { return Ok(strategy); }
    let persist = crate::canon::get(args, "persist").and_then(crate::canon::as_bool).unwrap_or(true);
    if !persist {
        return Ok(crate::canon::obj(vec![("strategy", strategy), ("plan_id", Value::Null), ("persisted", Value::Bool(false)), ("workflow", Value::string("start_plan"))]));
    }
    let forced = crate::canon::get(args, "strategy").and_then(crate::canon::as_str).map(|s| s.to_string());
    let mut stored = strategy.clone();
    if let Some(name) = forced { crate::canon::insert(&mut stored, "strategy_name", Value::string(name)); }
    let id = store.ok_or_else(|| Failure::new("store", "store is not open", ""))?.create_plan(&goal, &stored);
    Ok(crate::canon::obj(vec![
        ("strategy", strategy),
        ("plan_id", Value::Int(id)),
        ("persisted", Value::Bool(true)),
        ("workflow", Value::string("start_plan")),
    ]))
}

fn complete(args: &Value, store: &mut Store) -> Result<Value, Failure> {
    let id = required_i64(args, "plan_id")?;
    let outcome = crate::canon::get(args, "outcome").and_then(crate::canon::as_bool).unwrap_or(true);
    let plan = store.get_plan(id).ok_or_else(|| Failure::new("plan_not_found", format!("plan {id} was not found"), ""))?;
    let goal = crate::canon::get(&plan, "goal").and_then(crate::canon::as_str).unwrap_or("").to_string();
    let strategy = crate::canon::get(&plan, "strategy").cloned().unwrap_or(Value::Object(vec![]));
    let sig = store.save_recipe(&goal, &strategy, outcome);
    store.update_plan_status(id, if outcome { "completed" } else { "failed" });
    Ok(crate::canon::obj(vec![("plan_id", Value::Int(id)), ("recipe", Value::string(sig)), ("outcome", Value::Bool(outcome))]))
}

fn verify_step(args: &Value, fallback: &Path) -> Value {
    let root = arg_path(args, "project_root").unwrap_or_else(|| fallback.to_path_buf());
    let edits = edits_of(args);
    let cfg = config::load(&root);
    let language = crate::canon::get(args, "language").and_then(crate::canon::as_str).map(|s| s.to_string()).unwrap_or_else(|| analyze::detect(&root, &cfg));
    let test_cmd = crate::canon::get(args, "test_cmd").and_then(crate::canon::as_array).map(|items| items.iter().filter_map(crate::canon::as_str).map(|s| s.to_string()).collect()).unwrap_or_else(|| config::resolve_test_cmd(None, &cfg, analyze::test_cmd(&language, &root)));
    let full = crate::canon::get(args, "full_output").and_then(crate::canon::as_bool).unwrap_or(false);
    harness::verify(&VerifyRequest { edits, root, test_cmd, timeout: Duration::from_secs(60), full_output: full, cap: harness::default_cap() })
}

fn verify_plan(args: &Value, fallback: &Path, store: &mut Store) -> Result<Value, Failure> {
    let id = required_i64(args, "plan_id")?;
    let plan = store.get_plan(id).ok_or_else(|| Failure::new("plan_not_found", format!("plan {id} was not found"), ""))?;
    let steps = crate::canon::get(&plan, "steps").and_then(crate::canon::as_array).map(|items| items.to_vec()).unwrap_or_default();
    let mut completed = 0i64;
    for step in &steps {
        let content = crate::canon::get(step, "content").and_then(crate::canon::as_str);
        let file = crate::canon::get(step, "file").and_then(crate::canon::as_str);
        if let (Some(file), Some(content)) = (file, content) {
            let mut one = args.clone();
            crate::canon::insert(&mut one, "edits", Value::Array(vec![crate::canon::obj(vec![("path", Value::string(file)), ("content", Value::string(content))])]));
            let verdict = verify_step(&one, fallback);
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
    let plan = store.get_plan(id).ok_or_else(|| Failure::new("plan_not_found", format!("plan {id} was not found"), ""))?;
    let doc = crate::export::from_plan(&plan);
    if let Some(path) = crate::canon::get(args, "path").and_then(crate::canon::as_str) {
        std::fs::write(path, crate::canon::stable_pretty(&doc)).map_err(|e| Failure::new("export", e.to_string(), "Check the path is writable"))?;
    }
    Ok(doc)
}

fn merge(args: &Value, store: &mut Store) -> Result<Value, Failure> {
    let a = required_i64(args, "plan_a_id")?;
    let b = required_i64(args, "plan_b_id")?;
    let left = store.get_plan(a).ok_or_else(|| Failure::new("plan_not_found", format!("plan {a} was not found"), ""))?;
    let right = store.get_plan(b).ok_or_else(|| Failure::new("plan_not_found", format!("plan {b} was not found"), ""))?;
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

fn store_counts(path: &Path) -> Value {
    Store::open_shared(path).map(|s| s.counts()).unwrap_or_else(|_| crate::canon::obj(vec![]))
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
    crate::canon::get(args, key).and_then(crate::canon::as_str).map(|s| s.to_string()).ok_or_else(|| Failure::new("missing_argument", format!("{key} is required"), ""))
}

fn required_i64(args: &Value, key: &str) -> Result<i64, Failure> {
    crate::canon::get(args, key).and_then(crate::canon::as_i64).ok_or_else(|| Failure::new("missing_argument", format!("{key} is required"), "Pass an integer"))
}

fn arg_path(args: &Value, key: &str) -> Option<PathBuf> {
    crate::canon::get(args, key).and_then(crate::canon::as_str).map(PathBuf::from)
}

pub fn surface(name: &str) -> &'static str {
    if PRIMARY.contains(&name) { "primary" } else { "advanced" }
}

pub fn hints(name: &str) -> (bool, bool, bool, bool) {
    let read_only = matches!(name, "decompose" | "explore" | "get_plan" | "resume" | "get_recipe" | "list_recipes" | "list_strategies" | "get_constraints" | "status" | "estimate" | "usage_stats" | "history" | "list_plans" | "failure_history" | "available_steps" | "export_plan" | "validate_recipes");
    let _ = ProjectConfig::default();
    (read_only, !read_only, false, false)
}
