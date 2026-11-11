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

Argument schemas are served by `tools/list` (`src/catalogue.rs`). The result envelope files live in `schemas/`.
