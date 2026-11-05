# Trammel technical specification

**Version:** 4.2.0
**Language:** Rust, standard library only. No crates.

The old Python version lives on the `python-legacy` branch.

## 1. Purpose

Trammel plans multi-step coding work for an LLM. It decomposes a goal, tries nine orderings, verifies an edit in a copy of the project, records a failure as a constraint, and stores a successful strategy as a recipe. It does not call an LLM.

Stele retrieves context. Chisel analyses code. Trammel plans. Each one runs alone. They meet only at the MCP client.

## 2. Product

Two binaries from one crate: `trammel` and `trammel-mcp`. npm package `@iron_adamant/trammel` launches the matching `@iron_adamant/trammel-<os>-<arch>` binary. `npx -y @iron_adamant/trammel mcp` starts the MCP server. `trammel-mcp` does the same with no extra argument. Commands on `PATH` stay `trammel` and `trammel-mcp`. The registry manifest is `server.json` (`io.github.IronAdamant/trammel`).

JSON is the default stdout. `--human` prints indented JSON. Exit codes: 0 success, 1 failure, 2 usage.

```text
trammel GOAL | trammel <tool> | trammel doctor | trammel migrate | trammel mcp
```

## 3. Store

The store directory is `.trammel/` under the project root (`TRAMMEL_DB_PATH` or `--store` overrides it). Files: `store` (snapshot), `journal` (appended writes), `lock` (one exclusive writer).

Document keys: `schema_version`, `plans`, `steps`, `constraints`, `recipes`, `failures`, `usage_events`, `trajectories`.

`trammel migrate` reads an old SQLite `trammel.db` and its WAL, read-only, and writes this snapshot. A leftover `trammel.db` with no snapshot returns `legacy_sqlite` until migrate runs. `list_plans` does not accept `db_path`.

## 4. Tools

33 tools. The default MCP listing is the 13 primary tools. `TRAMMEL_MCP_SURFACE=all` lists the rest. Protocols: `2026-07-28` (`server/discover`), `2025-11-25`, `2025-06-18`.

Primary: `start_plan`, `decompose`, `get_plan`, `complete_plan`, `verify_step`, `get_recipe`, `save_recipe`, `status`, `resume`, `export_plan`, `get_constraints`, `add_constraint`.

`record_step` returns `tool_removed` and names `record_steps`. `create_plan` returns `tool_removed` and names `start_plan`.

`verify_plan` walks plan steps that already have file content and calls the same check as `verify_step`.

Schemas live in `schemas/`. Dispatch is `src/tools.rs`, shared by the CLI and MCP.

## 5. Planning

`decompose` and `start_plan` read the project, score files, and order steps. Nine strategies: `bottom_up`, `top_down`, `risk_first`, `critical_path`, `cohesion`, `minimal_change`, `leaf_first`, `hub_first`, `test_adjacent`.

Implicit edges come from `data/patterns.json` (embedded) plus an optional `.trammel/patterns.json` override. Equal confidences keep discovery order. The edge set for `tests/archetypes/test_farm` is frozen in `tests/goldens/test_farm_decompose.json`.

15 languages: Python, TypeScript, JavaScript, Go, Rust, C, C++, Java, Kotlin, C#, Ruby, PHP, Swift, Dart, Zig. Python symbols use the vendored parser. The other languages use hand-written scanners.

## 6. Verification

`verify_step` copies the project to a temp directory, writes the edits, runs a preflight, then the test command. Timeout is 60 seconds. `trace` is capped at 4 KB unless `full_output` is true.

Test command order: argument, then project config, then the language default. Project config is `.trammel.json`. If that file is absent, a `[tool.trammel]` table in the project's `pyproject.toml` is read. This repository does not ship a `pyproject.toml`.

## 7. Constraints

| Type | Effect |
|------|--------|
| `avoid` | Steps for that file are skipped |
| `dependency` | File A is ordered before file B |
| `incompatible` | The files are marked in conflict |
| `requires` | A prerequisite step is added |

## 8. Non-goals

- No network calls from the binary.
- No code generation. The caller supplies edit content.
- No third-party crates. `Cargo.lock` names only `trammel`.
- No PyPI publish from this tree. npm and the GitHub Release are the distribution path.
