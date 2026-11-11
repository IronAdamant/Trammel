# Changelog

The old Python version lives on the `python-legacy` branch. Notes below 4.0.0 describe that package. Older Python releases are also in [docs/PYTHON_CHANGELOG.md](docs/PYTHON_CHANGELOG.md) and `LLM_Development.md`.

## 4.4.0 - 2026-11-11

From Specula's M5 lanes, where Trammel was "rarely useful": plans ignored the files the goal named, test steps ranked with the code, and `verify_step` stopped after 60 s, shorter than one cargo build.

- **Plans start from the files the goal names.** Paths (`crates/specula/src/sight.rs`), bare file names (`game.rs`), Rust module paths (`specula::sight`, `crate::game`), module names in backticks or before "module" / "file", and the new `files` argument are matched to project files. The plan is those files (`role: named`, relevance 1.0) plus a capped set of direct neighbours from real imports: dependencies and dependents, alternating (two per named file, 3 to 8), then 2 to 4 tests that import them or are named after them. Neighbours rank 0.2 to 0.6, tests 0.1 to 0.4. `analysis_meta` reports `named_files`, `unresolved_files` and `ambiguous_files` (with candidates), and a name that fits several files is settled by the ones already matched (same crate first). The language follows the named files' extension. On Specula, the timeline goal planned 15 mostly unrelated files (`specula-core/src/math.rs`, `specula-studio/src/rules.rs`, `specula-anim/src/play.rs`, ...), and with bare names 15 test files across the repo; it now plans `sight.rs` and `game.rs` in dependency order with `world.rs`, `lib.rs`, `sight_overlay.rs`, `write_policy.rs` and two tests.
- **Code first, then tests.** Every step has a `role` (`named`, `dependency`, `dependent`, `core`, `test`, `named_test`), kept in the saved plan and in compact replies. Test files come after all code steps; without named files their relevance is halved unless the goal is about tests, also when the 15-file cap picks files. The `test_farm` golden changed only by the `role` field.
- **Rust imports resolve in workspaces.** `use crate::x` was looked up at the repo root, so a Cargo workspace had almost no edges, and `pub mod x;` and grouped imports were missed. The new resolver (`src/lang/rust_mods.rs`) handles `crate::`, `self::`, `super::`, other workspace packages, child modules, inline paths and `mod x;` with any visibility or `#[path]`.
- **Keywords and anchors from paths.** A named path counts as its file stem and non-generic directories ("specula sight", not "crates src"). `src`, `tests`, `crates` and similar words never anchor, a file stem never anchors (`game.rs` is not about `games/`), and a directory without source files is no anchor (Specula's `assets/timelines`).
- **`verify_step` time limit.** `timeout_s` per call, `verify_timeout_s` in `.trammel.json` or `[tool.trammel]`, default 300 s (was a fixed 60 s), capped at 600 s for a call that waits; a larger value is capped with a `timeout_note`. A run past the limit answers `timed_out: true` with `elapsed_s`, `timeout_s`, the output printed so far and a hint naming every way to raise it. Every result carries `elapsed_s`, `timeout_s` and `copy` (`reused`, `files_written`).
- **Background verify.** `verify_step` with `background: true` returns a `job_id` at once and may run up to 3600 s. New tools `job_status` (`wait_s` up to 600 waits for the end; `running`, `completed`, `cancelled`, `not_found`) and `cancel_job` (kills the test command), in the `verify` group and on the primary surface (now 15 of 35 tools), following Chisel's `start_job` / `job_status` shape. At most four jobs run at once. The CLI answers `background_unavailable`. `verify_plan` stops after 600 s in one call with `timed_out`.
- **The verify copy and its build cache are kept.** The copy lives at a stable path per project under `<temp>/trammel-verify-cache/` (four slots for concurrent runs) and is brought up to date by rewriting only files whose bytes differ, so unchanged files keep their modification times. The test command gets `CARGO_TARGET_DIR` set to the slot's own target directory, so cargo rebuilds only what changed. `TRAMMEL_VERIFY_CACHE=0` restores a fresh copy per run, or names another directory. A project's cache unused for 14 days is deleted. Edit paths that leave the project are refused. On Specula, `cargo test -p specula-tempus` took 48.6 s on every call in 4.3.1; now the first call takes 40.7 s and the next 5.9 s. `cargo test -p specula --lib timeline` was killed at 60 s in 4.3.1; now a background job built it cold in 963 s (polled with `job_status`, 240 s waits), and after an edit to `timeline.rs` a waiting call rebuilt only the `specula` crate and passed in 71 s.
- An edit whose `content` is not a string, or that has no path, is refused with `bad_edit` instead of being written as an empty file.
- Tool descriptions and the server instructions say when a plan helps (several files in dependency order, handoffs between agents) and when to skip it (a one- or two-file edit).

## 4.3.1 - 2026-11-05

- `verify_step` copies only what git would commit: tracked files plus untracked files no .gitignore excludes (`git ls-files --cached --others --exclude-standard`). Outside a git work tree, or without git, it walks the tree as before, skipping generated directories. On Specula each verify copied 4,645 files (469.5 MB, including the ignored 184 MB `gallery-full/`); it now copies 1,626 files (36.0 MB). Median of six runs: copy-only verify 4.4 s to 1.7 s, `cargo test -p sepulcra --lib` verify 11.0 s to 7.6 s.
- Suggested new files only when the goal asks for one: a creation verb with a file-like noun ("create a props module", "add a new settings page", "add a module called retry_policy") or a path in the goal that does not exist. At most three, in the folder of the most relevant step. Otherwise `creation_hints.suggested_files` is empty and `suggested_edits` lists the existing files to change. The Specula goal no longer suggests `blocks.rs`, `flag.rs`, `marked.rs` and the like.

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
