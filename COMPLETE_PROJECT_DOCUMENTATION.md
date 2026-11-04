# Trammel: project documentation index

**Updated:** 2026-11-04
**Version:** 4.0.1 (Rust binary and npm). The old Python version lives on the `python-legacy` branch.
**Purpose:** Dependency-aware planning, isolated verification, and recipe memory for coding agents. One std-only crate. Two binaries. 32 MCP tools, 12 primary. Store is `.trammel/`.

## Removed from main

These stay on `python-legacy` at `dc7deb335e91e5a29541733a234dfc1ed24788a3`.

| Path | Why it left main |
|------|------------------|
| `trammel/` | Python package. `data/patterns.json` moved to `data/` first, because the Rust binary embeds it. |
| `tests/test_*.py`, `tests/__init__.py` | Python unittest suite. Archetype fixtures and Rust tests stayed. npm checks moved to `tests/npm_packages.rs`. |
| `pyproject.toml` | Python package metadata. User projects may still have their own. The binary reads `[tool.trammel]` from those. |
| `.github/workflows/ci.yml` | Python 3.10-3.13 unittest matrix. |
| `.github/workflows/publish.yml` | PyPI trusted publishing. This tree publishes npm from `release.yml`. |
| `scripts/parity_check.py` | Imported the Python package to compare decompose output. The golden file stayed. |
| `scripts/same_runner_bench.py` | Re-ran the Python package for timings. `bench/same_runner.json` stayed. |
| `docs/wiki/export.py.wiki.md` | Notes for the deleted `trammel/export.py`. Export now lives in `src/export.rs`. |

## Kept on purpose

