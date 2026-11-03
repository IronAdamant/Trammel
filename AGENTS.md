# Trammel — agent guide

Trammel 4.0 is one std-only Rust binary (`trammel`, `trammel-mcp`). Python 3.15.3 stays in this branch so parity tests can import it. Do not tag, merge, or publish from a rewrite branch.

## Commands

```bash
cargo test
cargo build --release
python scripts/parity_check.py target/release/trammel
python scripts/bench_gate.py --self-test
python scripts/check_zero_deps.py
python scripts/check_no_todos.py
python scripts/check_syntax_core.py
```

`trammel doctor` is JSON by default. `--human` prints indented JSON. `--version` prints `{"version":"4.0.0",...}`.

## Module map

- `src/syntax/`, `src/lang/python/`, `src/lang/python_symbols/`, `src/lang/json.rs`, `src/lang/kind.rs` — stele-context parser core, commit `244561a491ac4002b2c4bc94a3884091870a6d11`. Do not edit these. `scripts/syntax_core_manifest.txt` is their digest.
- `src/lang/code.rs` — lossless lexer for the other languages.
- `src/lang/symbols.rs`, `src/lang/imports.rs` — hand-written extractors.
- `src/planner.rs`, `src/score.rs`, `src/strategies.rs`, `src/implicit.rs` — decompose and the nine beams.
- `src/store.rs` — `.trammel/store` snapshot. `src/migrate.rs` reads an old `trammel.db` and its WAL, read-only.
- `src/mcp.rs`, `src/cli.rs`, `src/tools.rs` — one dispatch for both surfaces.
- `src/benchstat.rs` — the regression rule.

## Add a language

1. Teach `src/lang/code.rs` the comment style if it is new.
2. Add a scanner in `src/lang/symbols.rs` and a resolver in `src/lang/imports.rs`.
3. Register the extension in `src/lang/mod.rs` and the name in `src/lang/analyze.rs`.
4. Add a fixture and compare `collect_symbols` with Python before changing a golden.

## MCP

Protocols: `2026-07-28` (`server/discover`), `2025-11-25`, `2025-06-18`. Keep the current revision and the two previous. `record_step` and `create_plan` return `tool_removed`. New tools need a `schemas/<name>.schema.json` file and a row in `src/tools.rs`.

## Zero dependencies

`[dependencies]` stays empty. CI fails if `Cargo.lock` names any package other than `trammel`. No build script, no download.

## Benchmarks

A PR is slower only when both are true: the 95% median intervals do not overlap in the slow direction, and the Holm-adjusted one-sided Mann-Whitney p is below 0.05. There is no percentage tolerance. `benchstat` tests cover an A/A run (must pass) and an injected slowdown (must fail). Rust medians must also stay under the recorded Python floors in `scripts/bench_gate.py`.
