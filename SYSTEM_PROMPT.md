# Trammel system prompt for coding agents

Paste this into an agent's instructions (or a project `CLAUDE.md`) when Trammel is connected as an MCP server. It describes Trammel 4.x. The same tools are CLI subcommands: `trammel <tool> --<arg> <value>` prints the same JSON.

## What Trammel is for

Trammel does not write code; you do. It gives you three things:

1. **A plan in dependency order.** `start_plan` reads the project's imports and symbols and returns the files a goal touches, ordered so that each file comes after the files it depends on, with each file's goal relevance. The plan is saved in `.trammel/` with a `plan_id`.
2. **A safe check of your edits.** `verify_step` writes your edited files into a copy of the project kept for this purpose, checks bracket balance, and runs the project's tests there. The working tree is never touched.
3. **Memory across sessions and agents.** Plans, step status, claims, handoff notes, constraints (rules), failures, and successful strategies (recipes) stay in `.trammel/`, so a later session or another agent can pick up where the last one stopped.

Use it for changes that span several files, for work you may hand to another agent or finish in another session, and when earlier attempts failed and you want to record why. For a one-file edit, plain editing and running the tests is enough.

## The loop

1. `get_constraints` and `get_recipe(goal)`: read rules and any strategy that worked for a similar goal.
2. `start_plan(goal)`: keep the `plan_id`. Pass `summary_only: true` for a short reply; the saved plan still has every step.
3. For each step, in order: edit the file, then `verify_step(edits: [{path, content}])` with the whole new content of each changed file. If `success` is false, read `preflight.issues` or `trace`, fix, and verify again.
4. Record what you learn: `add_constraint` for a rule the next agent must follow, `resolve_failure` for what fixed an error.
5. `complete_plan(plan_id)` when done (`outcome: false` if you gave up). It saves the strategy as a recipe.

## Handing off

- Before you stop, `add_note(plan_id, text)`: what you did, what is left, what to watch out for. Name files and functions.
- Give the next agent the `plan_id`. It calls `get_plan(plan_id)` (or `resume`) and gets the steps with their status and claims, plus all notes, oldest first.
- Steps the plan has not reached are `pending`. Mark progress with `record_steps(steps: [{step_id, status}])`, status `passed`, `failed`, `skipped`, `running` or `pending`.

## Several agents on one plan

These tools are listed with `TRAMMEL_MCP_SURFACE=all`, and are callable by name either way.

- `available_steps(plan_id, agent_id)` returns the steps you can take now: pending or running, not claimed by another agent, and every step they depend on `passed` or `skipped`.
- `claim_step(plan_id, step_id, agent_id)` before you start. A claim held by someone else returns `claim_held`, and a step whose dependencies are not done returns `step_blocked` with the blocking step ids; pick another step. Claims expire after 10 minutes; claiming again refreshes yours.
- `record_steps` when a step passes, then `release_step(step_id, agent_id)`.
- Use a stable `agent_id` per agent, such as its role or session name.

## Conventions

- **Step ids.** A step has an `id` (unique in the store) and a `step_index` (its position in the plan). `claim_step`, `release_step`, `record_steps` and `add_note` take the `id`. `depends_on` lists step indexes.
- **Results.** Every result is one JSON object. Lists come as `{"items": [...]}` over MCP. Plan and recipe replies are compact by default; pass `full: true` to `start_plan`, `decompose`, `get_plan`, `resume` or `get_recipe` when you need symbols, rationale or the import graph.
- **Errors.** A failed call returns `isError: true` and `{"code", "message", "hint"}`. The hint says what to do next, for example which argument is missing or that a number was a `step_index` rather than an `id`.
- **Project.** Tools work on the project in the server's working directory. Pass `project_root` to use another one.
- **Test command.** `verify_step` uses its `test_cmd` argument, then `test_cmd` from `.trammel.json`, then the language default (`python -m unittest`, `cargo test`, `npm test`, `go test ./...`, ...). Output is capped at 4 KB (head and tail) unless `full_output` is true.
- **Time limit.** `timeout_s`, else `verify_timeout_s` in `.trammel.json`, else 300 seconds. A call that waits is capped at 600 seconds. For a longer run (a cold build of a large workspace) pass `background: true`: the reply is a `job_id` at once; call `job_status(job_id, wait_s)` until `status` is `completed`, and `cancel_job` to stop it. A run past its limit answers `timed_out: true` with `elapsed_s` and a hint. The copy and its cargo target dir are kept per project, so later runs rebuild only what changed.
- **Named files.** Name the files in the goal (`crates/app/src/sight.rs`, `game.rs`, `` `timeline` ``, `app::sight`) or pass `files`. The plan starts from them (`role: named`, relevance 1.0), adds a few direct dependencies, dependents and tests ranked below them, and lists code before tests. `analysis_meta` reports `named_files`, and `unresolved_files` or `ambiguous_files` when a name matched no file or several.
- **Large repos.** Without named files, a plan keeps the 15 files most relevant to the goal, code first and tests after. Pass `scope` (a subdirectory) or `focus_globs` to aim it, or `expand_repo: false` to lift the cap. `estimate` counts files first.

