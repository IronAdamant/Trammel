# CLAUDE.md

Trammel 4.x is one std-only Rust binary (`trammel`, `trammel-mcp`) plus the npm wrapper `@iron_adamant/trammel`. `npx -y @iron_adamant/trammel mcp` starts the MCP server. The old Python version lives on the `python-legacy` branch.

This file is the source-tree guide. Do not confuse it with a client `CLAUDE.md` that tells an assistant how to call the MCP server.

## Commands

```bash
cargo test --locked
cargo build --release
cargo run --example check_zero_deps
cargo run --example check_no_todos
cargo run --example check_syntax_core
cargo run --release --example bench_gate -- --self-test
cargo run --release --example bench_gate -- target/release/trammel
```

The repo checks are std-only Rust in `examples/` (the Python scripts they replace are on `python-legacy`). `cargo test` runs their unit tests. Run them from the repo root.

`trammel doctor` is JSON by default. `--human` prints indented JSON. `--version` prints `{"version":"<Cargo.toml version>",...}`. To release X.Y.Z, edit only the manifests: Cargo.toml, Cargo.lock, every `npm/*/package.json` (wrapper `optionalDependencies` too), and server.json. `tests/npm_packages.rs` checks they agree with `CARGO_PKG_VERSION`.

`scripts/release.sh X.Y.Z` checks `Cargo.toml` and prints the tag command. It does not tag or publish.

## Layout

- `src/` is the library. `src/main.rs` and `src/bin/trammel_mcp.rs` are the two binaries. Both call `src/cli.rs` and `src/tools.rs`.
- `src/planner.rs`, `src/score.rs`, `src/strategies.rs`, `src/implicit.rs` decompose a goal and run the nine beams. Implicit-edge rules are `data/patterns.json`, embedded with `include_str!`.
- `src/lang/` detects language, extracts symbols, and resolves imports. Python uses the stele-context parser under `src/syntax/`, `src/lang/python/`, and `src/lang/python_symbols/`. `scripts/syntax_core_manifest.txt` is their digest. Other languages use `src/lang/code.rs`, `src/lang/symbols.rs`, and `src/lang/imports.rs`.
- `src/store.rs` is the `.trammel/store` snapshot. `src/migrate.rs` reads an old `trammel.db` and its WAL, read-only.
- `schemas/` has one result schema per tool. A new tool needs a schema file, a row in `src/tools.rs`, and an entry in `src/catalogue.rs` (description and input schema for `tools/list`).
- `tests/archetypes/` and `parity/fixtures/` are sample projects for the Rust tests. They are not a Python package.
- `tests/goldens/test_farm_decompose.json` is the decompose golden. `bench/` holds recorded timings.
- `npm/` is the wrapper and the six platform packages. The release workflow copies binaries in at publish time.

## MCP

The server advertises **33 tools**. Primary surface is 13 (`TRAMMEL_MCP_SURFACE=all` lists the rest). Protocols: `2026-07-28` (`server/discover`), `2025-11-25`, `2025-06-18`, `2025-03-26`, `2024-11-05`; `initialize` echoes any of them. `record_step` and `create_plan` return `tool_removed`. `verify_plan` is included.

## Add a language

1. Teach `src/lang/code.rs` the comment style if it is new.
2. Add a scanner in `src/lang/symbols.rs` and a resolver in `src/lang/imports.rs`.
3. Register the extension in `src/lang/mod.rs` and the name in `src/lang/analyze.rs`.
4. Add a fixture under `parity/fixtures/` and compare `collect_symbols` with the golden before changing one.

## Zero dependencies

`[dependencies]` stays empty. CI fails if `Cargo.lock` names any package other than `trammel`. No build script, no download.

## Release

`.github/workflows/release.yml` runs on a `v*` tag. Its first job fails unless the tag equals the Cargo.toml version and every `npm/*/package.json` and server.json version. It builds six binaries, uploads them with `SHA256SUMS`, then publishes npm. `workflow_dispatch` with a `tag` input republishes an existing release. Do not push a tag from a task branch. Do not create `NPM_TOKEN` in the repo.

## Benchmarks

A PR is slower only when both are true: the 95% median intervals do not overlap in the slow direction, and the Holm-adjusted one-sided Mann-Whitney p is below 0.05. There is no percentage tolerance. `benchstat` tests cover an A/A run (must pass) and an injected slowdown (must fail). Rust medians must also stay under the recorded Python floors in `examples/bench_gate.rs`.

## Docs

`AGENTS.md` is the short module map. `SYSTEM_PROMPT.md` is the tool loop for MCP clients. `docs/LLM_CONTRACT.md` is the JSON contract. `README.md` is the install and benchmark page.
