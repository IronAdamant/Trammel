//! What `tools/list` tells a model: one line on when to call each tool and the
//! arguments its handler in `src/tools.rs` reads, as JSON Schema.

use crate::canon::obj;
use crate::json::Value;

/// `(name, type, required, description)`. Types: `string`, `integer`,
/// `boolean`, `object`, `string[]`, `object[]`, `edits`, `steps`, `cmd`, or
/// `a|b|c` for a string enum.
type Param = (&'static str, &'static str, bool, &'static str);

const ROOT: Param = ("project_root", "string", false, "Project directory. Default: the server's working directory.");

const PLAN: &[Param] = &[
    ("goal", "string", true, "What to change, in plain words. Name the files when you know them (paths, file names such as game.rs, `module` names, crate::paths): the plan starts from those."),
    ("files", "string[]", false, "Files to plan from, relative to project_root. Added to the files the goal names."),
    ("scope", "string", false, "Subdirectory to analyze, relative to project_root."),
    ("language", "string", false, "Override detection: python, typescript, javascript, go, rust, cpp, c, java, kotlin, csharp, ruby, php, swift, dart, zig."),
    ("focus_keywords", "string[]", false, "Extra words that mark files as relevant."),
    ("focus_globs", "string[]", false, "Only plan files matching these globs, e.g. src/api/**."),
    ("relevant_only", "boolean", false, "Order steps by goal relevance instead of dependency order."),
    ("summary_only", "boolean", false, "Return file list and counts, not full steps."),
    ("max_steps", "integer", false, "Cap the number of steps returned."),
    ("max_files", "integer", false, "Cap analyzed files (at most 15 unless expand_repo is false)."),
    ("expand_repo", "boolean", false, "Default true: cap a whole-repo plan at the 15 most goal-relevant files. false lifts the cap."),
    ("num_beams", "integer", false, "Beam count for explore, 3 to 12. Default 3."),
    ("strict_greenfield", "boolean", false, "Reject create-only goals that name no file paths."),
    ("suppress_creation_hints", "boolean", false, "Omit creation_hints from the result."),
];

const FULL: Param = ("full", "boolean", false, "Return every field (symbols, rationale, dependency graph, analysis details). Default false: a compact reply.");

const PLAN_ID: Param = ("plan_id", "integer", true, "Plan id from start_plan or list_plans.");
const STEP_ID: Param = ("step_id", "integer", true, "The step's `id` from get_plan or available_steps (not step_index).");
const AGENT_ID: Param = ("agent_id", "string", true, "Stable name for the calling agent, e.g. its session or role.");

const VERIFY: &[Param] = &[
    ("test_cmd", "cmd", false, "Test command as a string or argv array. Default: .trammel.json test_cmd, else the language default (python -m unittest, cargo test, npm test, go test ./...)."),
    ("language", "string", false, "Override language detection for the default test command."),
    ("full_output", "boolean", false, "Return the whole test output instead of 4 KB head and tail."),
    ("timeout_s", "number", false, "Seconds the test command may run. Default: verify_timeout_s in .trammel.json, else 300. A call that waits is capped at 600; with background: true, 3600."),
];

struct Tool {
    name: &'static str,
    description: &'static str,
    params: &'static [Param],
    extra: &'static [&'static [Param]],
}