## Tool groups

The server may hide some groups. A hidden tool answers with an error naming what hid it. `core` is always there.

You can choose the groups for the project yourself: add an `agent-tools` block to the project's AGENTS.md, for example `trammel: disable locks, notes` when you are the only agent, or `trammel: tools core, verify` to keep only planning and verification. The server picks up the change on your next request and the client refreshes the tool list. If a line has a typo, the `initialize` instructions and `status` show a warning and the line is ignored. Env vars set by the operator (`TRAMMEL_TOOLS`, `TRAMMEL_DISABLE`) win over the file.

| Group | Tools | Overlaps with |
|---|---|---|
| `core` (always on) | `start_plan`, `decompose`, `get_plan`, `resume`, `complete_plan`, `status` | |
| `plan` | `explore`, `list_plans`, `update_plan_status`, `merge_plans`, `export_plan`, `record_steps`, `history`, `estimate`, `list_strategies` | |
| `verify` | `verify_step`, `verify_plan`, `job_status`, `cancel_job` | Chisel `start_job` / `job_status` (same shape) |
| `recipes` | `save_recipe`, `get_recipe`, `list_recipes` | |
| `constraints` | `add_constraint`, `get_constraints`, `deactivate_constraint`, `resolve_failure`, `failure_history` | |
| `locks` | `claim_step`, `release_step`, `available_steps` | Chisel locks, CoordinationHub leases |
| `notes` | `add_note` | wikifier journal notes |
| `maintenance` | `prune_recipes`, `prune_plans`, `validate_recipes`, `usage_stats` | |

### Choosing groups

Add a line to the project's own AGENTS.md (its root, next to `.git`). The server re-reads it on every request and tells the client the tool list changed, so no restart is needed.

````markdown
```agent-tools
trammel: disable locks, notes
```
````

| Group | For | Turn it off when |
|---|---|---|
| `core` | start_plan, get_plan, resume, complete_plan, decompose, status | Never (always on). |
| `plan` | alternative orderings, plan lists, merging, export, estimates | You only start, read and complete plans. |
| `verify` | running the tests on a copy with your edits | You run the tests yourself every time. |
| `recipes` | reusing strategies that worked for similar goals | Work is one-off and nothing repeats. |
| `constraints` | rules and failure history every agent reads | The project keeps its rules elsewhere. |
| `locks` | claims so several agents don't take the same step | One agent works on the project, or CoordinationHub manages leases. |
| `notes` | handoff notes on a plan | One session does the whole job, or handoffs go through another tool (wikifier journal, CoordinationHub messages). |
| `maintenance` | pruning old plans and recipes, usage counts | Almost always; turn it on to clean up a long-lived store. |

Use `trammel: tools <groups>` instead to keep only what you name (plus `core`). Lists take group or tool names, separated by commas or spaces. A wrong name is ignored with a warning in the `initialize` instructions and in `status`; it never stops the server. `TRAMMEL_TOOLS` / `TRAMMEL_DISABLE` in the server's `env` override the matching line.

## Tool reference

### Primary tools (listed by default)

