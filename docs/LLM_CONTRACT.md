# Trammel JSON contract 4.0.0

Every CLI command and MCP tool returns one object.

Success:

```json
{"ok":true,"schema_version":"4.0.0"}
```

The command's fields sit next to those two. Keys are sorted. Output is one compact line unless `--human` is set.

Failure:

```json
{"ok":false,"schema_version":"4.0.0","code":"plan_not_found","message":"plan 3 was not found","hint":"Call list_plans for valid plan ids","retryable":false}
```

Exit codes: `0` success, `1` failure, `2` usage. MCP puts the same object in `structuredContent` and sets `isError` to true on failure. A tool whose result is a list sends `{"items": [...]}` over MCP, because `structuredContent` must be an object.

`schema_version` is `4.0.0`. The Rust MCP surface is 35 tools: `verify_plan`, `add_note`, `job_status` and `cancel_job` are included, and `record_step` / `create_plan` are not. Breaking changes wait for 5.0. `trammel.plan` exports keep `format_version` 1. The npm package is `@iron_adamant/trammel` (`npm i -g @iron_adamant/trammel`, `npx -y @iron_adamant/trammel mcp`). The installed commands stay `trammel` and `trammel-mcp`. The registry name is `io.github.IronAdamant/trammel` (`server.json`).

The old Python version lives on the `python-legacy` branch.

Removed names `record_step` and `create_plan` return `code` `tool_removed` and name the replacement. `list_plans` does not accept `db_path`. A leftover `trammel.db` returns `legacy_sqlite` until `trammel migrate`.

`verify_step` caps `trace` at 4 KB (head and tail) and sets `truncated` when it cuts. Pass `full_output` true for the whole trace.

Since 4.5.0 a `verify_step` result that ran the tests also has:

- `copy`: `reused`, `files_written`, `path` (where the tests ran), `target` (the build output dir), `longest_path` (characters of the longest path a build would create there). `path_warning` is added on Windows from 240 characters. Past 259 the call fails with `code` `path_too_long`, and the hint names `TRAMMEL_VERIFY_CACHE`.
- `source`: `working_tree`, or `commit` with `commit` (the full id) and `ref` (as passed). A `ref` that names no commit fails with `bad_ref`.
- `left_behind`: `created`, `modified`, `deleted` counts; `line_endings_only` when some changes were only CRLF/LF; `files` with up to 5 `{path, change, now?}` (`change` is `created`, `modified`, `line_endings` or `deleted`; `now` is `LF`, `CRLF` or `mixed`); `files_more` for the rest.
- `failed_tests` on a failure the output shows: up to 8 `{name, excerpt}`, each excerpt at most 400 characters; `failed_tests_more` for the rest.
- `recorded: {plan_id, step_id}` when the call passed both (or `record_error`). The step then has `verification`: `passed`, `command`, `source`, `commit` and `ref` for a commit, `at` (Unix seconds), `elapsed_s`, and `excerpt` (at most 300 characters) for a failure. `get_plan`, `resume` and `export_plan` show it. `resume` adds `next_step` (the first step with no passing verification that is not passed or skipped, or null) and `next`.

After 4.5.1:

- `start_plan` steps carry their `id` (the `step_id` other tools take). With `summary_only` the reply has `steps: [{id, step_index, file, action?, description}]`. `next` names the first step.
- A step for a file that does not exist yet has `role: "create"` and `action: "create"`; `analysis_meta.new_files` lists them, and `plan_source` is `named_files`.
- `recorded` is `{plan_id, step_id, status}`. A pass sets a pending, running or failed step to `passed`; a failure sets a passed step to `failed`; `skipped` and `completed` stay. The plan's `current_step` follows the first step not done, and a `pending` plan becomes `running`.
- `cached: {at, note}` on a `verify_step` reply that was not run again (see SYSTEM_PROMPT.md, "Cached passes"); the step's `verification` then has `cached: true`. `rerun: true` always runs.
- `complete_plan` takes `verify`, `note`, `test_cmd`, `ref` (and `timeout_s`, `language`, `full_output`, `rerun` as verify_step). The reply adds `completed: true`, `verify` (`success`, `source`, `commit`, `ref`, `elapsed_s`, `cached`, `left_behind`) and `note_id`. A failed verify returns `{plan_id, completed: false, verify: <the verify_step reply>, hint}` and changes nothing.

Argument schemas are served by `tools/list` (`src/catalogue.rs`). The result envelope files live in `schemas/`.