| Path | Why |
|------|-----|
| `src/`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` | The product. |
| `tests/*.rs`, `tests/archetypes/`, `tests/goldens/` | Rust tests and the decompose golden. |
| `parity/fixtures/`, `parity/goldens/` | Parser tests read these files. |
| `schemas/`, `server.json`, `npm/`, `install.sh`, `install.ps1` | MCP schemas, registry id, npm layout, release installers. |
| `.github/workflows/release.yml` | Six binaries, GitHub Release, npm. `workflow_dispatch` republishes an existing tag. |
| `.github/workflows/rust.yml` | Linux, macOS, and Windows `cargo test`, plus the bench rule. The Python parity job was removed. |
| `scripts/check_*.py`, `scripts/bench_gate.py`, `scripts/syntax_core_manifest.txt`, `scripts/release.sh` | Rust CI gates. They do not import a Python package. |
| `bench/` | Recorded timings. |
| `data/patterns.json` | Compile-time input for implicit edges. |

## Files

| Path | Purpose |
|------|---------|
| `.gitattributes` | LF for source text |
| `.github/workflows/release.yml` | Six release binaries, GitHub Release, npm publish |
| `.github/workflows/rust.yml` | cargo test, zero deps, no markers, syntax core, bench rule |
| `.gitignore` | Build output, store, local databases, editor files |
| `AGENTS.md` | Module map, commands, release rules for agents |
| `CHANGELOG.md` | 4.0.1 note, then 4.0.0, plus the Python 3.15.3 notes |
| `CLAUDE.md` | Source-tree guide for coding agents |
| `Cargo.lock` | Locks the crate to itself. No other packages. |
| `Cargo.toml` | Crate metadata, version 4.0.1, empty dependencies |
| `GROK_CONNECTORS.md` | How a Grok client starts trammel mcp |
| `LICENSE` | MIT licence |
| `LLM_Development.md` | Chronological log, Python history included |
| `README.md` | Install, benchmarks, and the Python release notes |
| `SYSTEM_PROMPT.md` | MCP and CLI tool loop for LLM clients |
| `bench/python_baseline.json` | Recorded Python medians used as CI floors |
| `bench/rust_measured.json` | Last Rust medians written by bench_gate.py |
| `bench/same_runner.json` | Same-machine Python 3.15.3 vs Rust medians |
| `data/patterns.json` | Implicit-edge rules embedded by src/implicit.rs |
| `docs/LLM_CONTRACT.md` | JSON contract 4.0.0 |
| `docs/demo-plan-explore.png` | README screenshot |
| `findings/kimi_review_1/trammel.md_closed` | Historical review note |
| `findings/tmm_kimi_review_3/trammel_closed.md` | Historical review note |
| `install.ps1` | Download a published release on Windows |
| `install.sh` | Download a published release into ~/.local/bin |
| `llms.txt` | Short command card |
| `npm/trammel-darwin-arm64/package.json` | Platform package manifest. The release job copies the binary in. |
| `npm/trammel-darwin-x64/package.json` | Platform package manifest. The release job copies the binary in. |
| `npm/trammel-linux-arm64/package.json` | Platform package manifest. The release job copies the binary in. |
| `npm/trammel-linux-x64/package.json` | Platform package manifest. The release job copies the binary in. |
| `npm/trammel-windows-arm64/package.json` | Platform package manifest. The release job copies the binary in. |
| `npm/trammel-windows-x64/package.json` | Platform package manifest. The release job copies the binary in. |
| `npm/trammel/bin/trammel-mcp.js` | Spawn the platform trammel-mcp binary |
| `npm/trammel/bin/trammel.js` | Spawn the platform trammel binary |
| `npm/trammel/package.json` | Wrapper @iron_adamant/trammel |
| `parity/fixtures/c/broken.c` | Small sample parsed by the Rust language tests |
| `parity/fixtures/c/util.c` | Small sample parsed by the Rust language tests |
| `parity/fixtures/c/util.cc` | Small sample parsed by the Rust language tests |
| `parity/fixtures/c/util.cpp` | Small sample parsed by the Rust language tests |
| `parity/fixtures/c/util.cxx` | Small sample parsed by the Rust language tests |
| `parity/fixtures/c/util.h` | Small sample parsed by the Rust language tests |
| `parity/fixtures/c/util.hpp` | Small sample parsed by the Rust language tests |
| `parity/fixtures/c/util.hxx` | Small sample parsed by the Rust language tests |
| `parity/fixtures/data/broken.json` | Small sample parsed by the Rust language tests |
| `parity/fixtures/data/config.cfg` | Small sample parsed by the Rust language tests |
| `parity/fixtures/data/config.ini` | Small sample parsed by the Rust language tests |
| `parity/fixtures/data/config.json` | Small sample parsed by the Rust language tests |
| `parity/fixtures/data/config.toml` | Small sample parsed by the Rust language tests |
| `parity/fixtures/data/config.yaml` | Small sample parsed by the Rust language tests |
| `parity/fixtures/data/config.yml` | Small sample parsed by the Rust language tests |
| `parity/fixtures/data/query.sql` | Small sample parsed by the Rust language tests |
| `parity/fixtures/go/broken.go` | Small sample parsed by the Rust language tests |
| `parity/fixtures/go/main.go` | Small sample parsed by the Rust language tests |
| `parity/fixtures/javascript/broken.js` | Small sample parsed by the Rust language tests |
| `parity/fixtures/javascript/component.tsx` | Small sample parsed by the Rust language tests |
| `parity/fixtures/javascript/legacy.cjs` | Small sample parsed by the Rust language tests |
| `parity/fixtures/javascript/mod.mjs` | Small sample parsed by the Rust language tests |
| `parity/fixtures/javascript/sample.js` | Small sample parsed by the Rust language tests |
| `parity/fixtures/javascript/types.ts` | Small sample parsed by the Rust language tests |
| `parity/fixtures/javascript/view.jsx` | Small sample parsed by the Rust language tests |
| `parity/fixtures/jvm/App.cs` | Small sample parsed by the Rust language tests |
| `parity/fixtures/jvm/App.java` | Small sample parsed by the Rust language tests |
| `parity/fixtures/jvm/App.kt` | Small sample parsed by the Rust language tests |
| `parity/fixtures/jvm/App.scala` | Small sample parsed by the Rust language tests |
| `parity/fixtures/jvm/broken.java` | Small sample parsed by the Rust language tests |
| `parity/fixtures/jvm/build.kts` | Small sample parsed by the Rust language tests |
| `parity/fixtures/php/app.php` | Small sample parsed by the Rust language tests |
| `parity/fixtures/php/broken.php` | Small sample parsed by the Rust language tests |
| `parity/fixtures/python/broken.py` | Small sample parsed by the Rust language tests |
| `parity/fixtures/python/ext.pyx` | Small sample parsed by the Rust language tests |
| `parity/fixtures/python/sample.py` | Small sample parsed by the Rust language tests |
| `parity/fixtures/python/script.pyw` | Small sample parsed by the Rust language tests |
| `parity/fixtures/python/unicode.py` | Small sample parsed by the Rust language tests |
| `parity/fixtures/ruby/app.rb` | Small sample parsed by the Rust language tests |
| `parity/fixtures/ruby/broken.rb` | Small sample parsed by the Rust language tests |
| `parity/fixtures/rust/broken.rs` | Small sample parsed by the Rust language tests |
| `parity/fixtures/rust/lib.rs` | Small sample parsed by the Rust language tests |
| `parity/fixtures/shell/broken.sh` | Small sample parsed by the Rust language tests |
| `parity/fixtures/shell/run.bash` | Small sample parsed by the Rust language tests |
| `parity/fixtures/shell/run.sh` | Small sample parsed by the Rust language tests |
| `parity/fixtures/shell/run.zsh` | Small sample parsed by the Rust language tests |
| `parity/fixtures/swift/App.swift` | Small sample parsed by the Rust language tests |
| `parity/fixtures/swift/broken.swift` | Small sample parsed by the Rust language tests |
| `parity/fixtures/text/notes.adoc` | Small sample parsed by the Rust language tests |
| `parity/fixtures/text/notes.markdown` | Small sample parsed by the Rust language tests |
| `parity/fixtures/text/notes.rst` | Small sample parsed by the Rust language tests |
| `parity/fixtures/text/notes.txt` | Small sample parsed by the Rust language tests |
| `parity/fixtures/text/rows.csv` | Small sample parsed by the Rust language tests |
| `parity/fixtures/text/rows.tsv` | Small sample parsed by the Rust language tests |
| `parity/fixtures/text/run.log` | Small sample parsed by the Rust language tests |
| `parity/fixtures/text/tiebreak.txt` | Small sample parsed by the Rust language tests |
| `parity/fixtures/web/broken.html` | Small sample parsed by the Rust language tests |
| `parity/fixtures/web/page.htm` | Small sample parsed by the Rust language tests |
| `parity/fixtures/web/site.css` | Small sample parsed by the Rust language tests |
| `parity/fixtures/web/site.less` | Small sample parsed by the Rust language tests |
| `parity/fixtures/web/site.scss` | Small sample parsed by the Rust language tests |
| `parity/goldens/corpora.json` | Historical parser corpus summary |
| `parity/goldens/extensions.json` | Historical extension summary |
| `parity/goldens/fixtures.json` | Parser fixture golden used by Rust tests |
| `rust-toolchain.toml` | Pins rustc 1.99.1 |
| `schemas/add_constraint.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/available_steps.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/claim_step.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/complete_plan.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/deactivate_constraint.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/decompose.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/envelope.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/estimate.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/explore.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/export_plan.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/failure_history.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/get_constraints.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/get_plan.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/get_recipe.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/history.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/list_plans.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/list_recipes.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/list_strategies.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/merge_plans.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/prune_plans.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/prune_recipes.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/record_steps.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/release_step.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/resolve_failure.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/resume.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/save_recipe.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/start_plan.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/status.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/update_plan_status.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/usage_stats.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/validate_recipes.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/verify_plan.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `schemas/verify_step.schema.json` | JSON schema for one MCP tool or the shared envelope |
| `scripts/bench_gate.py` | Time the Rust binary against recorded Python floors |
| `scripts/check_no_todos.py` | Fail on TODO or FIXME in src |
| `scripts/check_syntax_core.py` | Check parser files against the manifest |
| `scripts/check_zero_deps.py` | Fail if Cargo.lock names anything but trammel |
| `scripts/release.sh` | Print the tag command after checking Cargo.toml. Does not publish. |
| `scripts/syntax_core_manifest.txt` | SHA-256 of the vendored parser files |
| `server.json` | MCP Registry manifest, schema 2025-12-11, name `io.github.IronAdamant/trammel` |
| `src/benchstat.rs` | Median-interval regression rule |
| `src/bin/trammel_mcp.rs` | trammel-mcp binary |
| `src/canon.rs` | Stable JSON helpers |
| `src/cli.rs` | Argv parser shared by both binaries |
| `src/config.rs` | .trammel.json and [tool.trammel] in a project pyproject.toml |
| `src/contract.rs` | Success and failure envelope |
| `src/doctor.rs` | Local health report, no network |
| `src/export.rs` | trammel.plan JSON |
| `src/goal.rs` | Goal text fingerprints and hints |
| `src/harness.rs` | Isolated verify: copy, preflight, test command |
| `src/hash.rs` | SHA-256 |
| `src/implicit.rs` | Naming, shared-state, and co-occurrence edges |
| `src/json.rs` | Std-only JSON parser |
| `src/lang/analyze.rs` | Language detection and project analysis |
| `src/lang/blank.rs` | Blank and comment spans |
| `src/lang/code.rs` | Lossless lexer for non-Python languages |
| `src/lang/imports.rs` | Hand-written import resolvers |
| `src/lang/json.rs` | JSON language helpers |
| `src/lang/kind.rs` | Token kinds |
| `src/lang/mod.rs` | Language module and extension map |
| `src/lang/python/lex.rs` | Python lexer |
| `src/lang/python/mod.rs` | Python parser facade |
| `src/lang/python_symbols/expr.rs` | Python expression walk |
| `src/lang/python_symbols/mod.rs` | Python symbol extraction |
| `src/lang/symbols.rs` | Hand-written symbol scanners |
| `src/lib.rs` | Crate root and version constants |
| `src/main.rs` | trammel binary |
| `src/mcp.rs` | Stdio MCP server |
| `src/migrate.rs` | Read-only import of trammel.db |
| `src/planner.rs` | Decompose, explore, plan_and_execute |
| `src/root.rs` | Project root and store path |
| `src/score.rs` | Step relevance |
| `src/similarity.rs` | Recipe text similarity |
| `src/store.rs` | .trammel snapshot, journal, and lock |
| `src/strategies.rs` | Nine beam orderings |
| `src/syntax/STELE_REV` | See the neighbouring files |
| `src/syntax/cst.rs` | Concrete syntax tree |
| `src/syntax/edit.rs` | Syntax edits |
| `src/syntax/incremental.rs` | Incremental reparse |
| `src/syntax/mod.rs` | Syntax core facade |
| `src/syntax/query.rs` | Syntax queries |
| `src/tools.rs` | One dispatch for CLI tools and MCP tools |
| `src/topo.rs` | Topological sort |
| `src/walk.rs` | File walk and ignore rules |
| `tests/archetypes/facade_service/pyproject.toml` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/facade_service/src/coordinator.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/facade_service/src/workers/worker_1.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/facade_service/src/workers/worker_10.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/facade_service/src/workers/worker_11.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/facade_service/src/workers/worker_12.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/facade_service/src/workers/worker_2.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/facade_service/src/workers/worker_3.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/facade_service/src/workers/worker_4.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/facade_service/src/workers/worker_5.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/facade_service/src/workers/worker_6.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/facade_service/src/workers/worker_7.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/facade_service/src/workers/worker_8.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/facade_service/src/workers/worker_9.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/generated_code/pyproject.toml` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/generated_code/src/generated/generated_1.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/generated_code/src/generated/generated_2.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/generated_code/src/generated/generated_3.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/generated_code/src/generated/generated_4.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/generated_code/src/generated/generated_5.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/generated_code/src/handmade/cli.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/generated_code/src/handmade/core.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/legacy_ball_of_mud/pyproject.toml` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/legacy_ball_of_mud/src/a.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/legacy_ball_of_mud/src/b.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/legacy_ball_of_mud/src/c.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/legacy_ball_of_mud/src/deep/__init__.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/legacy_ball_of_mud/src/deep/nested/__init__.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/legacy_ball_of_mud/src/deep/nested/module/__init__.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/legacy_ball_of_mud/src/deep/nested/module/thing.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/legacy_ball_of_mud/src/hub.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/legacy_ball_of_mud/src/m1.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/legacy_ball_of_mud/src/m2.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/legacy_ball_of_mud/src/m3.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/legacy_ball_of_mud/src/m4.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/legacy_ball_of_mud/src/m5.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/legacy_ball_of_mud/src/m6.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/legacy_ball_of_mud/src/m7.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/legacy_ball_of_mud/src/m8.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/legacy_ball_of_mud/src/shallow.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/monorepo/packages/api/src/api/__init__.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/monorepo/packages/auth/src/auth/__init__.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/monorepo/packages/shared_utils/src/shared_utils/__init__.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/monorepo/packages/shared_utils/src/shared_utils/hashing.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/monorepo/packages/shared_utils/src/shared_utils/tokens.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/monorepo/packages/shared_utils/src/shared_utils/validation.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/monorepo/pyproject.toml` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/test_farm/pyproject.toml` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/test_farm/src/module_1.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/test_farm/src/module_2.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/test_farm/src/module_3.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/test_farm/src/module_4.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/test_farm/src/module_5.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/test_farm/src/module_6.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/test_farm/src/module_7.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/test_farm/src/module_8.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/test_farm/tests/test_file_1.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/test_farm/tests/test_file_2.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/test_farm/tests/test_file_3.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/test_farm/tests/test_file_4.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/test_farm/tests/test_file_5.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/archetypes/test_farm/tests/test_file_6.py` | Sample project used by the Rust decompose golden. Not the product. |
| `tests/determinism.rs` | test_farm decompose matches the golden on every OS |
| `tests/four_claims.rs` | Four processes claim one store |
| `tests/goldens/test_farm_decompose.json` | Frozen decompose output, timings removed |
| `tests/npm_packages.rs` | npm names, bins, and release.yml publish order |
| `wiki-local/glossary.md` | Current terms |
| `wiki-local/index.md` | Local wiki hub |
| `wiki-local/spec-project.md` | Short Rust product spec |

## Data flow

1. `trammel` or `trammel-mcp` enters `src/cli.rs` or `src/mcp.rs`.
2. `src/tools.rs` opens `.trammel/` when the tool needs the store, then calls the planner, harness, or store.
3. `src/lang/` and `src/syntax/` read the project. `src/implicit.rs` adds edges from `data/patterns.json`.
4. `src/store.rs` writes `store` and appends `journal`.
5. A `v*` tag runs `release.yml`, which builds the six binaries and publishes `./npm/*`.