const TOOLS: &[Tool] = &[
    Tool { name: "start_plan", description: "Plan a change that spans several files and save it. Use it when the files must change in dependency order, or when another agent or a later session will continue the work (get_plan, add_note). Skip it for a one- or two-file edit you can make now. Name the files in the goal or pass files: the plan starts from them, adds a few direct dependencies, dependents and tests ranked below them, and lists code before tests, each step with a role. Without named files it ranks the whole repo by goal words (15 files). Returns plan_id.",
        params: &[("persist", "boolean", false, "Default true. false returns the plan without saving it."), ("strategy", "string", false, "Strategy name to record on the saved plan."), FULL], extra: &[PLAN] },
    Tool { name: "decompose", description: "Plan a change without saving it: which files it touches and in what order (dependencies first, tests last), with goal relevance and each file's role (named, dependency, dependent, test). Read-only. Same options as start_plan. Not worth it for a single-file fix. full: true adds symbols and the import graph.",
        params: &[FULL], extra: &[PLAN] },
    Tool { name: "explore", description: "Plan a change and return num_beams alternative step orderings (bottom_up, top_down, ...). Read-only.",
        params: &[], extra: &[PLAN] },
    Tool { name: "get_plan", description: "Load a saved plan with its steps, step status, claims and handoff notes. Use it to pick up work another agent or session left.",
        params: &[PLAN_ID, FULL], extra: &[] },
    Tool { name: "resume", description: "Same as get_plan: load a saved plan to continue it.",
        params: &[PLAN_ID, FULL], extra: &[] },
    Tool { name: "history", description: "Beam outcomes recorded for a plan by the goal command (`trammel GOAL`) or migrated from 3.x: variant, steps completed, success, failure reason.",
        params: &[PLAN_ID], extra: &[] },
    Tool { name: "add_note", description: "Leave a free-text note on a plan for whoever picks it up next: what you did, what is left, what to watch out for. get_plan and resume return the notes.",
        params: &[PLAN_ID, ("text", "string", true, "The note, a few sentences."), ("agent_id", "string", false, "Who wrote it."), ("step_id", "integer", false, "The step `id` it is about.")], extra: &[] },
    Tool { name: "list_plans", description: "List saved plans newest first (id, goal, status, step counts, times), optionally by status.",
        params: &[("status", "pending|running|completed|failed", false, "Only plans with this status.")], extra: &[] },
    Tool { name: "update_plan_status", description: "Set a plan's status, e.g. failed when abandoning it.",
        params: &[PLAN_ID, ("status", "pending|running|completed|failed", true, "New status.")], extra: &[] },
    Tool { name: "complete_plan", description: "Close a plan as completed (or failed) and save its strategy as a recipe for similar goals. Step statuses are left as they are.",
        params: &[PLAN_ID, ("outcome", "boolean", false, "Default true. false records a failure.")], extra: &[] },
    Tool { name: "merge_plans", description: "Save a new plan whose steps are plan A's then plan B's (or interleaved).",
        params: &[("plan_a_id", "integer", true, "First plan id."), ("plan_b_id", "integer", true, "Second plan id."), ("strategy", "sequential|interleave", false, "Default sequential.")], extra: &[] },
    Tool { name: "export_plan", description: "Return a saved plan as a trammel.plan document, and write it to path when given.",
        params: &[PLAN_ID, ("path", "string", false, "File to write the JSON to.")], extra: &[] },
    Tool { name: "verify_step", description: "Run the tests against edits in a copy of the project; the real tree is never touched. Use it to check an edit before writing it, or when other agents share your working tree; if you can just run the tests yourself, do that. The copy and its cargo target dir are kept per project, so later runs rebuild only what changed. Unbalanced brackets fail at once with line numbers. A timeout answers timed_out with elapsed_s and how to raise timeout_s. Long runs: background: true returns a job_id at once; poll job_status. Output capped at 4 KB.",
        params: &[("edits", "edits", false, "Files to write in the copy: [{path, content}], content is the whole new file."), ("background", "boolean", false, "Return a job_id at once and run in the background (up to 3600 s); read the result with job_status. Needs the MCP server.")], extra: &[VERIFY] },
    Tool { name: "job_status", description: "State of a verify_step background job: running (with elapsed_s), completed or cancelled (with the verify result), or not_found. wait_s waits here, up to 600 s, for it to finish.",
        params: &[("job_id", "string", true, "Id from verify_step with background: true."), ("wait_s", "number", false, "Seconds to wait for the job to finish before answering. Default 0, at most 600.")], extra: &[] },
    Tool { name: "cancel_job", description: "Stop a running verify_step background job; its test command is killed. job_status then returns the cancelled result.",
        params: &[("job_id", "string", true, "Id from verify_step with background: true.")], extra: &[] },
    Tool { name: "verify_plan", description: "Run verify_step for each step of a saved plan that carries file content, in order, stopping at the first failure. Steps without content count as done. The whole call stops after 600 s with timed_out.",
        params: &[PLAN_ID], extra: &[VERIFY] },
    Tool { name: "record_steps", description: "Set the status of plan steps, e.g. after verify_step passes. Unknown ids are listed in not_found.",
        params: &[("steps", "steps", true, "[{step_id, status}]. step_id is the step `id` from get_plan. Status: pending, running, passed, failed or skipped.")], extra: &[] },
    Tool { name: "claim_step", description: "Claim a step for one agent so others skip it. Refused (step_blocked) until the steps it depends on are passed or skipped. A claim expires after 10 minutes; claiming again refreshes it.",
        params: &[("plan_id", "integer", true, "Plan id."), STEP_ID, AGENT_ID], extra: &[] },
    Tool { name: "release_step", description: "Drop your claim on a step so another agent can take it.",
        params: &[STEP_ID, AGENT_ID], extra: &[] },
    Tool { name: "available_steps", description: "Steps of a plan that are ready: pending or running, not claimed by another agent, and every step they depend on passed or skipped.",
        params: &[("plan_id", "integer", true, "Plan id."), AGENT_ID], extra: &[] },
    Tool { name: "save_recipe", description: "Save a strategy that worked so get_recipe can return it for similar goals.",
        params: &[("goal", "string", true, "The goal the strategy solved."), ("strategy", "object", false, "The strategy object, e.g. a decompose result."), ("outcome", "boolean", false, "Default true.")], extra: &[] },
    Tool { name: "get_recipe", description: "Return the saved recipe whose goal best matches this one, paraphrases included (similarity at least 0.55), or {recipe: null}.",
        params: &[("goal", "string", true, "Goal to match."), FULL], extra: &[] },
    Tool { name: "list_recipes", description: "List saved recipes, most recently updated first.",
        params: &[("limit", "integer", false, "Default 50.")], extra: &[] },
    Tool { name: "prune_recipes", description: "Delete recipes not updated for max_age_days whose success ratio is below min_success_ratio (or that never succeeded). Returns removed.",
        params: &[("max_age_days", "number", false, "Default 90."), ("min_success_ratio", "number", false, "Default 0.1.")], extra: &[] },
    Tool { name: "validate_recipes", description: "Check every recipe's files against the project; delete recipes whose files are all gone. Returns recipes_checked, files_missing, removed.",
        params: &[], extra: &[] },
    Tool { name: "prune_plans", description: "Delete plans not updated for max_age_days with the given status, plus their steps, notes, trajectories and constraints. Returns removed.",
        params: &[("max_age_days", "number", false, "Default 7."), ("status", "string", false, "Default pending. null means any status.")], extra: &[] },
    Tool { name: "list_strategies", description: "List the nine built-in beam strategies explore can use.", params: &[], extra: &[] },
    Tool { name: "add_constraint", description: "Record a rule or known-bad approach (e.g. do not edit generated files) so every later agent sees it via get_constraints.",
        params: &[("constraint_type", "string", true, "Short category, e.g. avoid, dependency, incompatible."), ("description", "string", true, "The rule, in one sentence."), ("context", "object", false, "Any extra JSON."), ("plan_id", "integer", false, "Tie it to a plan."), ("step_id", "integer", false, "Tie it to a step id.")], extra: &[] },
    Tool { name: "get_constraints", description: "List active constraints. Read these before editing.",
        params: &[("constraint_type", "string", false, "Only this category.")], extra: &[] },
    Tool { name: "deactivate_constraint", description: "Turn off a constraint that no longer applies.",
        params: &[("constraint_id", "integer", true, "Id from get_constraints.")], extra: &[] },
    Tool { name: "resolve_failure", description: "Record what fixed an error in a file, so failure_history can tell the next agent.",
        params: &[("file_path", "string", true, "File that failed, relative to project_root."), ("error_type", "string", true, "e.g. ImportError, test_failure."), ("resolution", "string", true, "What fixed it."), ("plan_id", "integer", false, "Related plan."), ("step_id", "integer", false, "Related step id.")], extra: &[] },
    Tool { name: "failure_history", description: "Past failures and their fixes, optionally for one file. Check before editing a file that failed before.",
        params: &[("file_path", "string", false, "Only this file."), ("limit", "integer", false, "Default 50.")], extra: &[] },
    Tool { name: "status", description: "Store location and counts of plans, steps, recipes, constraints and failures.", params: &[], extra: &[] },
    Tool { name: "estimate", description: "Count the source files a plan would analyze, to decide whether to pass scope first.",
        params: &[("language", "string", false, "Override language detection."), ("scope", "string", false, "Subdirectory to count.")], extra: &[] },
    Tool { name: "usage_stats", description: "Count of local usage events, recorded only when TRAMMEL_TELEMETRY=1.", params: &[], extra: &[] },
];