| Tool | Arguments (**required**) | What it does |
|---|---|---|
| `add_constraint` | **constraint_type**, context, **description**, plan_id, step_id | Record a rule or known-bad approach (e.g. do not edit generated files) so every later agent sees it via get_constraints. |
| `add_note` | agent_id, **plan_id**, step_id, **text** | Leave a free-text note on a plan for whoever picks it up next: what you did, what is left, what to watch out for. get_plan and resume return the notes. |
| `cancel_job` | **job_id** | Stop a running verify_step background job; its test command is killed. job_status then returns the cancelled result. |
| `complete_plan` | outcome, **plan_id** | Close a plan as completed (or failed) and save its strategy as a recipe for similar goals. Step statuses are left as they are. |
| `decompose` | expand_repo, files, focus_globs, focus_keywords, full, **goal**, language, max_files, max_steps, num_beams, relevant_only, scope, strict_greenfield, summary_only, suppress_creation_hints | Plan a change without saving it: which files it touches and in what order (dependencies first, tests last), with goal relevance and each file's role (named, dependency, dependent, test). Read-only. Same options as start_plan. Not worth it for a single-file fix. full: true adds symbols and the import graph. |
| `export_plan` | path, **plan_id** | Return a saved plan as a trammel.plan document, and write it to path when given. |
| `get_constraints` | constraint_type | List active constraints. Read these before editing. |
| `get_plan` | full, **plan_id** | Load a saved plan with its steps, step status, claims and handoff notes. Use it to pick up work another agent or session left. |
| `get_recipe` | full, **goal** | Return the saved recipe whose goal best matches this one, paraphrases included (similarity at least 0.55), or {recipe: null}. |
| `job_status` | **job_id**, wait_s | State of a verify_step background job: running (with elapsed_s), completed or cancelled (with the verify result), or not_found. wait_s waits here, up to 600 s, for it to finish. |
| `resume` | full, **plan_id** | Same as get_plan: load a saved plan to continue it. |
| `save_recipe` | **goal**, outcome, strategy | Save a strategy that worked so get_recipe can return it for similar goals. |
| `start_plan` | expand_repo, files, focus_globs, focus_keywords, full, **goal**, language, max_files, max_steps, num_beams, persist, relevant_only, scope, strategy, strict_greenfield, summary_only, suppress_creation_hints | Plan a change that spans several files and save it. Use it when the files must change in dependency order, or when another agent or a later session will continue the work (get_plan, add_note). Skip it for a one- or two-file edit you can make now. Name the files in the goal or pass files: the plan starts from them, adds a few direct dependencies, dependents and tests ranked below them, and lists code before tests, each step with a role. Without named files it ranks the whole repo by goal words (15 files). Returns plan_id. |
| `status` | none | Store location and counts of plans, steps, recipes, constraints and failures. |
| `verify_step` | background, edits, full_output, language, test_cmd, timeout_s | Run the tests against edits in a copy of the project; the real tree is never touched. Use it to check an edit before writing it, or when other agents share your working tree; if you can just run the tests yourself, do that. The copy and its cargo target dir are kept per project, so later runs rebuild only what changed. Unbalanced brackets fail at once with line numbers. A timeout answers timed_out with elapsed_s and how to raise timeout_s. Long runs: background: true returns a job_id at once; poll job_status. Output capped at 4 KB. |

### Advanced tools (`TRAMMEL_MCP_SURFACE=all`; always callable by name)

| Tool | Arguments (**required**) | What it does |
|---|---|---|
| `available_steps` | **agent_id**, **plan_id** | Steps of a plan that are ready: pending or running, not claimed by another agent, and every step they depend on passed or skipped. |
| `claim_step` | **agent_id**, **plan_id**, **step_id** | Claim a step for one agent so others skip it. Refused (step_blocked) until the steps it depends on are passed or skipped. A claim expires after 10 minutes; claiming again refreshes it. |
| `deactivate_constraint` | **constraint_id** | Turn off a constraint that no longer applies. |
| `estimate` | language, scope | Count the source files a plan would analyze, to decide whether to pass scope first. |
| `explore` | expand_repo, files, focus_globs, focus_keywords, **goal**, language, max_files, max_steps, num_beams, relevant_only, scope, strict_greenfield, summary_only, suppress_creation_hints | Plan a change and return num_beams alternative step orderings (bottom_up, top_down, ...). Read-only. |
| `failure_history` | file_path, limit | Past failures and their fixes, optionally for one file. Check before editing a file that failed before. |
| `history` | **plan_id** | Beam outcomes recorded for a plan by the goal command (`trammel GOAL`) or migrated from 3.x: variant, steps completed, success, failure reason. |
| `list_plans` | status | List saved plans newest first (id, goal, status, step counts, times), optionally by status. |
| `list_recipes` | limit | List saved recipes, most recently updated first. |
| `list_strategies` | none | List the nine built-in beam strategies explore can use. |
| `merge_plans` | **plan_a_id**, **plan_b_id**, strategy | Save a new plan whose steps are plan A's then plan B's (or interleaved). |
| `prune_plans` | max_age_days, status | Delete plans not updated for max_age_days with the given status, plus their steps, notes, trajectories and constraints. Returns removed. |
| `prune_recipes` | max_age_days, min_success_ratio | Delete recipes not updated for max_age_days whose success ratio is below min_success_ratio (or that never succeeded). Returns removed. |
| `record_steps` | **steps** | Set the status of plan steps, e.g. after verify_step passes. Unknown ids are listed in not_found. |
| `release_step` | **agent_id**, **step_id** | Drop your claim on a step so another agent can take it. |
| `resolve_failure` | **error_type**, **file_path**, plan_id, **resolution**, step_id | Record what fixed an error in a file, so failure_history can tell the next agent. |
| `update_plan_status` | **plan_id**, **status** | Set a plan's status, e.g. failed when abandoning it. |
| `usage_stats` | none | Count of local usage events, recorded only when TRAMMEL_TELEMETRY=1. |
| `validate_recipes` | none | Check every recipe's files against the project; delete recipes whose files are all gone. Returns recipes_checked, files_missing, removed. |
| `verify_plan` | full_output, language, **plan_id**, test_cmd, timeout_s | Run verify_step for each step of a saved plan that carries file content, in order, stopping at the first failure. Steps without content count as done. The whole call stops after 600 s with timed_out. |
