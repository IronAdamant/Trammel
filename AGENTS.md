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

- `src/syntax/`, `src/lang/python/`, `src/lang/python_symbols/`, `src/lang/json.rs`, `src/lang/kind.rs` — stele-context parser core, commit `244561a491ac4002b2c4bc94a3884091870a6d11`. `scripts/syntax_core_manifest.txt` is their digest. The only local change is a newline index in `python_symbols` so line numbers stay identical without scanning the file prefix per identifier. `check_syntax_core.py --against` a stele checkout reports those two files.
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

The Rust server advertises **32 tools**. That is the Python 33 minus `record_step` and `create_plan`, plus `verify_plan`. The primary surface is still the same 12. Protocols: `2026-07-28` (`server/discover`), `2025-11-25`, `2025-06-18`. Removed names return `tool_removed`. New tools need a `schemas/<name>.schema.json` file and a row in `src/tools.rs`.

## Tied implicit edges

Rust sorts implicit edges by confidence, highest first. Equal confidences keep discovery order from sorted maps. Python uses set iteration, so its order changes with `PYTHONHASHSEED` and the OS. The edge set matches. `tests/determinism.rs` checks that `tests/archetypes/test_farm` decomposes to the same bytes on every OS (`tests/goldens/test_farm_decompose.json`, timings removed). `scripts/parity_check.py` checks that test_farm matches Python on files, descriptions, symbols, relevance, rationale, and fingerprint, and that a second Rust run repeats the edge list.

## Distribution

`.github/workflows/release.yml` runs only on a `v*` tag. It builds six binaries (Linux musl x64 and arm64, macOS x64 and arm64, Windows x64 and arm64), uploads them with `SHA256SUMS` to a GitHub Release, and only then publishes npm. The npm job reads the repository secret `NPM_TOKEN` (an npm automation token with publish rights). The workflow does not print the token. Do not create the secret's value in the repo, and do not push a tag from this branch.

The npm wrapper is `@iron_adamant/trammel` (`npm i -g @iron_adamant/trammel`, `npx @iron_adamant/trammel`). The commands on PATH stay `trammel` and `trammel-mcp`. Its optionalDependencies are `@iron_adamant/trammel-<os>-<arch>` for the six platforms. The release job copies each binary in and publishes with `--access public`. The Rust crate stays at zero dependencies.

## Zero dependencies

`[dependencies]` stays empty. CI fails if `Cargo.lock` names any package other than `trammel`. No build script, no download.

## Benchmarks

A PR is slower only when both are true: the 95% median intervals do not overlap in the slow direction, and the Holm-adjusted one-sided Mann-Whitney p is below 0.05. There is no percentage tolerance. `benchstat` tests cover an A/A run (must pass) and an injected slowdown (must fail). Rust medians must also stay under the recorded Python floors in `scripts/bench_gate.py`.