pub fn description(name: &str) -> &'static str {
    find(name).map(|t| t.description).unwrap_or("")
}

pub fn input_schema(name: &str) -> Value {
    let mut properties = Vec::new();
    let mut required = Vec::new();
    if let Some(tool) = find(name) {
        for (key, ty, needed, text) in tool.params.iter().chain(tool.extra.iter().flat_map(|group| group.iter())).chain(std::iter::once(&ROOT)) {
            if properties.iter().any(|(k, _): &(String, Value)| k == key) { continue; }
            properties.push((key.to_string(), property(ty, text)));
            if *needed { required.push(Value::string(*key)); }
        }
    }
    let mut fields = vec![("type", Value::string("object")), ("properties", Value::Object(properties))];
    if !required.is_empty() { fields.push(("required", Value::Array(required))); }
    obj(fields)
}

/// Names of the required arguments, for tests and error hints.
pub fn required(name: &str) -> Vec<&'static str> {
    find(name).map(|tool| tool.params.iter().chain(tool.extra.iter().flat_map(|g| g.iter())).filter(|p| p.2).map(|p| p.0).collect()).unwrap_or_default()
}

pub fn names() -> Vec<&'static str> {
    TOOLS.iter().map(|t| t.name).collect()
}

fn find(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.name == name)
}

