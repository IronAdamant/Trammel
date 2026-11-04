# CLAUDE.md

Trammel 4.0.0 is one std-only Rust binary (`trammel`, `trammel-mcp`) plus the npm wrapper `@iron_adamant/trammel`. The old Python version lives on the `python-legacy` branch.

This file is the source-tree guide. Do not confuse it with a client `CLAUDE.md` that tells an assistant how to call the MCP server.

## Commands

```bash
cargo test --locked
cargo build --release
python scripts/check_zero_deps.py
python scripts/check_no_todos.py
python scripts/check_syntax_core.py
python scripts/bench_gate.py --self-test
python scripts/bench_gate.py target/release/trammel
```

`trammel doctor` is JSON by default. `--human` prints indented JSON. `--version` prints `{"version":"4.0.0",...}`.

`scripts/release.sh X.Y.Z` checks `Cargo.toml` and prints the tag command. It does not tag or publish.

## Layout

- `src/` is the library. `src/main.rs` and `src/bin/trammel_mcp.rs` are the two binaries. Both call `src/cli.rs` and `src/tools.rs`.
- `src/planner.rs`, `src/score.rs`, `src/strategies.rs`, `src/implicit.rs` decompose a goal and run the nine beams. Implicit-edge rules are `data/patterns.json`, embedded with `include_str!`.
- `src/lang/` detects language, extracts symbols, and resolves imports. Python uses the stele-context parser under `src/syntax/`, `src/lang/python/`, and `src/lang/python_symbols/`. `scripts/syntax_core_manifest.txt` is their digest. Other languages use `src/lang/code.rs`, `src/lang/symbols.rs`, and `src/lang/imports.rs`.
- `src/store.rs` is the `.trammel/store` snapshot. `src/migrate.rs` reads an old `trammel.db` and its WAL, read-only.
- `schemas/` has one JSON schema per tool. A new tool needs a schema file and a row in `src/tools.rs`.
- `tests/archetypes/` and `parity/fixtures/` are sample projects for the Rust tests. They are not a Python package.
- `tests/goldens/test_farm_decompose.json` is the decompose golden. `bench/` holds recorded timings.
- `npm/` is the wrapper and the six platform packages. The release workflow copies binaries in at publish time.

## MCP

The server advertises **32 tools**. Primary surface is 12 (`TRAMMEL_MCP_SURFACE=all` lists the rest). Protocols: `2026-07-28` (`server/discover`), `2025-11-25`, `2025-06-18`. `record_step` and `create_plan` return `tool_removed`. `verify_plan` is included.

## Add a language

1. Teach `src/lang/code.rs` the comment style if it is new.
2. Add a scanner in `src/lang/symbols.rs` and a resolver in `src/lang/imports.rs`.
3. Register the extension in `src/lang/mod.rs` and the name in `src/lang/analyze.rs`.
4. Add a fixture under `parity/fixtures/` and compare `collect_symbols` with the golden before changing one.

## Zero dependencies

`[dependencies]` stays empty. CI fails if `Cargo.lock` names any package other than `trammel`. No build script, no download.

## Release

`.github/workflows/release.yml` runs on a `v*` tag. It builds six binaries, uploads them with `SHA256SUMS`, then publishes npm. `workflow_dispatch` with a `tag` input republishes an existing release. Do not push a tag from a task branch. Do not create `NPM_TOKEN` in the repo.

## Benchmarks

A PR is slower only when both are true: the 95% median intervals do not overlap in the slow direction, and the Holm-adjusted one-sided Mann-Whitney p is below 0.05. There is no percentage tolerance. `benchstat` tests cover an A/A run (must pass) and an injected slowdown (must fail). Rust medians must also stay under the recorded Python floors in `scripts/bench_gate.py`.

## Docs

`AGENTS.md` is the short module map. `SYSTEM_PROMPT.md` is the tool loop for MCP clients. `docs/LLM_CONTRACT.md` is the JSON contract. `README.md` is the install and benchmark page.
