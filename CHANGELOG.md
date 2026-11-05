# Changelog

The old Python version lives on the `python-legacy` branch. Notes below 4.0.0 describe that package. Older Python releases are also in [docs/PYTHON_CHANGELOG.md](docs/PYTHON_CHANGELOG.md) and `LLM_Development.md`.

## Unreleased

- MCP `tools/list` gives every tool a description and a real input schema. List results arrive as `{"items": [...]}` in `structuredContent`; the MCP TypeScript SDK rejected bare arrays.
- `verify_plan` no longer crashes the server. `claim_step` / `release_step` with a step index say `step_not_found`; `record_steps` lists unknown ids in `not_found`. Missing-argument errors name the argument.
- `verify_step` runs on Windows: real Python before the Store alias, `.cmd` shims, no stall on large test logs, no shared temp dir between concurrent runs.
- The 15-file cap keeps the files most relevant to the goal. `.claude` is ignored. Steps no longer depend on themselves.
- `get_plan` and `resume` no longer repeat every step inside `strategy` (about half the bytes). `start_plan` with `summary_only` still saves every step.
- `doctor` no longer prints `schema_version` twice.
- The repo checks are std-only Rust (`cargo run --example ...`), replacing `scripts/*.py`.

See MIGRATION.md for details.

## 4.0.1

Prepare the official MCP Registry listing `io.github.IronAdamant/trammel`. `npx @iron_adamant/trammel mcp` starts the same stdio server as `trammel-mcp`. `server.json` follows schema `2025-12-11` and passes `mcp` as a positional argument. The seven `@iron_adamant/trammel` npm packages and the crate version move to 4.0.1 together. The JSON contract `schema_version` stays `4.0.0`. Not tagged and not published from this change.

## 4.0.0

Rust binary and npm packages `@iron_adamant/trammel` and `@iron_adamant/trammel-<os>-<arch>`. Store is `.trammel/`. MCP surface is 32 tools. `record_step` and `create_plan` return `tool_removed`. `verify_plan` is included. Releases are GitHub Release binaries plus npm. There is no PyPI publish from this tree.

## 3.15.3

### Fixed

- **`trammel-mcp` crash on MCP Python SDK 2.x.** A fresh `pip install trammel[mcp]` resolved `mcp` 2.x. SDK 2.0 removed the low-level `@server.list_tools()` / `@server.call_tool()` decorators, so startup raised `AttributeError: 'Server' object has no attribute 'list_tools'`. The stdio server now uses those decorators on SDK 1.x and `on_list_tools` / `on_call_tool` on SDK 2.x. The extra is `mcp>=1.0.0,<3` (same range stele-context uses). Verified against the latest 2.x resolver result and against mcp 1.30.0.
- **Tool failures looked like successes.** Exceptions inside a tool call were returned as ordinary text content. They now come back as a tool result with `isError: true`.
- **`initialize` reported the mcp library version.** `serverInfo.version` is Trammel's own version.
- **Doc counts.** Current layout, schema, and architecture notes now match the code: 33 MCP tools (12 primary), 15 SQLite tables, 12 `SPEC_REGISTRY` entries (10 spec-backed engine classes, with C and Kotlin aliasing C++ and Java, plus regex TypeScript and JavaScript analyzers).

### Changed

- CI tests Python 3.10–3.13 on Ubuntu and macOS. Python 3.12 jobs also perform a fresh virtualenv install of `trammel[mcp]` and drive `initialize` plus `tools/list` over stdio. Ubuntu 3.12 additionally does the same against mcp 1.30.0. Windows was tried and left out: tests assert POSIX relative paths, and temp-directory cleanup hits `WinError 32` while SQLite still holds the database file.