fn property(ty: &str, text: &str) -> Value {
    let d = ("description", Value::string(text));
    let string = || obj(vec![("type", Value::string("string"))]);
    match ty {
        "string[]" => obj(vec![("type", Value::string("array")), ("items", string()), d]),
        "object[]" => obj(vec![("type", Value::string("array")), ("items", obj(vec![("type", Value::string("object"))])), d]),
        "cmd" => obj(vec![("type", Value::Array(vec![Value::string("string"), Value::string("array")])), ("items", string()), d]),
        "edits" => obj(vec![
            ("type", Value::string("array")),
            ("items", obj(vec![
                ("type", Value::string("object")),
                ("properties", obj(vec![("path", string()), ("content", string())])),
                ("required", Value::Array(vec![Value::string("path"), Value::string("content")])),
            ])),
            d,
        ]),
        "steps" => obj(vec![
            ("type", Value::string("array")),
            ("items", obj(vec![
                ("type", Value::string("object")),
                ("properties", obj(vec![("step_id", obj(vec![("type", Value::string("integer"))])), ("status", string())])),
                ("required", Value::Array(vec![Value::string("step_id")])),
            ])),
            d,
        ]),
        enumerated if enumerated.contains('|') => obj(vec![
            ("type", Value::string("string")),
            ("enum", Value::Array(enumerated.split('|').map(Value::string).collect())),
            d,
        ]),
        plain => obj(vec![("type", Value::string(plain)), d]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_description_and_schema() {
        let mut listed = names();
        listed.sort();
        assert_eq!(listed, crate::tools::all_names());
        for name in names() {
            assert!(description(name).len() > 20, "{name} needs a real description");
            let schema = input_schema(name);
            let props = crate::canon::get(&schema, "properties").and_then(|p| if let Value::Object(f) = p { Some(f.len()) } else { None }).unwrap_or(0);
            assert!(props >= 1, "{name} has no properties");
        }
    }

    #[test]
    fn required_arguments_match_what_the_handlers_demand() {
        // Call each tool with nothing, then add whatever it says is missing.
        // The arguments it asks for must be exactly the schema's `required`.
        let dir = std::env::temp_dir().join(format!("trammel-catalogue-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = dir.join(".trammel");
        let no_op = Value::Array(crate::harness::no_op_cmd().into_iter().map(Value::String).collect());
        for name in names() {
            let mut args = obj(vec![("project_root", Value::string(dir.display().to_string())), ("test_cmd", no_op.clone())]);
            let mut asked = Vec::new();
            for _ in 0..8 {
                match crate::tools::call(name, &args, &dir, Some(&store), false) {
                    Err(failure) if failure.code == "missing_argument" => {
                        let key = failure.message.split(' ').next().unwrap().to_string();
                        assert!(failure.hint.contains(&key), "{name}: hint should name {key}: {}", failure.hint);
                        let ty = find(name).unwrap().params.iter().chain(find(name).unwrap().extra.iter().flat_map(|g| g.iter())).find(|p| p.0 == key).map(|p| p.1).unwrap_or("string");
                        let dummy = match ty {
                            "integer" => Value::Int(1),
                            "steps" => Value::Array(vec![obj(vec![("step_id", Value::Int(1)), ("status", Value::string("passed"))])]),
                            t if t.contains('|') => Value::string(t.split('|').next().unwrap()),
                            _ => Value::string("x"),
                        };
                        crate::canon::insert(&mut args, &key, dummy);
                        asked.push(key);
                    }
                    _ => break,
                }
            }
            asked.sort();
            let mut want: Vec<String> = required(name).into_iter().map(String::from).collect();
            want.sort();
            assert_eq!(asked, want, "{name}");
        }
        crate::workspace::forget(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
