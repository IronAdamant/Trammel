# Trammel agent guide

Trammel 4.0.0 is one std-only Rust binary (`trammel`, `trammel-mcp`) and the npm package `@iron_adamant/trammel`. The old Python version lives on the `python-legacy` branch. Do not tag or publish from a task branch.

## Commands

```bash
cargo test
cargo build --release
python scripts/bench_gate.py --self-test
python scripts/check_zero_deps.py
python scripts/check_no_todos.py
python scripts/check_syntax_core.py
```

`trammel doctor` is JSON by default. `--human` prints indented JSON. `--version` prints `{"version":"4.0.0",...}`.

## Module map

- `src/syntax/`, `src/lang/python/`, `src/lang/python_symbols/`, `src/lang/json.rs`, `src/lang/kind.rs`: stele-context parser core, commit `244561a491ac4002b2c4bc94a3884091870a6d11`. `scripts/syntax_core_manifest.txt` is their digest. The only local change is a newline index in `python_symbols` so line numbers stay identical without scanning the file prefix per identifier. `check_syntax_core.py --against` a stele checkout reports those two files.
- `src/lang/code.rs`: lossless lexer for the other languages.
- `src/lang/symbols.rs`, `src/lang/imports.rs`: hand-written extractors.
- `src/planner.rs`, `src/score.rs`, `src/strategies.rs`, `src/implicit.rs`: decompose and the nine beams. Rules are `data/patterns.json`.
- `src/store.rs`: `.trammel/store` snapshot. `src/migrate.rs` reads an old `trammel.db` and its WAL, read-only.
- `src/mcp.rs`, `src/cli.rs`, `src/tools.rs`: one dispatch for both surfaces.
- `src/benchstat.rs`: the regression rule.

## Add a language

1. Teach `src/lang/code.rs` the comment style if it is new.
2. Add a scanner in `src/lang/symbols.rs` and a resolver in `src/lang/imports.rs`.
3. Register the extension in `src/lang/mod.rs` and the name in `src/lang/analyze.rs`.
4. Add a fixture and compare `collect_symbols` with the golden before changing one.

## MCP

The Rust server advertises **32 tools**. `record_step` and `create_plan` return `tool_removed`. `verify_plan` is included. The primary surface is 12. Protocols: `2026-07-28` (`server/discover`), `2025-11-25`, `2025-06-18`. New tools need a `schemas/<name>.schema.json` file and a row in `src/tools.rs`.

## Tied implicit edges

Rust sorts implicit edges by confidence, highest first. Equal confidences keep discovery order from sorted maps. `tests/determinism.rs` checks that `tests/archetypes/test_farm` decomposes to the same bytes on every OS (`tests/goldens/test_farm_decompose.json`, timings removed). The comparison script that imported the Python package is on `python-legacy`.

## Distribution

`.github/workflows/release.yml` runs only on a `v*` tag. It builds six binaries (Linux musl x64 and arm64, macOS x64 and arm64, Windows x64 and arm64), uploads them with `SHA256SUMS` to a GitHub Release, and only then publishes npm. The release job does not check out the repo, so every `gh` command passes `--repo "$GITHUB_REPOSITORY"`. The npm job checks out the repo, authenticates `@iron_adamant` with `NODE_AUTH_TOKEN`, publishes `./npm/<pkg>` (a bare `npm/<pkg>` is a GitHub shorthand), the six platform packages first, then the wrapper, and skips a version that is already on the registry. `workflow_dispatch` with a `tag` input runs only that job and downloads the existing release. The npm job reads the repository secret `NPM_TOKEN` (an npm automation token with publish rights). The workflow does not print the token. Do not create the secret's value in the repo, and do not push a tag from a task branch.

The npm wrapper is `@iron_adamant/trammel` (`npm i -g @iron_adamant/trammel`, `npx @iron_adamant/trammel`). The commands on PATH stay `trammel` and `trammel-mcp`. Its optionalDependencies are `@iron_adamant/trammel-<os>-<arch>` for the six platforms. The release job copies each binary in and publishes with `--access public`. The Rust crate stays at zero dependencies.

## Zero dependencies

`[dependencies]` stays empty. CI fails if `Cargo.lock` names any package other than `trammel`. No build script, no download.

## Benchmarks

A PR is slower only when both are true: the 95% median intervals do not overlap in the slow direction, and the Holm-adjusted one-sided Mann-Whitney p is below 0.05. There is no percentage tolerance. `benchstat` tests cover an A/A run (must pass) and an injected slowdown (must fail). Rust medians must also stay under the recorded Python floors in `scripts/bench_gate.py`.
