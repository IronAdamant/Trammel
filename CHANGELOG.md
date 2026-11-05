# Changelog

The old Python version lives on the `python-legacy` branch. Notes below 4.0.0 describe that package. Older Python releases are also in [docs/PYTHON_CHANGELOG.md](docs/PYTHON_CHANGELOG.md) and `LLM_Development.md`.

## 4.3.0 - 2026-11-05

From the live test on Specula, a 27-crate Rust workspace (goal: "add a per-prop blocks_build flag to Sepulcra props: ..."):

- Plans follow the code the goal names. A directory whose name, or whose Cargo.toml / package.json package name, is a goal word is an anchor (`analysis_meta.anchors`). Files inside it get 30% of their relevance from being there, and import count drops to 10%, so shared helpers no longer outrank the named crate. The language is detected at the anchor. On Specula, 15 of 15 steps are now in `games/sepulcra` (was 3 of 15, led by `specula-core` `dmath.rs`, `hash.rs` and `rng.rs`).
- Keywords: parts of identifiers such as `blocks_build` stay keywords even when they are verbs ("build"); more English filler words ("their", "others", "per", ...) are dropped; partial matches need a shared prefix of four letters or more ("per" no longer matches "upper").
- Suggested new files use the extension of the files around them (`.rs` in a Rust crate), not always `.py` (B9).
- `start_plan`, `decompose`, `get_plan`, `resume` and `get_recipe` reply compactly by default: steps with file, description, dependencies and relevance; plan status and notes; the recipe's files. `full: true` returns every field as before. On Specula, `start_plan` went from 39,434 to 8,566 bytes and `get_plan` from 39,456 to 8,866. Stored plans and recipes are unchanged.
- Release: the repository variable `NPM_PUBLISH=false` makes a tag push create the GitHub Release without publishing npm; running the Release workflow by hand with the tag publishes it later.

## 4.2.0 - 2026-11-05

- Tool groups (the suite-wide convention): every MCP tool is in one group: `core` (always on), `plan`, `verify`, `recipes`, `constraints`, `locks` (`claim_step`, `release_step`, `available_steps`), `notes` (`add_note`), `maintenance`. `TRAMMEL_TOOLS` exposes only the named groups or tools (plus `core`); `TRAMMEL_DISABLE` hides groups or tools. Both combine with `TRAMMEL_MCP_SURFACE`. An unknown name, or `core` in `TRAMMEL_DISABLE`, stops the server with one stderr line and exit code 2. A call to a hidden tool returns `tool "claim_step" is disabled (group "locks") by TRAMMEL_DISABLE; remove "locks" from it to enable`. When a switch is set, the `initialize` instructions end with one line naming the groups on and off, and `status` adds `tool_groups`. With neither set, `initialize` and `tools/list` match 4.1.0 except `tools.listChanged`; `tests/goldens/mcp_default_*.jsonl` (recorded from a 4.1.0 build, that field updated) enforce it. The CLI is unchanged.
- Agents choose the groups: an `agent-tools` block in the project's AGENTS.md (`trammel: tools ...` / `trammel: disable ...`). Env vars override it per key. Mistakes in the file are warnings in the `initialize` instructions and `status` (`tool_group_warnings`), never fatal. The server stats the file on every request; when an edit changes the tool list it sends `notifications/tools/list_changed` before the reply, and `initialize` now advertises `capabilities.tools.listChanged: true` (4.1.0 said false), so a client that connected before a block existed still refreshes. Apart from that field, with no block and no env switch `initialize` and `tools/list` are byte-identical to 4.1.0, with or without an AGENTS.md.

## 4.1.0 - 2026-11-05

- Version 4.1.0 in every manifest. Tests read `CARGO_PKG_VERSION`, so a bump only edits Cargo.toml, Cargo.lock, `npm/*/package.json` and server.json. `install.sh` / `install.ps1` fetch the latest release unless `TRAMMEL_VERSION` pins one.
- The release workflow refuses a tag that does not match those manifests, and a `workflow_dispatch` republish uses the tagged commit's npm package files.

- MCP `tools/list` gives every tool a description and a real input schema. List results arrive as `{"items": [...]}` in `structuredContent`; the MCP TypeScript SDK rejected bare arrays.
- `verify_plan` no longer crashes the server. `claim_step` / `release_step` with a step index say `step_not_found`; `record_steps` lists unknown ids in `not_found`. Missing-argument errors name the argument.
- `verify_step` runs on Windows: real Python before the Store alias, `.cmd` shims, no stall on large test logs, no shared temp dir between concurrent runs.
- The 15-file cap keeps the files most relevant to the goal. `.claude` is ignored. Steps no longer depend on themselves.
- `get_plan` and `resume` no longer repeat every step inside `strategy` (about half the bytes). `start_plan` with `summary_only` still saves every step.
- `doctor` no longer prints `schema_version` twice.
- `initialize` echoes `2025-03-26` and `2024-11-05` as well, so older SDK clients connect.
- `verify_step` preflight is language-aware (no false alarms on 1,242 real files) and a failed preflight is `success: false`, without running tests. Large non-Python files preflight in linear time.
- `available_steps` only offers steps whose dependencies are passed or skipped, and `claim_step` refuses a blocked step with `step_blocked`, naming the blocking step ids.
- New primary tool `add_note`: free-text handoff notes on a plan, returned by `get_plan` and `resume`.
- `prune_recipes`, `prune_plans` and `validate_recipes` do what they say (3.x rules) instead of returning `removed: 0`. `history` returns trajectories; the goal command records one per beam and closes its plan.
- After `trammel migrate`, new rows get ids after the migrated ones instead of reusing 1, 2, ... The migrate tests read a committed 3.15.3 database, so the test suite needs no Python.
- `SYSTEM_PROMPT.md` is rewritten for the 4.x tools: the loop, handoff with notes, several agents, conventions, and a tool reference generated from `tools/list`.
- The goal command reads `test_cmd` from `.trammel.json` and detects the language, like `verify_step`; it ran Python's `unittest` on a Go project. `verify_step` with a failed preflight answers in under 1 ms (it resolved the test command first, about 65 ms on Windows).
- `list_plans` returns seven-field summaries, newest first, not every stored column. Python-era wording is gone from errors: the `strict_greenfield` rejection and the repo checks (no tracebacks; see MIGRATION.md, Repo checks).
- `get_recipe` matches paraphrases (stemming, stopwords, relation words, the recipe's files and symbols as half-weight context).
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
