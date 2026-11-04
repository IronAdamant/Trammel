# Trammel: LLM development log

## Project overview

**Trammel** 4.0.1 is a std-only Rust binary (`trammel`, `trammel-mcp`) and the npm package `@iron_adamant/trammel`. It plans multi-step coding work for an LLM: dependency-aware decomposition, nine beam orderings, isolated verification, failure constraints, and a local recipe store. It does not call an LLM.

The old Python package lives on the `python-legacy` branch. Entries below 4.0.0 describe that package.

**Guidelines:** No third-party crates. `[dependencies]` stays empty. No new TODO or FIXME markers in `src/`.

## Active context

- **Version:** 4.0.1 is prepared for the MCP Registry name `io.github.IronAdamant/trammel`. npm `@iron_adamant/trammel` 4.0.0 and the six platform packages are already published. Do not tag or publish 4.0.1 from a task branch.
- **Store:** `.trammel/`. `trammel migrate` imports an old `trammel.db`.
- **CI:** `.github/workflows/rust.yml` on Linux, macOS, and Windows. Release workflow is `.github/workflows/release.yml` (unchanged). PyPI workflow removed.

## Session log

---

## 4.0.1 MCP Registry manifest

**Date:** 2026-11-04

- `trammel mcp` is the positional argument that starts the same stdio server as `trammel-mcp`. `npx -y @iron_adamant/trammel mcp` uses it, because `npx` launches the `trammel` bin.
- `server.json` names `io.github.IronAdamant/trammel`, schema `2025-12-11`, npm `@iron_adamant/trammel` 4.0.1, transport stdio, package argument `mcp`. The wrapper `package.json` sets `mcpName` to the same name.
- Crate and all seven npm packages are 4.0.1. JSON `schema_version` stays 4.0.0. Not tagged. Not published.

---

## Rust-only main

**Date:** 2026-11-04

- Branch `python-legacy` points at the pre-removal tip `dc7deb335e91e5a29541733a234dfc1ed24788a3`. Nothing on that branch was edited.
- Removed from main: the `trammel/` package, Python unittest modules, `pyproject.toml`, `.github/workflows/ci.yml`, `.github/workflows/publish.yml`, `scripts/parity_check.py`, `scripts/same_runner_bench.py`.
- Kept: Rust sources and tests, archetype and `parity/fixtures` samples, `bench/*.json`, `tests/goldens/`, npm packages, `install.sh`, `install.ps1`, `server.json`, `schemas/`, release workflow. `data/patterns.json` moved out of `trammel/data/` because `src/implicit.rs` embeds it. `tests/npm_packages.rs` keeps the npm layout checks that used to live in `tests/test_npm_packages.py`.
- CI parity job removed. It installed the Python package. `scripts/bench_gate.py` stays. It times the Rust binary against recorded floors and does not import the package.
- Docs now describe the binary and the npm package. One line in each user-facing doc points at `python-legacy`.

---

## Rust 4.0 rewrite (draft, not released)

**Date:** 2026-11-03

- Cargo package `trammel` 4.0.0, empty `[dependencies]`. Parser core copied from stele-context `244561a` (pull/5). `python_symbols` keeps a newline index so line numbers stay the same and large files stay cheap. `scripts/syntax_core_manifest.txt` records the digest, including that local change.
- Native store `.trammel/store`. `trammel migrate` reads a SQLite db and WAL read-only. `status` reads the store it already holds.
- 32 MCP tools: the 33 Python tools minus `record_step` and `create_plan`, plus `verify_plan`. Primary surface stays 12.
- Parity: five archetypes match Python 3.15.3 on files, descriptions, symbols, relevance, rationale, fingerprint, and dependency edge sets. `test_farm` edge order differs where implicit edges share a confidence. The same input is byte-identical across two Rust runs (`tests/goldens/test_farm_decompose.json`). Four processes claim one store in `tests/four_claims.rs`.
- Same-runner medians are in `bench/same_runner.json` and the README table. Rust is ahead of Python 3.15.3 on every row, and ahead of the recorded 8 vCPU baseline as well. Not tagged. Not published.
- Release path, when a person pushes tag `v4.0.0`: `.github/workflows/release.yml` builds six binaries, publishes the GitHub Release with `SHA256SUMS` using `github.token`, then publishes npm. The release job has no checkout, so `gh` commands pass `--repo`. npm publish uses `./npm/<pkg>` so the path is not a GitHub shorthand. `workflow_dispatch` publishes an existing release without rebuilding. The wrapper is `@iron_adamant/trammel` (`npm i -g @iron_adamant/trammel`, `npx @iron_adamant/trammel`). Platform packages are `@iron_adamant/trammel-<os>-<arch>`, published with `--access public` before the wrapper. A version already on the registry is skipped. Commands stay `trammel` and `trammel-mcp`. The npm job needs repository secret `NPM_TOKEN`.

---

## v3.15.3 — MCP SDK 2.x compatibility

**Date:** 2026-11-03

- `trammel-mcp` registers tools via SDK 1.x decorators or SDK 2.x `on_list_tools` / `on_call_tool`. Optional dep is `mcp>=1.0.0,<3`.
- Tool exceptions return `CallToolResult(isError=True)`. `Server(..., version=)` is Trammel's version.
- Docs: 33 tools, 15 SQLite tables, 12 `SPEC_REGISTRY` entries. CI matrix adds macOS. Windows stayed out: POSIX path assertions and SQLite `WinError 32` on temp cleanup.
- Fresh-venv handshake test (latest allowed mcp, plus mcp 1.30.0) covers `initialize` and `tools/list`.
- PyPI via Trusted Publishing when a GitHub Release `v3.15.3` is published. Do not tag from this branch.

---

## v3.15.2 — CI PEP 695 gating + docs

**Date:** 2026-07-09

- Skip PEP 695 type-alias test on Python < 3.12; pre-3.12 skip/assert path; README CI matrix note.
- PyPI via Trusted Publishing (GH Release `v3.15.2`).

---

## v3.15.0 — MCP primary surface + analyzer aging

**Date:** 2026-07-09

### Changes
- Tool tiers + `TRAMMEL_MCP_SURFACE`; compose tool `start_plan`; status tier metadata; SYSTEM_PROMPT happy path updated.
- `_strip_string_literals` for symbol preprocess; Go/Dart/TS/Python analyzer fixes; regression tests.

### Tests
- 445 passing.

---

## v3.14.0 — config test_cmd + plan export

**Date:** 2026-07-09

### Changes

- **`project_config.resolve_test_cmd`**: explicit arg > config `test_cmd` > None (analyzer default). Wired into `ExecutionHarness._effective_test_cmd` and `plan_and_execute`.
- **`trammel/export.py`**: `export_strategy` / `export_plan` / `export_plan_from_store` with `format=trammel.plan`, `format_version=1`.
- **MCP `export_plan`**, CLI `--export PATH`; public re-exports from package root.
- **Tests:** `tests/test_export_and_config.py` (14 cases) — real harness/store/dispatch paths.

### Tests
- 428 passing (was 414; +14).

---

## v3.13.0 — CI hardening (Node 24 artifact actions + verifiable publish)

**Date:** 2026-06-11

### Changes

- **`.github/workflows/publish.yml`**: `actions/upload-artifact` v5 → v7, `actions/download-artifact` v5 → v8 (v5 of both still ran Node 20, force-switched by GitHub on 2026-06-16; the new majors are the Node 24 releases — release notes checked, `name`/`path` inputs unchanged). Added `workflow_dispatch` trigger with a dispatch-only `verify-artifacts` job (download + sdist/wheel presence check) so the artifact round-trip can be exercised without publishing; `publish` job guarded to `github.event_name == 'release'`.
- **Verification**: manual dispatch run — build/test/verify-artifacts all green, publish correctly skipped, zero Node 20 deprecation annotations (previous release run had two).
- No package-code changes.

### Tests
- 414 passing (unchanged).

---

## v3.12.3 — list_plans db_path cleanup + doc parity + wikifier baseline

**Date:** 2026-06-11

### Changes

- **`list_plans` `db_path` cleanup** (`trammel/mcp_server.py`, `trammel/tool_schemas.py`): an earlier uncommitted working-tree change (from an external M5 dogfood session) added an optional `db_path` parameter so MCP clients can read plans from a different project's `trammel.db`. Kept the feature; replaced the session-log comments with a proper docstring note and rewrote the schema description/param doc to remove dogfood-session specifics.
- **New regression test** (`tests/test_mcp_dispatch.py`): `TestListPlansDbPath` verifies the default (server's own DB) and the cross-database read via `db_path`.
- **Doc parity fixes** (verified against source):
  - `README.md` — removed the stale claim that MCP `verify_step` is "static/heuristic only"; `_handle_verify_step` runs `ExecutionHarness.verify_step`, which executes the real test suite in an isolated temp copy in addition to static analysis/preflight/import-integrity checks.
  - `SYSTEM_PROMPT.md` — category list and "Tool reference" table now cover all **31** tools (added `resume`, `update_plan_status`, `record_steps`, `prune_recipes`, `deactivate_constraint`, `prune_plans`, `estimate`, `resolve_failure` where missing; header said 30).
  - `CLAUDE.md` (new contributor guide for AI agents, created this session) — corrected tool count and `verify_step` description.
- **Wikifier integration**: proper setup of the Wikifier documentation-health system — `monitored_paths.txt` scoped to `trammel/` + key root docs (was the whole repo root, which had tracked only a binary `.coordinationhub` DB), new `exclude_patterns.txt`, and an initial agent-to-agent wiki baseline: 45 `docs/wiki/*.wiki.md` entries (one per source file + `patterns.json` + 3 root docs), all marked Green. `library.md` dependency map generated (160 nodes / 397 edges). Note: requires Wikifier's exclude-pattern fix in `wikifier/scripts/wikifier.sh` (uncommitted in the Wikifier repo as of this session).

### Tests
- 414 passing (was 413; +1 new `db_path` regression test).

---

## v3.12.1 — Findings review fixes (transaction nesting + write-path hardening)

**Date:** 2026-04-29

### Fixes

- **Transaction nesting** (`trammel/utils.py`, `trammel/recipe_index.py`): Python 3.14's default `sqlite3.isolation_level=""` auto-began an implicit transaction before DML, so `backfill_recipe_index` (which ran DML outside a `transaction()` context) left an open implicit transaction; the next `BEGIN IMMEDIATE` failed with *"cannot start a transaction within a transaction"*. Switched `db_connect` to `isolation_level=None` (manual mode), made `transaction()` defensive with SAVEPOINT for nested cases, and wrapped `backfill_recipe_index` in an explicit transaction. Unblocks `create_plan`, `add_constraint`, `save_recipe`, `validate_recipes`, `log_event` (telemetry) — all four were silently or visibly failing.
- **`get_plan` JSON corruption** (`trammel/store_plans.py`): wrapped `json.loads` for the `strategy`, `scaffold`, and step JSON columns; corrupt rows now return a synthetic empty payload plus `_corrupted_fields: [...]` instead of erroring out and breaking `resume`/`available_steps`.
- **`explore` ignores scope** (`trammel/core.py`): added `_scaffold_matches_scope` so a scaffold-recipe match whose files don't intersect the queried scope is rejected. The 0.15-min-similarity scaffold match no longer overrides scope=X with a stored scaffold from elsewhere in the project.
- **`recipe_files` dedup** (`trammel/store.py`, `trammel/store_recipes.py`): one-shot cleanup on init collapses duplicate `(recipe_sig, file_path)` rows, then a unique index plus `INSERT OR IGNORE` prevent regressions. Caps the 320+ duplicate rows seen on `c54a62b9a7e6`.
- **`prune_plans` tool** (`trammel/store_plans.py`, `trammel/tool_schemas.py`, `trammel/mcp_server.py`): new MCP tool symmetric to `prune_recipes`. Default targets `pending` plans older than 7 days; cascade-deletes their steps, trajectories, and constraints. Addresses the 65 stuck pending plans observed in the review.
- **`get_recipe` ↔ `near_match_recipes` parity** (`trammel/mcp_server.py`): when `get_recipe` finds nothing above its 0.55 threshold, it now also returns `near_match_recipes` (same retrieval path as `explore`) so the two tools can no longer disagree about whether a goal is matchable.
- **`trammel_blind_spots` lifted** (`trammel/core.py`): the implicit-dependency blind-spot count now appears at the top level of `decompose` output instead of being buried in `analysis_meta.implicit_dependency_analysis`.
- **Regression tests** (`tests/test_trammel_extra.py`): added `TestFindingsRegressions` (7 tests) covering each fix above.

### Tests
- 413 passing (was 406; +7 new regression tests).

---

## v3.12.0 — Module split (every file under 500 LOC)

**Date:** 2026-04-17

> Version number rationale: README already documented a "v3.11.3" set of runtime features (over_constrained warning, `explore` fallback, auto-persisting trajectories) that had shipped in the repo without a pyproject bump. This refactor is substantial enough to warrant a minor-version bump, so pyproject goes directly from the drifted 3.11.2 to **3.12.0**, keeping the existing 3.11.3 changelog entry intact.

### Summary
Took the codebase from 9 files over the 500-LOC project-convention target (worst at 1223) down to **zero**. Extracted 14 new modules, externalized hardcoded pattern tables to `trammel/data/patterns.json`, added MCP dispatch integration tests, and documented every Planner / ExecutionHarness / RecipeStore / MCP-handler public API. All public imports preserved via re-exports from legacy modules; **406 tests pass** (395 prior + 11 new).

### Splits (by file)

- **`store_recipes.py`** (1223 → 373): extracted
  - `recipe_fingerprints.py` — structural fingerprinting + composite-match scoring (`strategy_fingerprint`, `structural_similarity`, `goal_fingerprint_from_text`, `scaffold_fingerprint`, `scaffold_structural_similarity`, `goal_scaffold_fingerprint_from_text`, `is_scaffold_pattern`, `recipe_match_components`, `sql_in`, `_FILE_ROLE_RE`, `_GOAL_ROLE_RE`).
  - `store_retrieval.py` — `RecipeRetrievalMixin` with `retrieve_best_recipe` + `retrieve_near_matches`.
  - `store_scaffolds.py` — `ScaffoldRecipeMixin` with the `scaffold_recipes` / `scaffold_trigrams` schema and related CRUD.
- **`store.py`** (745 → 313): extracted `store_plans.py` (plan/step CRUD, `complete_plan`, merge wrapper, `_STEP_COLUMNS`, `_step_to_dict`) and `store_telemetry.py` (failure patterns, status summary, usage stats, `log_event`). Also fixed an MRO bug where `RecipeStoreMixin` had a `log_event` stub that shadowed `TelemetryMixin`.
- **`mcp_server.py`** (717 → 400): extracted `tool_schemas.py` — the 30 JSON Schema definitions plus `TOOL_CATEGORIES`, `LANGUAGES`, and `coerce_int_params()`. `mcp_server.py` now holds only the `_handle_*` functions, the `_DISPATCH` table, `_validate_registries()`, and `dispatch_tool()`.
- **`implicit_deps.py`** (807 → 268): extracted `implicit_deps_engines.py` (`NamingConventionEngine`, `SharedStateDetector`, `_extract_base_name`, `_extract_suffix`, `INFRASTRUCTURE_PATTERNS`) and `pattern_learner.py` (`PatternLearner`).
- **`analyzer_engine.py`** (588 → ~195): extracted `analyzer_resolvers.py` — all per-language import-graph resolvers (`_resolve_go_imports`, `_resolve_rust_imports`, `_resolve_cpp_imports`, `_resolve_java_imports`, `_detect_java_source_roots`, `_resolve_csharp_imports`, `_resolve_ruby_imports`, `_resolve_php_imports`, `_resolve_swift_imports`, `_build_swift_module_map`, `_resolve_dart_imports`, `_resolve_zig_imports`) and their regex constants (`_GO_IMPORT_LINE_RE`, `_CARGO_*_RE`, `_MAVEN_SRC_DIR_RE`).
- **`utils.py`** (699 → 434): extracted `text_similarity.py` (trigrams, cosine, goal normalization, `_VERB_SYNONYMS`, `_ABBREVIATIONS`, `_TECH_SYNONYMS`, `expand_goal_terms`, `goal_similarity`) and `scaffold_validation.py` (`compute_scaffold_dag_metrics`, `validate_scaffold`). Lazy import in `scaffold_validation` breaks a circular with `utils.topological_sort`.
- **`scaffold_logic.py`** (649 → 179): extracted `scaffold_creation.py` — the goal→scaffold heuristic pipeline (`_creation_hints`, `_generate_creation_steps`, `_detect_layered_architecture`, `_directories_for_role_hints`, `_fallback_directories`, `_sibling_convention_clones`, `_infer_file_name`, plus `_ROLE_DIR_SUBSTRINGS`, `_FALLBACK_ROLE_DIRS`, `_LAYER_DIRS`, `_LAYER_PATTERNS` constants). `scaffold_logic.py` now re-exports them so existing `from .scaffold_logic import _creation_hints` imports continue to work.
- **`core.py`** (667 → 477): extracted `planner_helpers.py` (`decompose_scaffold_only(planner, …)`, `explore_trajectories(planner, …)`, `suggest_strategy(store, goal, language)`). `Planner._decompose_scaffold_only`, `Planner.explore_trajectories`, and the legacy `_suggest_strategy` name are thin delegations.
- **`analyzers.py`** (525 → 420): extracted `language_detection.py` (`detect_language`, `_detect_from_config`, `_detect_from_trammel_config`, `_LANG_EXTENSIONS`). Re-exported from `analyzers` for backward compatibility.

### Configuration extraction
- **`trammel/data/patterns.json`**: new JSON configuration with `naming_convention_rules` (for `NamingConventionEngine`), `infrastructure_patterns`, `file_role_patterns`, `goal_role_patterns`, and `default_convention_confidence`. Previously these were inline Python tables in `implicit_deps.py` / `store_recipes.py`.
- **`trammel/pattern_config.py`**: loader with schema validation and a cached `get_config()` accessor. `implicit_deps_engines.py` and `recipe_fingerprints.py` consume it at module load so compiled regex tables and convention rules stay identical to pre-refactor.
- **`pyproject.toml`**: `[tool.setuptools.package-data]` now includes `data/*.json` so the config ships in the wheel.

### Documentation / discoverability
- Version bumped to **3.12.0** in `pyproject.toml` (README `v3.11.3` changelog entry is retained — it describes prior shipped features; the drifted 3.11.2 → 3.12.0 bump closes the gap and marks this refactor as its own release).
- `README.md`: "Verifies as it goes" clarified — Python harness runs tests; MCP `verify_step` is static/heuristic (run your test suite yourself between steps, or call `plan_and_execute` from Python for real isolated execution).
- `SYSTEM_PROMPT.md`: new "`decompose` vs `explore` vs `create_plan` — which do I call?" decision block. `decompose` is the canonical entry point; `explore` fans out into beam variants (calls `decompose` internally — don't call both); `create_plan` persists a strategy.
- Docstrings added to `Planner` (class + every public method), `ExecutionHarness` (class + `__init__`), `RecipeStore` (class + lifecycle), every MCP `_handle_*` in `mcp_server.py`, constraint methods (`add_constraint`, `get_active_constraints`, `deactivate_constraint`, `log_trajectory`), and all 11 language analyzer subclasses.

### Testing
- **New file:** `tests/test_mcp_dispatch.py` with 11 tests covering schema↔dispatch parity, `TOOL_CATEGORIES` sync with `_DISPATCH`, unknown-tool error shape, schema field shape, previously-uncovered handlers (`record_steps`, `claim_step` / `release_step` / `available_steps`, `merge_plans`, `usage_stats`, `list_strategies`, `resolve_failure`), and schema-driven int coercion (string `plan_id` values accepted).
- Baseline 395 tests pass before every split. After final split: **406 tests pass** (395 + 11 new).

### Minor
- `analyzers_ext.py` / `analyzers_ext2.py` kept as backward-compat shims (intentional re-exports; the earlier critique mis-flagged them as dead).
- `COMPLETE_PROJECT_DOCUMENTATION.md`, `wiki-local/spec-project.md`, `wiki-local/glossary.md`, and this log updated to reflect the new module layout.

---

## v3.11.2 — Long-term improvements (plan merging, decomposition, recipes, verify_step)

**Date:** 2026-04-14

### Summary
Implemented long-term fixes for four weak areas identified in Phase 13 findings: scaffold-less decomposition, plan merging, recipe matching, and verify_step depth. Added multi-agent proximity warnings. All 358 tests pass.

### Changes
- **`trammel/plan_merge.py`**: New plan merging engine with `detect_conflicts()` (file overlap, action clash, dependency inversion, cycle introduction) and `merge_plans()` supporting `sequential`, `interleave`, `priority`, and `unified` strategies. Exposed via `merge_plans` MCP tool.
- **`trammel/core.py`**: Scaffold-less decomposition now tries `retrieve_best_scaffold_recipe()` before falling back to full-repo scan. `analysis_meta` includes `ambiguity` score/flag/signals from `_compute_ambiguity_score()`.
- **`trammel/scaffold_templates.py`**: Expanded with `crud_service` template and smarter directory resolution via `_resolve_template_directories()`.
- **`trammel/scaffold_logic.py`**: Hard-caps fallback file suggestions at 15 and uses sibling-convention cloning in `_infer_file_name()`.
- **`trammel/store_recipes.py`**: Recipe retrieval now unions TF-IDF and MinHash candidates. Added architecture-shape MinHash index (canonical role strings) and scaffold-fingerprint comparison in `retrieve_best_recipe()` / `retrieve_near_matches()`.
- **`trammel/utils.py`**: Added `_TECH_SYNONYMS` for goal normalization.
- **`trammel/harness.py`**: Deepened `verify_step` with AST syntax preflight (Python), import integrity checks, symbol reference validation, test-command existence checks, and expanded `_static_analysis` heuristics.
- **`trammel/store_agents.py`**: `claim_step()` now returns a `warning` when other active plans have pending steps targeting the same file.
- **`trammel/mcp_server.py`**: Added `merge_plans` tool schema and dispatch handler.
- **Tests**: Expanded to 358 tests. New test classes: `TestPlanValidation`, `TestAmbiguity`, `TestPreflightChecks`, `TestMergePlans`, `TestClaimProximityWarning`, `TestFallbackFileCap`, `TestRecipeMatching`.

---

## v3.11.1 — Phase 13 findings closure

**Date:** 2026-04-14

### Summary
Addressed immediate Phase 13 review recommendations: plan cycle validation, ambiguity detection, static analysis in verify_step, and MinHash recipe retrieval.

### Changes
- **`create_plan` cycle validation**: Rejects strategies that would introduce dependency cycles in step `depends_on`.
- **`decompose` ambiguity detection**: `_compute_ambiguity_score()` detects vague phrasing (`real-time`, `AI-powered`, `scalable`, etc.) and surfaces `ambiguity` metadata.
- **`verify_step` static analysis**: Added path-convention and test-coverage heuristics via `_static_analysis()`.
- **`store_recipes.py`**: Integrated MinHash LSH retrieval (`search_recipes_by_minhash`) as a secondary candidate source for recipe matching.
- **Docs**: Updated `README.md`, `SYSTEM_PROMPT.md`, `wiki-local/spec-project.md`, `COMPLETE_PROJECT_DOCUMENTATION.md`.

---

## v3.11.0 — Core modularization, recipe word index + MinHash, analyzer farm collapse

**Date:** 2026-04-14

### Summary
Major structural refactoring: extracted `core.py` into focused modules, built a zero-dep word-level recipe index with MinHash LSH, and collapsed 15 regex analyzers into a single declarative spec engine.

### Changes
- **`trammel/core.py` extraction**: Moved step generation/scoring into `scoring.py`, scaffold logic into `scaffold_logic.py`, goal NLP into `goal_nlp.py`, constraint propagation into `constraints.py`, and scaffold templates into `scaffold_templates.py`. `core.py` is now a thin orchestrator (~570 LOC).
- **`trammel/recipe_index.py`**: New `RecipeIndex` class managing `recipe_terms` (inverted word index for TF-IDF) and `recipe_signatures` (MinHash LSH signatures using stdlib `hashlib.md5`). No third-party dependencies.
- **`trammel/store.py` + `store_recipes.py`**: Schema expanded with `recipe_terms` and `recipe_signatures` tables. `save_recipe()` indexes into `RecipeIndex`; `retrieve_best_recipe()` uses `RecipeIndex.search()` as candidate source. Legacy `recipe_trigrams` retained for transition safety.
- **`trammel/analyzer_specs.py`**: Declarative `AnalyzerSpec` dataclass for all 13 regex-based analyzers (Go, Rust, C/C++, Java/Kotlin, C#, Ruby, PHP, Swift, Dart, Zig). Each spec defines `symbol_patterns`, `typed_patterns`, `strip_comments`, `test_cmd`, `error_patterns`, and an optional `ImportSpec`.
- **`trammel/analyzer_engine.py`**: Single `RegexAnalyzerEngine` backed by `AnalyzerSpec`. Strategy-specific import resolvers (`go_mod`, `rust_crate`, `cpp_include`, `java_namespace`, `csharp_namespace`, `ruby_require`, `php_namespace`, `swift_module`, `dart_package`, `zig_import`). Backward-compatible class shims (`GoAnalyzer`, `RustAnalyzer`, etc.) so existing imports keep working.
- **`trammel/analyzers_ext.py` / `analyzers_ext2.py`**: Now thin backward-compat shims re-exporting from `analyzer_engine.py`.
- **Documentation**: Updated `README.md`, `COMPLETE_PROJECT_DOCUMENTATION.md`, `LLM_Development.md`, `wiki-local/index.md`, `wiki-local/spec-project.md`.
- **Version**: `pyproject.toml` bumped to 3.11.0.

### Tests
337 passing.

---

## v3.10.4 — Adversarial scaffold hardening + structural recipe matching fix

**Date:** 2026-04-14

### Summary
Closed Review Nineteen findings from `findings/trammel.md`. Addressed two critical gaps (`decompose` crashes on circular scaffolds; empty scaffold triggers massive incorrect decompositions) and two high-severity gaps (no update mode for existing scaffold files; recipe matching structural similarity stuck at 0.0).

### Findings addressed
| Gap | Fix |
|-----|-----|
| `decompose` crashes on circular scaffolds | Added `validate_scaffold()` with DFS cycle detection; returns `{"error": "circular_dependency", "cycle": [...]}` |
| Empty scaffold defaults to full-repo refactor | Early return with `{"error": "empty_scaffold", "warning": ...}` for creation-intent goals |
| No "update existing files" scaffold mode | Scaffold entries now support `action: "update"` |
| Recipe matching structural similarity 0.0 | `_goal_fingerprint_from_text` now uses word-based `_GOAL_ROLE_RE`; scaffold recipes use `_scaffold_fingerprint` with DAG metrics |

### Changes
- **`trammel/utils.py`**: New `validate_scaffold()` function (cycles, duplicates, self-reference, over-constrained, missing deps). Hardened `compute_scaffold_dag_metrics` against cycles (`longest_path.get(d, 0)`).
- **`trammel/core.py`**: Empty-scaffold guard and scaffold validation in `Planner.decompose()`. `_scaffold_steps()` respects `action: "update"`.
- **`trammel/store_recipes.py`**: Fixed `_goal_fingerprint_from_text` word-based role detection. Added `_scaffold_fingerprint`, `_scaffold_structural_similarity`, and `_goal_scaffold_fingerprint_from_text` for DAG-aware scaffold recipe matching.
- **`tests/test_findings_checklist.py`**: 9 new adversarial scaffold tests (circular, missing, empty, over-constrained, self-ref, diamond, deep nesting, duplicate, update action).
- **Documentation**: Updated `README.md`, `COMPLETE_PROJECT_DOCUMENTATION.md`, `LLM_Development.md`. Renamed `findings/trammel.md` → `findings/trammel.md_closed`.

### Tests
337 passing (328 existing + 9 new).

---

## v3.10.3 — Documentation: scaffold DAG metrics for multi-agent dispatch

**Date:** 2026-04-10

### Summary
Documentation update based on RecipeLab Review Ten closure. Zero bugs found — Trammel's strongest performance across all review phases. Added scaffold DAG metrics documentation (`max_parallelism`, `layer_widths`, `critical_path_length`, `max_dependency_depth`) to README and spec for multi-agent workflow dispatch.

### Review Ten findings
- **decompose** (with scaffold): 40 steps, 59 edges, correct topological ordering across 6 feature trees — FLAWLESS
- **create_plan**: Plan #3 created from 40-step strategy — RELIABLE
- **complete_plan**: All 40 steps batch-processed, recipe + scaffold saved — PERFECT
- **Refactoring plan** (Plan #4): 14 steps, 5 edges, 2-layer structure — FLAWLESS
- **Recipe library**: Grew to 4 recipes spanning 3 distinct patterns (migration, feature build, refactoring)
- **DAG metrics validated**: 40 nodes, 59 edges, 6-layer depth, 12-file peak parallelism, critical path length 6

### Changes
- **README.md**: New "Scaffold DAG Metrics for Multi-Agent Dispatch" section with metric table and usage guidance; updated Architecture description; added v3.10.3 changelog entry.
- **wiki-local/spec-project.md**: Added scaffold DAG metrics documentation to §5 Planner.
- **No code changes** — documentation only.

---

## v3.10.1 — Fix create_plan scaffold migration bug

**Date:** 2026-04-09

### Summary
Fixed critical schema migration bug where `create_plan` failed with `"table plans has no column named scaffold"`. The migration code was adding the `scaffold` column to the `steps` table instead of the `plans` table, breaking the entire plan execution workflow for any existing database created before the scaffold feature was introduced.

### Bug fixed
- **`store.py` `_init_schema()`**: The migration loop added `scaffold TEXT NOT NULL DEFAULT '[]'` to the `steps` table (line 193), but `create_plan()` inserts scaffold into the `plans` table. Fresh databases were unaffected (the `CREATE TABLE` schema was correct), but any pre-existing database hit this error.
- **Fix**: Separated the migration — multi-agent columns (`claimed_by`, `claimed_at`) stay on `steps`; `scaffold` now correctly targets `plans`.

### Impact
This bug blocked 60% of Trammel's workflow: `create_plan` → `claim_step` → `record_steps` → `complete_plan` → recipe saving. All are now unblocked.

### Tests
328 passing (no new tests; migration validated manually against simulated old DB).

---

## v3.10.0 — Phase 10: Implicit Dependency Inference

**Date:** 2026-03-31

### Summary
Implemented **`ImplicitDependencyGraphEngine`** to address Phase 8 findings that Trammel is blind to ~50% of actual dependencies.

### What was built
- **`trammel/implicit_deps.py`** (~480 LOC): Four main components:
  - **`NamingConventionEngine`**: Infers dependencies from naming conventions (XService→XModel, XRoute→XService, Controller→Service/Model, etc.) with confidence scores
  - **`SharedStateDetector`**: Tracks file read/write patterns to detect implicit coupling via shared files
  - **`PatternLearner`**: Learns co-occurrence patterns, suffix-based patterns, and infrastructure usage from existing import graphs
  - **`ImplicitDependencyGraphEngine`**: Orchestrates all three; provides `get_hybrid_dependency_graph()`, `get_gap_analysis()`, `suggest_dependencies_for_new_module()`

### Integration
- `Planner.decompose()` now calls the engine after building explicit dependency graph
- Results surface in `analysis_meta.implicit_dependency_analysis`:
  - `inferred_edges`, `trammel_blind_spots`
  - Breakdown by type: `naming_convention_inferences`, `shared_state_inferences`, `pattern_based_inferences`

### Public API
New exports from `trammel/__init__.py`: `ImplicitDependencyGraphEngine`, `NamingConventionEngine`, `SharedStateDetector`, `PatternLearner`

### Bug fixed
Pattern learner co-occurrence tracking was incorrectly storing direct dependencies rather than pairs of modules that appear TOGETHER in dependency lists. Fixed by adding separate `_dep_pair_cooccurrence` and `_suffix_patterns` tracking structures.

### Tests
328 passing (23 new in `tests/test_implicit_deps.py` + 305 existing).

---

## v3.9.4 — RecipeLab findings checklist closure (CI), docs sync

**Date:** 2026-03-27

### Summary
- **`tests/test_findings_checklist.py`:** Automates former manual items A–C (estimate/list_recipes, greenfield chain, all-existing JSON, refactor+suppress, chain `critical_path_length`).
- **`COMPLETE_PROJECT_DOCUMENTATION.md`:** Version and inventory rows for `core`/`utils`/`mcp_server`, new test file row, changelog bullet.
- **`findings/trammel.md`:** Checklist marked complete; in-repo `estimate` baseline (21 files); closure date.

### Tests
305 passing.

---

## v3.7.13 — Phase 6 MCP findings: composite near-matches, relevance blend, goal paths, recipe scaffold

**Date:** 2026-03-25

### Summary
- **store_recipes:** `_recipe_match_components` shared by `retrieve_best_recipe` and `retrieve_near_matches`; near-matches accept `context_files` + `min_composite`, return `match_score` / `match_components` when structurally ranked.
- **core:** `_extract_paths_from_goal`, merged scaffold from goal + explicit scaffold (empty `scaffold=[]` still skips `creation_hints`); `_directories_for_role_hints`; relevance = keyword ratio blended with import in-degree; `strategy_to_scaffold()`.
- **mcp_server:** `get_recipe` optional `include_scaffold`.

### Tests
285 passing (`tests/test_trammel_extra.py` additions for store, planner, MCP).

---

## v3.7.12 — LLM workflow optimization: recipe dedup fix, complete_plan, match metadata

**Date:** 2026-03-24

### Summary
Three changes driven by real-world LLM usage feedback (ConsistencyHub audit):

1. **Recipe deduplication fix** — `save_recipe` was hashing the full strategy dict including `analysis_meta.timing_s` (float timing values that differ every run). Same decomposition produced different sigs, creating unbounded duplicate recipes. Fix: `_stable_strategy_sig()` strips volatile keys (`_source`, `analysis_meta`) before computing SHA-256. Now identical decompositions always merge into the same recipe entry, incrementing its success counter.

2. **`complete_plan` compound tool** — Single-agent workflows required 3+ sequential tool calls to close out a plan (`record_step` x N + `update_plan_status` + `save_recipe`). New `complete_plan(plan_id, outcome)` does all three atomically: batch-marks remaining pending steps, sets plan status, saves recipe. Reduces single-agent close-out from ~4 tool calls to 1.

3. **Match metadata on `get_recipe`** — Previously returned the raw strategy dict, giving LLMs no signal about match quality. Now returns `{sig, pattern, match_score, match_components, successes, failures, strategy: {...}}` at the top level. LLMs can inspect `match_score` and `match_components.text_similarity` to decide whether to trust the recipe or decompose fresh.

### Changes

**store_recipes.py:**
- Added `_VOLATILE_STRATEGY_KEYS = frozenset({"_source", "analysis_meta"})` class constant
- Added `_stable_strategy_sig(strategy)` classmethod — strips volatile keys, then `sha256_json`
- `save_recipe` now calls `_stable_strategy_sig` instead of `sha256_json`
- `retrieve_best_recipe` attaches `_match` metadata dict to returned recipe: `{sig, pattern, match_score, match_components, successes, failures}`

**store.py:**
- Added `complete_plan(plan_id, outcome, step_status="passed")` — batch-updates pending steps, sets plan status, saves recipe

**mcp_server.py:**
- Added `complete_plan` tool schema and `_handle_complete_plan` handler
- `_handle_get_recipe` now surfaces match metadata at top level with strategy nested under `strategy` key (28 tools total)

**SYSTEM_PROMPT.md:**
- Updated tool reference (27 → 28 tools)
- Added `complete_plan` to tool table and "Close out" workflow section
- Documented `get_recipe` match metadata fields

**tests/test_trammel_extra.py:**
- Updated `test_save_and_get_recipe_dispatch` to expect new response structure (`strategy` nested, `match_score`/`match_components` present)

### Files changed
- `trammel/store_recipes.py` — stable sig, match metadata
- `trammel/store.py` — `complete_plan` compound method
- `trammel/mcp_server.py` — new tool + response reshaping
- `SYSTEM_PROMPT.md` — updated docs
- `COMPLETE_PROJECT_DOCUMENTATION.md` — updated inventory
- `pyproject.toml` — version 3.7.12
- `tests/test_trammel_extra.py` — updated test assertion

---

## v3.7.11 — Fix MCP integer coercion bug, add decompose output filtering

**Date:** 2026-03-24

---

## v3.7.10 — Fix cross-thread SQLite crash in MCP stdio server

**Date:** 2026-03-24

### Summary
Fixed the "SQLite objects created in a thread can only be used in that same thread" error that made every Trammel MCP tool call fail under Python 3.14. The root cause: `_configure_server` accepted a pre-built `RecipeStore` (created on the asyncio event-loop thread), but `call_tool` dispatched work via `asyncio.to_thread` to a worker thread — crossing the thread boundary that Python 3.14's sqlite3 now strictly enforces (`check_same_thread=True` by default).

Fix: `_configure_server` now takes `db_path: str` instead of a shared `RecipeStore`. Each `call_tool` invocation creates a short-lived `RecipeStore` inside a `_run()` closure that executes within the `asyncio.to_thread` worker, so the `sqlite3.Connection` is created and consumed on the same thread. The `with` statement ensures the connection is closed after each call. `_run_server` no longer wraps the server lifecycle in a `RecipeStore` context manager.

### Changes

**mcp_stdio.py:**
- `_configure_server(store: RecipeStore)` → `_configure_server(db_path: str)` — no longer receives a shared connection
- `call_tool`: new `_run()` closure creates `RecipeStore(db_path)` per-call inside the worker thread
- `_run_server`: removed `with RecipeStore(db_path) as store:` wrapper — connection lifecycle is now per-call

**pyproject.toml:**
- Version 3.7.10

### Files modified
- `trammel/mcp_stdio.py` — per-call connection fix
- `pyproject.toml` — version bump

---

## v3.7.9 — Comprehensive cleanup: dead code, bug fixes, modernization, robustness

**Date:** 2026-03-23

### Summary
Full codebase audit and cleanup: (1) fixed CLI goal validation bug (`str(None)` silently producing `"None"`), added `--root` directory validation, (2) added missing `created` column to `_STEP_COLUMNS`/`_step_to_dict`, (3) mixin stubs now raise `NotImplementedError` instead of silently returning `None`, (4) replaced all fragile positional unpacking of `sqlite3.Row` with named column access in `store_recipes.py`, (5) added empty-input guard to `_sql_in`, (6) renamed public `cosine` to private `_cosine`, (7) removed dead `_is_ignored_dir` import from `analyzers_ext2.py`, (8) pre-compiled Rust/Cargo regex patterns at module level instead of per-file, (9) pre-compiled Maven `sourceDirectory` regex, (10) simplified beam-count capping into named variable, extracted `_step_description` helper, (11) modernized `_extract_step_files` with walrus operator, (12) moved `sqlite3` imports under `TYPE_CHECKING` in store mixins, (13) moved late `_ANALYZER_REGISTRY` import into `_validate_registries()` function in `mcp_server.py`, replaced bare `detect_language` call with lazy `_detect_language()` wrapper, moved `_collect_project_files` to top-level import, (14) simplified `_handle_list_strategies` from obscure one-element tuple comprehension to readable loop, (15) added `KeyboardInterrupt` handling to MCP stdio entry point, (16) added `py.typed` to `[tool.setuptools.package-data]`, (17) fixed misleading Zig symbol patterns comment, (18) documented Laplace smoothing in `strategy_win_rates`, (19) fixed `test_beam_count_respects_cap` to actually test source code instead of local math, (20) consolidated `import threading` to module level in tests. All 248 tests pass.

### Changes

**Bug fixes:**
- `cli.py` — `str(payload.get("goal", ""))` converted JSON `null` to string `"None"` silently; now validates goal is a non-empty string. Added `--root` directory existence check.
- `store.py` — `_STEP_COLUMNS` was missing the `created` column; `_step_to_dict` now includes it.
- `store_agents.py`, `store_recipes.py` — Mixin stub methods (`get_plan`, `log_event`) silently returned `None` via `...` body; now raise `NotImplementedError`.
- `store_recipes.py` — `_sql_in([])` produced invalid `IN ()` SQL; now raises `ValueError`.

**Dead code removal:**
- `analyzers_ext2.py` — Removed unused `_is_ignored_dir` import.

**Fragile code hardening:**
- `store_recipes.py` — Replaced all positional `sqlite3.Row` unpacking (8 call sites: `_rebuild_trigram_index`, `_backfill_files`, `retrieve_best_recipe`, `list_recipes`, `validate_recipes`) with named column access.

**Performance:**
- `analyzers_ext.py` — Pre-compiled 5 Rust `use` regex patterns and 3 Cargo regex patterns at module level. Pre-compiled per-workspace-crate regexes once before the file loop (was N*M recompilation). Pre-compiled Maven `_MAVEN_SRC_DIR_RE`.

**Modernization:**
- `store_recipes.py` — `_extract_step_files` uses walrus operator (`:=`) instead of double `.get("file")` call.
- `store_agents.py`, `store_recipes.py` — Moved `import sqlite3` under `TYPE_CHECKING` block (only needed for annotations).
- `utils.py` — Renamed public `cosine()` to `_cosine()` (private naming convention).

**Simplification:**
- `core.py` — Beam-count capping `min(num_beams, max(3, min(12, cores)))` broken into named `cap` variable. Nested f-string in step description extracted to `_step_description()` helper.
- `mcp_server.py` — `_handle_list_strategies` rewritten from obscure one-element tuple comprehension to plain loop. Late module-level `_ANALYZER_REGISTRY` import wrapped in `_validate_registries()` function. Inline imports in `_handle_estimate` moved to top-level `_collect_project_files` import and lazy `_detect_language()` wrapper.
- `store.py` — Module-level `_log = logging.getLogger(__name__)` replaces per-call logger creation. Documented Laplace smoothing formula.

**Robustness:**
- `mcp_stdio.py` — `main()` now catches `KeyboardInterrupt` for clean exit.
- `pyproject.toml` — Added `[tool.setuptools.package-data]` section including `py.typed` marker (was declared as typed but marker not packaged).
- `analyzers_ext2.py` — Fixed misleading comment on `_ZIG_SYMBOL_PATTERNS` (said "omit type_alias" but type_alias was present).

**Test fixes:**
- `test_trammel_extra.py` — `test_beam_count_respects_cap` now actually calls `planner.explore_trajectories()` with mocked CPU count instead of testing local arithmetic. Consolidated 3 redundant inline `import threading` statements to single module-level import.
- `test_trammel.py` — Updated `cosine` → `_cosine` references.

### Files changed
- `trammel/analyzers_ext.py` — Pre-compiled regex constants, crate regex pre-compilation
- `trammel/analyzers_ext2.py` — Removed dead import, fixed Zig comment
- `trammel/cli.py` — Goal validation, root validation
- `trammel/core.py` — `_step_description()` helper, named beam cap variable
- `trammel/harness.py` — No changes (reviewed, confirmed correct)
- `trammel/mcp_server.py` — `_validate_registries()`, `_detect_language()`, simplified list_strategies, top-level import
- `trammel/mcp_stdio.py` — KeyboardInterrupt handling
- `trammel/store.py` — `created` in `_STEP_COLUMNS`, module-level logger, documented smoothing
- `trammel/store_agents.py` — `NotImplementedError` stub, `TYPE_CHECKING` import
- `trammel/store_recipes.py` — Named row access, `_sql_in` guard, walrus `_extract_step_files`, `NotImplementedError` stub, `TYPE_CHECKING` import
- `trammel/utils.py` — `_cosine` rename
- `pyproject.toml` — Version 3.7.9, `package-data` section
- `tests/test_trammel.py` — `_cosine` references
- `tests/test_trammel_extra.py` — Real beam cap test, consolidated imports

---

## v3.7.8 — Code quality: DRY step dicts, simplified helpers, consistent isinstance

**Date:** 2026-03-23

### Summary
Code quality pass: (1) extracted `_STEP_COLUMNS` constant and `_step_to_dict()` helper in store.py, eliminating duplicated step-dict construction between `get_plan()` and `get_step()`, (2) simplified `word_jaccard`, `cosine`, and `_strip_php_comments` in utils.py to reduce line count, (3) fixed inconsistent `type()` vs `isinstance()` usage in `PythonAnalyzer.collect_typed_symbols` (analyzers.py:79). All 248 tests pass.

### Changes

**_step_to_dict() and _STEP_COLUMNS (store.py):**
- New module-level `_STEP_COLUMNS` string constant — single source of truth for step SELECT columns
- New `_step_to_dict(row)` converts a sqlite3.Row to parsed dict (handles JSON fields)
- `get_plan()` step list comprehension replaced with `[_step_to_dict(s) for s in steps]`
- `get_step()` reduced from 18 lines to 4 lines using the shared helper

**Simplified helpers (utils.py):**
- `_strip_php_comments`: two-statement body merged into single return expression
- `word_jaccard`: 6-line body simplified to 3 lines using ternary on union set
- `cosine`: 6-line body simplified to 3 lines by combining norm computation and guard

**Consistent isinstance (analyzers.py):**
- `PythonAnalyzer.collect_typed_symbols` line 79: changed `type(n) in _PY_TYPE_MAP` to `isinstance(n, _PY_AST_TYPES)` for consistency with `collect_symbols` (line 70)

### Files changed
- `trammel/store.py` — `_STEP_COLUMNS`, `_step_to_dict()`, simplified `get_plan()` and `get_step()`
- `trammel/utils.py` — Simplified `_strip_php_comments`, `word_jaccard`, `cosine`
- `trammel/analyzers.py` — Consistent `isinstance()` in `collect_typed_symbols`
- `pyproject.toml` — Version bump to 3.7.8

---

## v3.7.7 — Structural deduplication: sqlite3.Row, shared step-file accessor, namespace resolver

**Date:** 2026-03-23

### Summary
Three structural deduplication passes targeting the remaining audit findings: (1) replaced all fragile positional tuple indexing in the store layer with `sqlite3.Row` named column access, (2) extracted `_step_file()` helper in strategies.py eliminating 17 repeated `.get("file", "")` calls, (3) extracted `_resolve_namespace_import()` shared utility deduplicating identical prefix-matching loops in Java/C#/PHP analyzers. All 248 tests pass.

### Changes

**sqlite3.Row named columns (store.py, store_recipes.py, store_agents.py, utils.py):**
- Set `conn.row_factory = sqlite3.Row` in `db_connect()` — all query results now support named column access
- Replaced all `row[0]`/`row[1]`/... positional indexing in `get_plan()`, `get_step()`, `list_plans()`, `get_active_constraints()`, `get_trajectories()`, `get_strategy_stats()`, `record_failure_pattern()`, `get_failure_history()`, `get_status_summary()`, `get_usage_stats()` with `row["column_name"]`
- Updated `store_agents.py` `claim_step` from `row[0]`/`row[1]`/`row[2]` to `row["status"]`/`row["claimed_by"]`/`row["claimed_at"]`
- Updated `store_recipes.py` `retrieve_best_recipe`, `list_recipes`, `prune_recipes` from `row[0]` to `row["sig"]`/`row["recipe_sig"]`
- Added named aliases to `get_status_summary` SQL for `sqlite3.Row` compatibility

**_step_file() helper (strategies.py):**
- New `_step_file(step)` extracts `step.get("file", "")` — single source of truth
- Replaced 17 occurrences across 9 strategy functions (including both `s` and `st` variable names)

**_resolve_namespace_import() (utils.py, analyzers_ext.py, analyzers_ext2.py):**
- New shared utility: tries progressively shorter dotted prefixes against a namespace-to-files mapping
- JavaAnalyzer: 7-line inline loop → single `_resolve_namespace_import()` call
- CSharpAnalyzer: 7-line inline loop → single `_resolve_namespace_import()` call
- PhpAnalyzer: 8-line inline loop with `ns_dot_map` indirection → normalized `dot_ns_to_files` dict + single `_resolve_namespace_import()` call

### Files changed
- `trammel/utils.py` — `sqlite3.Row` factory, `_resolve_namespace_import()` utility
- `trammel/store.py` — Named column access throughout
- `trammel/store_recipes.py` — Named column access for sig lookups
- `trammel/store_agents.py` — Named column access in `claim_step`
- `trammel/strategies.py` — `_step_file()` helper, 17 call sites updated
- `trammel/analyzers_ext.py` — Java uses shared resolver
- `trammel/analyzers_ext2.py` — C# and PHP use shared resolver
- `pyproject.toml` — Version bump to 3.7.7

---

## v3.7.6 — Code quality audit: bug fixes, robustness, modernization, test consistency

**Date:** 2026-03-23

### Summary
Full codebase audit via 5 parallel agents across all 15 source files and 4 test files. Fixed 2 bugs (unsafe CLI JSON parsing, fragile harness failure_analysis access), applied code simplifications and modernization across 7 files, and standardized return type hints on 24 test methods. All 248 tests pass.

### Bug fixes
- **cli.py:** Added type validation for JSON stdin payload — non-dict JSON (e.g., `["goal"]`) previously crashed with `AttributeError` instead of a clean error message.
- **harness.py:** Fixed fragile `failure_analysis` access in `run_incremental` — if `failure_analysis` key existed but was explicitly `None`, the chained `.get()` call would crash with `AttributeError`. Changed to `(result.get("failure_analysis") or {}).get("message", "")`.

### Simplification & modernization
- **core.py:** Added `+N more` truncation indicator to `_step_rationale` dependency list (consistent with symbol list formatting). Simplified beam-count capping from nested `min(num_beams, min(12, max(3, cores)))` to clearer `min(num_beams, max(3, min(12, cores)))`.
- **analyzers.py:** Replaced manual JS extension stripping (`import_path[: -len(js_ext)]`) with `removesuffix()` (Python 3.9+ built-in).
- **mcp_server.py:** Applied walrus operator in `_handle_get_recipe` for cleaner context_files extraction. Eliminated double `stats.get(name, (0, 0))` lookup in `_handle_list_strategies` using generator expression binding.

### Test consistency
- **test_strategies.py:** Added `-> None` return type hints to 9 test methods (TestNewStrategies class).
- **test_analyzers.py:** Added `-> None` return type hints to 15 test methods (TypeScript symbol/import tests and tsconfig tests).

### Files changed
- `trammel/cli.py` — JSON type validation
- `trammel/harness.py` — Robust failure_analysis access
- `trammel/core.py` — Rationale formatting, beam count simplification
- `trammel/analyzers.py` — removesuffix() modernization
- `trammel/mcp_server.py` — Walrus operator, comprehension dedup
- `tests/test_strategies.py` — 9 type hints added
- `tests/test_analyzers.py` — 15 type hints added
- `pyproject.toml` — Version bump to 3.7.6

---

## v3.7.5 — Code quality audit: analyzer deduplication and shared namespace walker

**Date:** 2026-03-23

### Summary
Multi-agent parallel audit of all 15 source files and 4 test files. Identified and eliminated the largest remaining code duplication: three analyzer `analyze_imports` methods (CSharp, PHP, Java) shared a near-identical 15–20 line walk-and-map-namespaces pattern. Extracted `_walk_and_map_namespaces()` into `utils.py` as a shared utility, then refactored all three analyzers to use it. No dead code, TODOs, or truncated code found. 248 tests pass (unchanged).

### Changes
- **Extracted `_walk_and_map_namespaces()`** (utils.py): New shared utility function that walks project files, reads sources with comment stripping, and builds a namespace/package → files mapping. Accepts `extensions`, `namespace_re`, `preprocess` function, and optional `source_roots` for Java-style multi-root projects.
- **Refactored CSharpAnalyzer.analyze_imports** (analyzers_ext2.py): Replaced 15-line inline os.walk+namespace extraction with single `_walk_and_map_namespaces()` call.
- **Refactored PhpAnalyzer.analyze_imports** (analyzers_ext2.py): Replaced 15-line inline os.walk+namespace extraction with single `_walk_and_map_namespaces()` call.
- **Refactored JavaAnalyzer.analyze_imports** (analyzers_ext.py): Replaced 17-line inline os.walk+package extraction with single `_walk_and_map_namespaces()` call using `source_roots` parameter.

### Audit findings (no action needed)
- **No dead code:** All functions and classes are referenced in source or tests.
- **No TODOs/FIXMEs:** Codebase is clean of placeholder comments.
- **No truncated code:** All mixin protocol methods (`...`) are intentional.
- **No broken imports:** All test imports match actual exports.
- **Exception handling:** All uses are specific and appropriate; no bare `except:`.

---

## v3.7.4 — Code quality audit: dead code removal, simplification, modernization

**Date:** 2026-03-23

### Summary
Multi-agent parallel audit of all 15 source files. Removed dead code (`_default_beam_count`), fixed potential ZeroDivisionError and ValueError, replaced fragile `assert` with `RuntimeError`, modernized async patterns, consolidated SQL queries, extracted reusable `_sql_in()` helper, eliminated redundant dict copies, and simplified confusing comprehension patterns. 248 tests pass (unchanged).

### Changes
- **Fixed ZeroDivisionError** (core.py): `explore_trajectories` could crash on `i % len(ordered_variants)` when no strategies registered — added empty-guard early return.
- **Fixed ValueError** (\_\_init\_\_.py): `ProcessPoolExecutor(max_workers=0)` raised ValueError when called with 0 beams — clamped to `max(1, ...)`.
- **Removed dead code** (strategies.py): Deleted `_default_beam_count()` — logic inlined into `core.py:explore_trajectories()` where it's actually used.
- **Replaced assert with RuntimeError** (mcp_server.py): Schema/dispatch and language/analyzer sync checks now use `RuntimeError` instead of `assert`, surviving Python's `-O` flag.
- **Fixed logger initialization order** (mcp_stdio.py): Moved `logging.basicConfig()` to module level so early exceptions are properly logged.
- **Modernized async dispatch** (mcp_stdio.py): Replaced verbose `loop.run_in_executor(None, lambda: ...)` with `asyncio.to_thread()` (Python 3.9+).
- **Consolidated SQL counts** (store.py): `get_status_summary()` reduced from 4 separate `SELECT COUNT(*)` queries to a single scalar-subquery SQL statement.
- **Extracted `_sql_in()` helper** (store_recipes.py): Reusable SQL IN-clause builder deduplicates 8 instances of repeated `",".join("?" for _ in items)` pattern.
- **Added `_MAX_LOG_GOAL_LENGTH` constant** (store_recipes.py): Replaces magic `100` in recipe log event goal truncation.
- **Replaced dead `TYPE_CHECKING: pass`** (store_recipes.py): Now imports `RecipeStore` for documentation of mixin host class.
- **Eliminated redundant dict copies** (core.py): Recipe fast-path returns used `{**recipe, "_source": ...}` creating unnecessary copies — now uses `setdefault()` in place.
- **Added lazy `_get_analyzer_registry()`** (core.py): Replaces late inline `from .analyzers import _ANALYZER_REGISTRY` inside `decompose()` method body.
- **Simplified comprehension** (mcp_server.py): `_handle_list_strategies` removed confusing `for s, f in [stats.get(...)]` single-element list unpacking.
- **Updated test** (test_trammel_extra.py): Removed import of deleted `_default_beam_count`, test now validates beam-count logic directly.

---

## v3.7.3 — Code quality audit: deduplication, modernization, and hardening

**Date:** 2026-03-23

### Summary
Comprehensive 5-agent parallel audit of all 15 source files. Deduplicated code in store_recipes and Swift analyzer, modernized strategies with Counter, narrowed exception handling, fixed type annotations, improved symbol deduplication performance, extracted magic numbers to named constants, and marked unused handler parameters. 248 tests pass (unchanged).

### Changes
- **Narrowed telemetry exception** (store.py): `log_event()` caught bare `except Exception` — now catches only `sqlite3.Error` for precise error handling.
- **Fixed type annotation** (store.py): `get_failure_history()` params annotated as `tuple[Any, ...]` but mixed `str` and `int` — now `tuple[str | int, ...]`.
- **Set-based symbol deduplication** (utils.py): `_collect_symbols_regex()` used `list` with `O(n)` membership checks — added `set` tracking for `O(1)` lookups.
- **Deduplicated recipe helpers** (store_recipes.py): Extracted `_insert_trigrams()`, `_extract_step_files()`, and `_insert_file_entries()` — eliminated 3 instances of repeated trigram insertion and 2 instances of duplicated file extraction logic.
- **Named constants for magic numbers** (store_recipes.py): `_MAX_PATTERN_LENGTH = 200` and `_NEAR_PERFECT_SIMILARITY = 0.9999` replace inline magic values.
- **Modernized `_count_importers`** (strategies.py): Replaced manual dict accumulation with `collections.Counter` one-liner.
- **Deduplicated Swift SPM scanning** (analyzers_ext2.py): Extracted `_scan_spm_dir()` helper — Sources and Tests directory scanning was near-identical.
- **Fixed Dart import mutual exclusivity** (analyzers_ext2.py): Direct and relative import resolution now uses `elif` to avoid redundant resolution when direct match succeeds.
- **Simplified string building** (core.py): Replaced `+` concatenation with f-string continuation for step descriptions.
- **Marked unused handler parameters** (mcp_server.py): Prefixed unused `store`/`args` params with underscore in `_handle_verify_step`, `_handle_status`, `_handle_list_strategies`, `_handle_estimate`.

---

## v3.7.2 — Codebase audit and cleanup

**Date:** 2026-03-23

### Summary
Full codebase audit across all 14 source files. Fixed 4 bugs (None dict keys, recipe mutation, hardcoded constant, missing API fields), improved cross-platform path handling in Swift analyzer, and applied minor code simplifications. 248 tests pass (unchanged).

### Changes
- **Fixed `_inject_orderings` None key bug** (core.py): Dict comprehension `{s.get("file"): i ...}` could produce `None` keys when steps lack a `file` field. Now uses walrus operator with `is not None` guard.
- **Fixed recipe mutation** (core.py): `decompose()` was mutating dicts returned by `retrieve_best_recipe()` via `setdefault()`. Now returns shallow copies with `{**recipe, "_source": ...}` to avoid corrupting store-internal state.
- **Used `DEFAULT_DB_PATH` constant** (__init__.py): `synthesize()` had a hardcoded `"trammel.db"` default instead of using the `DEFAULT_DB_PATH` constant (already used by `plan_and_execute()` and `explore()`).
- **Added missing fields to `get_step()`** (store.py): `get_step()` was missing `claimed_by` and `claimed_at` columns that `get_plan()` returns — API inconsistency for multi-agent step coordination.
- **Normalized Swift path separators** (analyzers_ext2.py): `SwiftAnalyzer._build_module_map()` used `f.startswith(prefix + os.sep) or f.startswith(prefix + "/")` — now normalizes to forward slashes before comparison for cross-platform correctness.
- **Minor**: `dict(step)` → `step.copy()` (core.py), removed stray blank line (analyzers.py).

---

## v3.7.1 — Mixin type safety, robustness, performance, test coverage

**Date:** 2026-03-23

### Summary
Follow-up to v3.7.0 audit: addressed 5 architectural items flagged during review. Added type safety for mixins, hardened multi-agent step claiming, narrowed parallel exception handling, optimized incremental verification, and closed typed-symbol test coverage gap. 248 tests pass (9 new).

### Changes
- **Mixin type safety** (store_recipes.py, store_agents.py): Added `conn: sqlite3.Connection`, `log_event()`, and `get_plan()` typed stubs under `TYPE_CHECKING` so type checkers can validate mixin attribute access without runtime overhead.
- **`claim_step` status guard** (store_agents.py): Now selects and checks `status` column — rejects non-pending steps. Previously a direct `claim_step` call could claim completed/failed steps.
- **Narrowed parallel exception catch** (__init__.py): Changed `except (OSError, RuntimeError)` to `except OSError` — `RuntimeError` from worker processes now propagates instead of being silently swallowed. Added `logging.debug` so fallback is visible.
- **`run_incremental` O(K) optimization** (harness.py): Replaced O(K^2) approach (copy original + re-apply all accumulated edits per step) with persistent base directory that accumulates edits. Each step now only applies its own edits.
- **9 new typed-symbol tests** (test_analyzers.py): Added `collect_typed_symbols` coverage for Rust, C++, Java, C#, Ruby, PHP, Swift, Dart, Zig.

---

## v3.7.0 — Comprehensive audit, bug fixes, and modernization

**Date:** 2026-03-23

### Summary
Deep codebase audit across all 15 source files and 4 test files. Fixed 8 bugs (including 3 data-corrupting ones), removed dead code, eliminated duplication, consolidated constants, and modernized imports. All 239 tests pass.

### Bug fixes
- **`_order_critical_path` cycle leak** (strategies.py): Cycle detection did not remove nodes from `in_stack`, causing subsequent dependent nodes to be falsely flagged as cycles. Fixed by adding `in_stack.discard(node)`.
- **`log_event` bare commit** (store.py): Used `self.conn.commit()` outside `transaction()` context manager, bypassing BUSY retry logic. Now wrapped in `with transaction()`.
- **Schema migration too-broad catch** (store.py): `ALTER TABLE` migration caught all `OperationalError`, potentially masking real errors. Now only suppresses "duplicate column" errors.
- **Inconsistent win-rate formulas** (store.py vs core.py): `get_usage_stats` used raw ratio while `explore_trajectories` used Laplace smoothing. Unified to Laplace smoothing (`s / (s + f + 1)`).
- **`trigram_signature()[:8]` produced `list[float]`** (core.py): Goal fingerprint was a list of floats, not a string. Replaced with `sha256_json(goal)[:16]` for a proper hex hash fingerprint.
- **Rust `_read_cargo_crates` relpath bug** (analyzers_ext.py): `os.path.relpath(src_dir, ".")` computed paths relative to CWD instead of project root. Fixed to use `src_dir` directly.
- **Java `analyze_imports` missing comment stripping** (analyzers_ext.py): Import statements inside block comments were falsely included in the dependency graph. Added `_strip_c_comments()` call.
- **PHP grouped-use alias stripping** (analyzers_ext2.py): `use Foo\{Bar, Baz as B}` produced `Foo\Baz as B` instead of `Foo\Baz`. Added `item.split(" as ")[0]`.

### Dead code removal
- Removed `ExecutionHarness.run()` (harness.py): trivial one-line alias for `verify_step()`, never called externally. Updated 3 test call sites.

### Code simplification
- **N+1 query eliminated** (store_recipes.py): `retrieve_best_recipe` fetched file paths per-candidate in a loop. Replaced with single batch-fetch query.
- **`prune_recipes` SQL-side filtering** (store_recipes.py): Replaced Python-side loop with SQL `WHERE` clause for candidate selection.
- **`_walk_project_sources` generator** (utils.py): Extracted shared os.walk + dir-filter + file-read logic from `_collect_symbols_regex` and `_collect_typed_symbols_regex`, eliminating ~30 lines of duplication.
- **`DEFAULT_DB_PATH` constant** (utils.py): Consolidated `"trammel.db"` hardcoded in 6 locations (cli.py, mcp_stdio.py, store.py, __init__.py ×3) into a single `DEFAULT_DB_PATH` constant.
- **Redundant `has()` in C# detection** (analyzers.py): `any(has(f) for f in os.listdir(...) if f.endswith(...))` redundantly stat'd files already listed. Simplified to direct `.endswith()` check.

### Modernization
- **`_SUPPORTED_LANGUAGES` derived from registry** (core.py): Removed hardcoded frozenset that could drift from `_ANALYZER_REGISTRY`. Now imports registry directly.
- **Language/analyzer sync assertion** (mcp_server.py): Added `assert set(_LANGUAGES) == set(_ANALYZER_REGISTRY)` to catch language list drift at import time.
- **Mid-file imports moved to top** (analyzers.py, test_analyzers.py): `analyzers_ext`/`analyzers_ext2` imports moved from mid-file to top-level imports block.
- **Removed premature Python 3.14 classifier** (pyproject.toml): Python 3.14 is still in pre-release.
- **Consistent `unittest.mock` import** (test_strategies.py): Changed `import unittest.mock` + `@unittest.mock.patch` to `from unittest.mock import patch` + `@patch`.

---

## v3.6.0 — Codebase audit, bug fixes, and modernization

**Date:** 2026-03-23

### Summary
Comprehensive codebase audit and cleanup: fixed 3 bugs, eliminated 10 instances of duplicated logic, simplified code, modernized imports, and improved packaging. All 239 tests pass.

### Bug fixes
- **TOCTOU race in `record_failure_pattern`** (store.py): SELECT + INSERT/UPDATE was outside a transaction — concurrent agents could create duplicate patterns. Now wrapped in `with transaction()`.
- **Missing transaction in `resolve_failure_pattern`** (store.py): UPDATE + commit was bare — now wrapped in `with transaction()`.
- **`JavaAnalyzer.pick_test_cmd` fallback** (analyzers_ext.py): Fell back to `./gradlew test` even when no `gradlew` file existed. Changed to `gradle test` (system binary).

### Duplication elimination
- **`_count_importers()` helper** (strategies.py): Extracted from 3x identical importer-counting blocks in `_order_risk_first`, `_order_leaf_first`, `_order_hub_first`.
- **`run()` delegates to `verify_step()`** (harness.py): `run()` duplicated `verify_step()` body verbatim — now a one-line delegation.
- **`_is_claimed_by_other()` helper** (store_agents.py): Extracted stale-claim check duplicated between `claim_step` and `get_available_steps`.
- **`_try_resolve()` helper** (analyzers.py): Extracted identical extension-try loop from `_resolve_ts_path` and `_resolve_alias`.
- **Removed redundant `_collect_files` wrappers**: `TypeScriptAnalyzer._collect_files` and `CppAnalyzer._collect_files` were one-liner wrappers around `_collect_project_files` — callers now call the utility directly.
- **Merged two transactions into one** in `validate_recipes` (store_recipes.py).

### Dead code removal
- Removed always-true `self.store is not None` guard (core.py)
- Removed unnecessary `or ""` on `subprocess.run(text=True)` output (harness.py)
- Removed redundant `success = False` assignment (store.py)
- Removed ineffective deferred import of `explore` in cli.py (module already loaded)

### Modernization
- `Callable` and `Generator` imported from `collections.abc` instead of deprecated `typing` (utils.py, strategies.py, mcp_server.py)
- Added PEP 561 `py.typed` marker file (matches `Typing :: Typed` classifier)
- Added `get_analyzer` to `__init__.py` `__all__` (was importable but undiscoverable)
- Added `.mypy_cache/` and `.ruff_cache/` to `.gitignore` (consistent with `_IGNORED_DIRS`)

### Simplifications
- `_split_active_skipped` now single-pass (was iterating list twice)
- `_handle_list_strategies` eliminated redundant `stats.get()` double-call
- Fragile `c["description"]` changed to `c.get("description", "")` in core.py

### Test improvements
- `{(n, t) for n, t in entries}` simplified to `set(entries)` in test_analyzers.py
- Hardcoded tool count `27` replaced with `len(_TOOL_SCHEMAS)` in test_trammel_extra.py
- Removed stale migration comment in test_strategies.py

---

## v3.5.2 — Codebase cleanup & modernization

**Date:** 2026-03-23

### Summary
Comprehensive codebase audit and cleanup: eliminated dead code, consolidated duplicated patterns, fixed bugs, modernized syntax, and improved type safety across all modules.

### Changes

#### Consolidation
- Removed duplicate comment strippers: `_strip_js_comments` (analyzers.py) and `_strip_cpp_comments` (analyzers_ext.py) → all analyzers now use `_strip_c_comments` from utils
- Removed redundant `store` parameter from `Planner.explore_trajectories()` — method now uses `self.store`
- Consolidated `get_analyzer` imports in `__init__.py` — removed redundant lazy imports since module is already eagerly imported
- `_handle_status` in mcp_server.py now delegates to `RecipeStore.get_status_summary()` instead of raw SQL queries
- `_handle_estimate` in mcp_server.py now uses shared `_get_analyzer()` helper instead of inline import logic

#### Bug Fixes
- PHP `_PHP_TYPED_PATTERNS`: changed method modifier quantifier from `*` to `+` to prevent overlap with the function pattern
- Dart `_DART_TYPED_PATTERNS`: added negative lookahead for control flow keywords (`if`, `for`, `while`, `do`, `switch`, `catch`) to prevent false positive function matches
- `detect_language()`: fixed `max(counts, key=counts.get)` → `key=lambda k: counts[k]` for type safety
- `store_recipes.py`: fixed float equality `text_sim == 1.0` → `text_sim >= 0.9999` for floating-point safety
- `store.py`: fixed `list_plans` status filter from `if status:` to `if status is not None:` (empty string edge case)
- `store.py`: fixed `get_failure_history` params type annotation from bare `tuple` to `tuple[Any, ...]`
- `store.py`: fixed `get_strategy_stats` return — uses explicit `(v[0], v[1])` instead of `tuple(v)` for correct static typing
- `core.py`: fixed hardcoded "No Python symbols" fallback message → "No symbols found"
- `core.py`: changed "imports from" to "depends on" in step rationale (language-agnostic)
- `core.py`: `analysis_meta["warning"]` now only included when non-None (less JSON noise)

#### Consistency & Correctness
- All regex-based analyzers now strip comments before `analyze_imports()`: Ruby (hash comments), C# (C-style), PHP (C + hash), Dart (C-style), Zig (C-style), Go (C-style in import extraction)
- `str.endswith()` now uses native tuple form throughout `_collect_symbols_regex`, `_collect_typed_symbols_regex`, and `_collect_project_files`
- Java `analyze_imports` also uses tuple form for extension check

#### Infrastructure
- Added module-load assertion `assert set(_TOOL_SCHEMAS) == set(_DISPATCH)` in mcp_server.py
- `mcp_stdio.py`: moved `logging.basicConfig()` before server construction
- `mcp_stdio.py`: fixed `call_tool` arguments type hint from bare `dict` to `dict[str, Any]`
- `store.py`: removed unreliable `__del__` finalizer — `__exit__` via context manager handles cleanup
- `store.py`: narrowed schema migration exception from `Exception` to `sqlite3.OperationalError`
- `store.py`: `log_event` now logs debug message on failure instead of silent swallow
- `store.py`: added `get_status_summary()` method
- `cli.py`: added `json.JSONDecodeError` handling for stdin input
- `__main__.py`: added `if __name__ == "__main__"` guard
- `_ANALYZER_REGISTRY` type annotation tightened from `dict[str, type]` to `dict[str, type[LanguageAnalyzer]]`
- `_LANG_EXTENSIONS` moved from function body to module level in analyzers.py

#### Performance
- `store_recipes.py` `list_recipes()`: batch-fetches recipe files in single query (N+1 fix)
- `store_recipes.py` `validate_recipes()`: removed redundant DELETE for invalidated recipes
- `strategies.py` `_order_test_adjacent()`: pre-computes basename set for O(1) lookup instead of O(steps × files)

### Test impact
- All 242 existing tests pass without modification (1 test updated to match new `explore_trajectories` signature)

- **2026-03-23:** [FIX] Release **3.4.1**. Analyzer gap fixes for 5 languages. **C++ nested templates:** Replaced `[^>]*` with `(?:[^<>]|<[^<>]*>)*` in template patterns (`_CPP_TYPED_PATTERNS` class detection, `_CPP_SYMBOL_PATTERNS` template function). Handles 2 levels of nesting (`template<typename T, vector<int>>`). Same fix applied to Rust `impl<>` and Java/Kotlin `fun<>` generic patterns. **Rust imports:** `analyze_imports` now handles `use super::` (parent module), `use self::` (current module), and workspace crate imports. New `_resolve_rust_mod` helper. New `_read_cargo_crates` reads `Cargo.toml` `[workspace] members`, extracts crate names (with hyphen→underscore conversion and explicit name lookup from member Cargo.toml), maps to `src/` dirs. Comment stripping now applied to import source before regex matching. **PHP grouped use:** New `_PHP_USE_GROUP_RE` pattern matches `use Prefix\{A, B, C};`. `analyze_imports` expands grouped uses by combining prefix with each item, then resolves all paths uniformly. **Swift SPM:** New `SwiftAnalyzer._build_module_map` detects `Sources/<Module>/` and `Tests/<Module>/` directories. For SPM projects, module names map to their directory's files. Falls back to parent-directory mapping for non-SPM. Comment stripping added to import source. **TypeScript workspaces:** New `_read_workspace_packages` in `utils.py` reads `package.json` workspaces (npm array, yarn `{packages:[]}`, simple `dir/*` glob expansion), discovers workspace packages by reading each `package.json` name. New `_resolve_workspace_import` resolves bare imports to workspace package entry points (`src/index.*` or `index.*`), handles scoped packages (`@scope/pkg/sub`). `TypeScriptAnalyzer.analyze_imports` now checks workspace packages for non-relative imports after alias resolution. All 242 tests pass (unchanged). All docs updated.
- **2026-03-23:** [FEATURE] Release **3.4.0**. Usage telemetry, dispatch refactor, analyzer improvements. **Telemetry:** New `usage_events` SQLite table (8 tables total) with `log_event(event_type, detail, value)` and `get_usage_stats(days=30)` methods on `RecipeStore`. All `dispatch_tool` calls logged as `tool_call` events. `retrieve_best_recipe` logs `recipe_hit` (with score) and `recipe_miss` events. `get_usage_stats` aggregates tool call counts, recipe hit/miss rates (with average hit score), and strategy win rates from trajectories. New `usage_stats` MCP tool (22 tools total). Telemetry uses fire-and-forget pattern with `try/except pass` to never break core functionality. **Dispatch refactor:** Replaced 153-line `match/case` statement in `mcp_server.py` with dispatch-dict pattern. 22 individual `_handle_*` functions at module level, looked up via `_DISPATCH: dict[str, Callable]`. Shared `_get_analyzer(args)` helper extracts analyzer creation from dispatch. `dispatch_tool` is now 6 lines: lookup handler, log event, call handler. Adding new tools requires only: handler function + schema entry + dict entry. **Analyzer improvements:** PHP class method detection added — new pattern `(?:(?:public|protected|private|static|abstract|final)\s+)*function\s+(\w+)` with type label "method" (was major gap). Java 16+ `record` keyword added to `_JAVA_TYPED_PATTERNS` with type label "record". Dart factory/named constructor detection added — pattern `(?:factory\s+)?(\w+\.\w+)\s*\(` with type label "constructor". **Comment stripping for 7 languages:** Added shared `_strip_c_comments` (Go, Rust, Java, C#, Swift, Dart, Zig), `_strip_hash_comments` (Ruby), `_strip_php_comments` (PHP) in `utils.py`. All regex-based analyzers now preprocess source through comment strippers before symbol detection, eliminating false matches from commented-out code. **5 new sample repos:** Jekyll (Ruby), Flame (Dart), ZLS (Zig), Laravel (PHP), Ktor (Kotlin) cloned to `sample_file_test/` — brings total to 26 sample repos covering all 15 supported languages. All 242 tests pass (unchanged). All docs updated.
- **2026-03-23:** [CLEANUP] Release **3.3.1**. Comprehensive codebase audit and module extraction. **Strategy extraction:** Beam strategy registry and 9 built-in orderings extracted from `core.py` into new `strategies.py` (~280 LOC). `core.py` reduced from 595 to 324 LOC. `__init__.py` and `mcp_server.py` imports updated. Test imports in `test_strategies.py` updated. **Circular dependency elimination:** Moved shared `_collect_symbols_regex` and `_collect_typed_symbols_regex` helper functions from `analyzers.py` to `utils.py`. Removed `functools.cache` lazy-import wrappers from `analyzers_ext.py` (12 lines) and `analyzers_ext2.py` (12 lines), replaced with direct `from .utils import` statements. Also removed `functools` and `Callable` imports (no longer needed in ext files). **Regex deduplication:** Derived `_*_SYMBOL_PATTERNS` from `_*_TYPED_PATTERNS` via `[p for p, _ in _*_TYPED_PATTERNS]` for 8 languages where patterns were identical: TypeScript, Rust, Java, C#, Ruby, PHP, Swift, Dart. Go and Zig kept separate (different pattern structures). C++ uses `[p for p, _ in _CPP_TYPED_PATTERNS] + [extra patterns]` for the additional C++-specific patterns. Net ~70 lines of duplicated regex definitions eliminated. **Python AST deduplication:** Extracted `PythonAnalyzer._iter_ast()` static generator method yielding `(rel_path, ast_tree)` tuples. Both `collect_symbols` and `collect_typed_symbols` now use it, eliminating ~20 lines of duplicated file-walking and AST-parsing code. `_PY_AST_TYPES` and `_PY_TYPE_MAP` promoted to module-level constants. **Import ordering:** Fixed stdlib import ordering in `store_recipes.py` (`import os` moved from after local imports to correct position between `import json` and `import time`). **All files now under 500 LOC:** `analyzers.py` 559→463, `core.py` 595→324, `analyzers_ext.py` 478→438, `analyzers_ext2.py` 490→445. **Total source:** 3,876→3,771 LOC (105 lines net reduction). All 242 tests pass (unchanged). All docs updated.
- **2026-03-23:** [FEATURE] Release **3.3.0**. Typed symbol analysis, 3 new beam strategies, analyzer bug fixes, store refactor. New `collect_typed_symbols()` method on all 15 language analyzers (`LanguageAnalyzer` protocol extended) returns `dict[str, list[tuple[str, str]]]` — file → [(symbol_name, type_label)] with type classification: Python (function/class via AST), TypeScript (function/class/interface/enum/type_alias/namespace), Go (function/struct/interface/type/constant/variable), Rust (function/struct/enum/trait/impl/type_alias), C++ (class/struct/namespace/enum/type_alias/function), Java (class/interface/enum/function/object/annotation), C# (class/interface/struct/enum/record/function), Ruby (class/module/function), PHP (class/interface/trait/enum/function), Swift (class/struct/enum/protocol/function/extension/type_alias/actor), Dart (class/mixin/extension/enum/type_alias/function), Zig (function/struct/enum/union/import_const/type_alias). New shared `_collect_typed_symbols_regex()` helper in `analyzers.py` with lazy import wrappers in `analyzers_ext.py`/`analyzers_ext2.py`. Three new beam strategies (9 total): `leaf_first` (files with zero importers first — safe, isolated changes), `hub_first` (files that both import many AND are imported by many first — network hubs with highest connectivity risk, scored by in*out degree product), `test_adjacent` (files with corresponding test files first — verifiable changes prioritized; matches test_X, X_test, X_spec patterns). **Bug fixes:** Ruby analyzer basename overwriting — separated into `stem_to_file` (full-path priority) and `basename_to_file` (fallback, `setdefault` prevents overwrites); Swift analyzer overly broad directory mapping — now maps only immediate parent directory instead of all ancestor directories; Java analyzer packageless file gap — files without `package` declarations now linked to directory siblings as co-dependents. **Refactoring:** `_init_schema()` in `store.py` decomposed into `_SCHEMA_RECIPE_TABLES` and `_SCHEMA_PLAN_TABLES` class-level SQL constants; Java `analyze_imports` now caches source content in `file_sources` dict (eliminates double file read). All 242 tests pass (12 new: 3 typed symbols [Python/TS/Go], 6 new strategies [2 per strategy], 1 Ruby basename overwrite, 1 Swift immediate parent, 1 Java packageless fallback). All docs updated.
- **2026-03-23:** [CLEANUP] Release **3.2.1**. Comprehensive codebase audit and cleanup. **Bug fix:** `retrieve_best_recipe` in `store_recipes.py` had a scoring bug where `best_score` was updated before JSON deserialization of the strategy string — if `json.loads` failed on a corrupted entry, the inflated `best_score` could shadow valid lower-scoring recipes; fixed by moving `best_score` assignment after successful parse. **Dead code removed:** unused `total_files` variable in `mcp_server.py` `estimate` tool, redundant `get_analyzer` re-import in `estimate` case. **Code simplification:** `detect_language` in `analyzers.py` — removed 8 redundant extension alias constants (`_CPP_EXTENSIONS`, `_JAVA_EXTENSIONS`, etc.) and replaced 12-branch if/elif chain with a data-driven `_LANG_EXTENSIONS` loop; replaced PEP 8–violating lambda in `_detect_from_config` with proper `def`. Added `OSError` guard to `os.listdir` call in C# config detection. Added missing `Callable` return type annotations to `_get_collect_symbols_regex()` in both `analyzers_ext.py` and `analyzers_ext2.py` (previously suppressed with `# noqa: ANN202`). **Performance:** eliminated double file reads in `CSharpAnalyzer.analyze_imports` and `PhpAnalyzer.analyze_imports` (single walk caches source content for reuse in dependency phase); optimized `GoAnalyzer.analyze_imports` from two `os.walk` passes to one; optimized PHP namespace lookup from O(n) linear scan to O(1) dict lookup via reverse index. Removed redundant `DartAnalyzer.pick_test_cmd` branch (both paths returned identical command). Simplified `utils.py` fallback error detection: `"Error" in line or "error" in line.lower()` → `"error" in line.lower()`. Moved `_SUPPORTED` set from inside `Planner.decompose()` to module-level `_SUPPORTED_LANGUAGES` frozenset in `core.py`. Simplified `__main__.py` by removing unnecessary `if __name__ == "__main__"` guard. All 230 tests pass (unchanged). All docs updated.
- **2026-03-23:** [FEATURE] Release **3.2.0**. Six new language analyzers, expanding Trammel to 15 supported languages. New `analyzers_ext2.py` (~480 LOC) contains `CSharpAnalyzer` (regex-based, `.cs`, class/interface/struct/enum/record/delegate symbols, `using` import resolution), `RubyAnalyzer` (regex-based, `.rb`, class/module/def symbols, `require`/`require_relative` import resolution), `PhpAnalyzer` (regex-based, `.php`, class/interface/trait/enum/function symbols, `use`/`require`/`include` import resolution), `SwiftAnalyzer` (regex-based, `.swift`, class/struct/enum/protocol/func/actor symbols, `import` resolution), `DartAnalyzer` (regex-based, `.dart`, class/mixin/extension/enum/typedef symbols, `import`/`part` resolution), `ZigAnalyzer` (regex-based, `.zig`, pub fn/const/struct/enum/union symbols, `@import` resolution). Config-file detection in `analyzers.py` expanded: Package.swift (swift), build.zig (zig), pubspec.yaml (dart), .csproj/.sln (csharp), Gemfile (ruby), composer.json (php). Extension counting in `detect_language` expanded: .cs, .rb, .php, .swift, .dart, .zig all counted in fallback. `_ANALYZER_REGISTRY` expanded to 15 languages: python, typescript, javascript, go, rust, cpp, c, java, kotlin, csharp, ruby, php, swift, dart, zig. MCP `_LANGUAGES` list in `mcp_server.py` updated to match. All 6 new analyzers (`CSharpAnalyzer`, `DartAnalyzer`, `PhpAnalyzer`, `RubyAnalyzer`, `SwiftAnalyzer`, `ZigAnalyzer`) exported from `__init__.py`. All 230 tests pass (15 new: symbols/imports for C#, Ruby, PHP, Swift, Dart, Zig + 6 config detection tests). All 21 sample repos re-tested: zero errors across all languages and scopes. All docs updated.
- **2026-03-23:** [FEATURE] Release **3.1.0**. Abbreviation handling in recipe matching, analysis timing metadata, `estimate` MCP tool, iterative critical_path. New `_ABBREVIATIONS` dict in `utils.py` with ~40 common coding abbreviations (gc→garbage collector, db→database, auth→authentication, api→application programming interface, etc.). `normalize_goal` in `utils.py` now expands abbreviations before applying verb synonyms, enabling recipe matching across abbreviated goals (e.g., "optimize GC" vs "optimize garbage collector" now matches at 0.86+ similarity). `Planner.decompose` in `core.py` now returns `analysis_meta` in the response with `language`, `scope`, `files_analyzed`, `dep_files`, `dep_edges`, `timing_s` (symbols, imports, total), and optional `warning` when the detected language analyzer may not be native (e.g., C# falling back to C++). New `estimate` MCP tool in `mcp_server.py` (21 tools total) for quick file count without full analysis — returns `language`, `matching_files`, `recommendation` ("use scope" if >5000 files, "full analysis OK" otherwise); helps LLMs decide whether to scope before analyzing large repos. Converted recursive `_longest` depth computation in `critical_path` strategy (`core.py`) to iterative stack-based DFS with `in_stack` cycle detection — fixes stack overflow on deep dependency graphs (Guava's 1.66M-edge Java import graph was crashing). All 215 tests pass (5 new: 3 abbreviation, 1 analysis meta, 1 estimate tool). All docs updated.
- **2026-03-23:** [MAJOR] Release **3.0.0**. Monorepo scope support, concurrent write safety validation. New `scope` parameter on `Planner.decompose` in `core.py` — when provided, analysis is scoped to `os.path.join(project_root, scope)` while the full project remains available for test execution. `scope` propagated through `plan_and_execute` and `explore` in `__init__.py`, `--scope` CLI flag in `cli.py`, and `decompose`/`explore` MCP tool schemas in `mcp_server.py`. Concurrent write safety validated with 3 threading tests: 4 threads × 5 operations each on plans, recipes, and constraints — all operations complete without errors and produce correct counts. `sample_file_test/` added to `.gitignore` (large monorepo clone testing) and `_IGNORED_DIRS` in `utils.py`. All 206 tests pass (6 new: 3 concurrent write safety, 3 monorepo scope). All docs updated.
- **2026-03-23:** [FEATURE] Release **2.9.0**. Plan resumption, recipe validation, config-file language detection, shared file-collection helper, `verify_step` language support, full MCP dispatch test coverage. New `get_plan_progress(plan_id)` in `store.py` returns accumulated `prior_edits` from passed steps, `remaining_steps`, and `next_step_index` for plan resumption. New `validate_recipes(project_root)` in `store_recipes.py` checks recipe `recipe_files` entries against current project state — removes stale file entries and cascade-prunes recipes whose files are entirely missing. New `_detect_from_config(project_root)` in `analyzers.py` detects language from project config files (Cargo.toml → rust, go.mod → go, tsconfig.json/package.json → typescript, build.gradle/pom.xml → java, CMakeLists.txt → cpp, pyproject.toml/setup.py → python) before falling back to extension counting; config detection takes priority over file counts. New `_collect_project_files(root, extensions)` shared helper in `utils.py` replaces duplicated file-walk patterns in `TypeScriptAnalyzer._collect_files`, `CppAnalyzer._collect_files`, and `RustAnalyzer.analyze_imports`. `explore_trajectories` in `core.py` now uses existing `_split_active_skipped` helper instead of reimplementing the pattern inline. `verify_step` MCP tool gains `language` parameter for auto-detecting test command and error patterns (was previously language-agnostic). New `resume` and `validate_recipes` MCP tools (20 tools total). All 200 tests pass (17 new: 8 MCP dispatch coverage, 5 plan resumption, 8 config detection, 4 recipe validation, minus 8 overlapping). All docs updated.
- **2026-03-23:** [CLEANUP] Release **2.8.0**. Codebase audit and cleanup. Removed unused imports: `Any` from `analyzers.py`, `json` from `analyzers_ext.py`, `ExecutionHarness` and `dumps_json` from `test_strategies.py`. Replaced fragile lazy-import global (`_collect_symbols_regex = None` + `global` mutation) in `analyzers_ext.py` with `functools.cache` decorator — thread-safe, no type suppression needed, simpler code. Fixed overly broad exception handling in `utils.py` transaction helper: `except BaseException` narrowed to `except Exception` (no longer catches `KeyboardInterrupt`/`SystemExit`). Modernized transaction commit/rollback to use `conn.commit()`/`conn.rollback()` instead of raw `conn.execute("COMMIT")`/`conn.execute("ROLLBACK")`. Optimized `_order_cohesion` in `core.py` by pre-computing `comp_set = set(comp)` outside dict comprehension (was recreating set on each iteration). Updated `SYSTEM_PROMPT.md`: corrected tool count from 17 to 18 (added missing `prune_recipes` entry), added C/C++ and Java/Kotlin to multi-language support section. All 175 tests pass. All docs updated.
- **2026-03-23:** [FEATURE] Release **2.7.0**. Store split, expanded analyzers, MCP prune tool. Extracted recipe methods (`save_recipe`, `retrieve_best_recipe`, `list_recipes`, `prune_recipes`, `_rebuild_trigram_index`, `_backfill_files`) from `store.py` into `store_recipes.py` as `RecipeStoreMixin` (~210 LOC); `RecipeStore` in `store.py` now inherits from it (~342 LOC, down from 540). `CppAnalyzer` in `analyzers_ext.py` expanded from single function pattern to 5 targeted patterns: template functions, qualified functions (static/inline/constexpr), operator overloading, constructor/destructor detection, macro-prefixed functions (EXPORT_API etc). New `JavaAnalyzer._detect_source_roots(project_root)` in `analyzers_ext.py` reads `build.gradle`/`build.gradle.kts` and `pom.xml` to find standard source directories (`src/main/java`, `src/main/kotlin`, etc); falls back to project root; `analyze_imports` now walks detected source roots instead of project root. Exposed recipe pruning as MCP tool `prune_recipes` with `max_age_days` and `min_success_ratio` parameters in `mcp_server.py` (18 tools total). All 175 tests pass (9 new: 5 C++ expansion, 3 Java source roots, 2 MCP prune, minus 1 renamed). All docs updated.
- **2026-03-23:** [FEATURE] Release **2.6.0**. Parallel beams, new analyzers, module split, recipe pruning, harness caching, CLI flags, constraint decomposition. `plan_and_execute` in `__init__.py` now runs beams concurrently via `concurrent.futures.ProcessPoolExecutor` (stdlib); new `_run_beam` and `_run_beams_parallel` helper functions; falls back to sequential on systems where process spawning fails. New `CppAnalyzer` in `analyzers_ext.py` — regex-based, handles `.c/.cpp/.cc/.cxx/.h/.hpp/.hxx`; symbol detection for class, struct, namespace, enum, typedef, function; `#include "..."` import resolution with comment stripping; registered as "cpp" and "c". New `JavaAnalyzer` in `analyzers_ext.py` — regex-based, handles `.java/.kt/.kts`; symbol detection for class, interface, enum, fun, object, @interface; package-based import resolution; registered as "java" and "kotlin". Analyzer module split: `analyzers.py` split into `analyzers.py` (protocol, PythonAnalyzer, TypeScriptAnalyzer, registry, detection, ~370 LOC) + `analyzers_ext.py` (GoAnalyzer, RustAnalyzer, CppAnalyzer, JavaAnalyzer, ~400 LOC) to stay under 500 LOC per file; all existing imports preserved via re-export from `analyzers.py`. New `RecipeStore.prune_recipes(max_age_days=90, min_success_ratio=0.1)` in `store.py` — removes stale, low-quality recipes and cascades deletes to `recipe_trigrams` and `recipe_files`. New `ExecutionHarness.prepare_base(project_root)` and `run_from_base(edits, base_dir)` in `harness.py` — creates one filtered base copy; beams copy from it instead of re-filtering ignored dirs per beam. New `--dry-run` CLI flag in `cli.py` runs `explore()` instead of `plan_and_execute()`; new `--language` flag also added. `_apply_constraints` (85-line function) in `core.py` decomposed into `_parse_constraints`, `_mark_avoided`, `_inject_orderings`, `_mark_incompatible`, `_add_prerequisites`. MCP language enum in `mcp_server.py` expanded to 9 entries: python, typescript, javascript, go, rust, cpp, c, java, kotlin. `CppAnalyzer` and `JavaAnalyzer` exported from `__init__`. All 166 tests pass (20 new). All docs updated.
- **2026-03-23:** [CLEANUP] Release **2.5.0**. Full codebase audit: removed unused `import sys` from `harness.py` (dead import). Eliminated duplicate `set(symbols) | set(dep_graph)` computation in `Planner.decompose` in `core.py` — `context_files` and `all_files` were identical values computed 5 lines apart; consolidated to single `all_files` variable. Added defensive `json.loads` error handling in `retrieve_best_recipe` in `store.py` — unguarded parse at DB boundary could crash on corrupted strategy strings. Made failure default explicit in `get_strategy_stats` in `store.py` — `except` clause now explicitly sets `success = False` instead of `pass` (functionally identical but clearer intent). Fixed `register_strategy` parameter order in `spec-project.md` documentation (`(name, description, fn)` not `(name, fn, description)` — was missed in the 2.3.0 doc fix that corrected the glossary). All 146 tests pass. All docs updated.
- **2026-03-23:** [FEATURE] Release **2.4.0**. Two new language analyzers + scoring improvements. New `GoAnalyzer` in `analyzers.py` — regex-based, reads `go.mod` for module path, resolves internal imports. New `RustAnalyzer` — regex-based, resolves `use crate::` and `mod` declarations. Shared `_collect_symbols_regex` helper extracted for regex-based analyzers. `_strip_js_comments` added for TypeScript comment stripping before symbol/import detection. Namespace pattern added to `_TS_SYMBOL_PATTERNS`. `detect_language` expanded to count `.go`/`.rs` files; registry now supports 5 languages. `_order_bottom_up` in `core.py` now stable-sorts by ascending dependency count (files with fewer deps first); `_order_top_down` by descending dependency count (most consumer-facing files first); both now genuinely use the `dep_graph` parameter. New `word_substring_score(a, b)` in `utils.py` for partial word matching; `goal_similarity` reweighted to 0.3 trigram cosine + 0.4 word Jaccard + 0.3 substring (was 0.4/0.6). `store.py` merged duplicated `save_recipe` SQL branches into single parameterized query; merged `list_plans` and `get_active_constraints` query branches; simplified `get_strategy_stats`; composite scoring gains recency weighting (30-day half-life); new weights: text 0.4, files 0.25, success 0.15, recency 0.2; trimmed from 534 to 516 lines. `mcp_server.py` refactored with `_schema()` and `_prop()` helpers, reducing from 507 to 255 lines; added "go" and "rust" to language enums. `GoAnalyzer` and `RustAnalyzer` exported from `__init__`. All 146 tests pass (17 new: 4 Go, 3 Rust, 2 TS enhancement, 2 detection, 2 strategy, 4 matching). All docs updated.
- **2026-03-23:** [CLEANUP] Release **2.3.0**. Full codebase audit: no dead code, no TODOs, no truncated code, no unused imports, all linkages correct. Extracted `_split_active_skipped(steps)` helper in `core.py` — shared by all 6 beam strategy functions, eliminating the duplicated 2-line active/skipped split pattern. Modernized `_VERB_SYNONYMS` in `utils.py` from imperative for-loop to dict comprehension; eliminates leaked module-level variables `_canonical`, `_variants`, `_v`. Fixed documentation errors: corrected canonical verb form examples in glossary and spec (was "refactor", now correctly "restructure"); fixed `register_strategy` parameter order in glossary (`(name, description, fn)` not `(name, fn, description)`). All 129 tests pass. All docs updated.
- **2026-03-23:** [FEATURE] Release **2.2.0**. Three features. (1) Improved recipe matching: new `_VERB_SYNONYMS` dict in `utils.py` mapping 40+ verb variants to 9 canonical forms; `normalize_goal(text)` for lowercase + verb synonym replacement; `word_jaccard(a, b)` for word-level Jaccard similarity; `goal_similarity(a, b)` blending 0.4 trigram cosine + 0.6 word Jaccard on normalized text. `save_recipe` normalizes patterns before trigram indexing. `retrieve_best_recipe` uses `goal_similarity` for scoring and normalizes goals for trigram index queries. `_backfill_trigrams` renamed to `_rebuild_trigram_index`, now rebuilds all trigrams with normalized text on init. 10 new recipe matching tests. (2) New beam strategies (6 total, 3 new): `critical_path` — longest dependency chain first (bottleneck feedback, recursive depth computation); `cohesion` — flood-fill connected components, process tightly coupled groups contiguously, largest first, topological sort within each component; `minimal_change` — fewest symbols first (quick wins, catch trivial failures early). 9 new strategy tests. (3) TypeScript analyzer improvements: replaced single `_TS_SYMBOL_RE` with `_TS_SYMBOL_PATTERNS` list covering interface, enum, const enum, type alias, abstract class, decorated class, function expression; expanded `_TS_IMPORT_RE` to cover re-exports (`export { } from`), barrel exports (`export * from`), type re-exports (`export type { } from`), dynamic imports (`import()`); new `_TS_ALIAS_IMPORT_RE` for non-relative alias imports; `_read_ts_path_aliases(root)` reads tsconfig.json `compilerOptions.paths` + `baseUrl`; `_resolve_alias()` method for alias-based resolution; added `.mts`/`.mjs` to `_TS_EXTENSIONS`. 15 new TypeScript tests. All 129 tests pass. All docs updated.
- **2026-03-23:** [CLEANUP] Release **2.1.0**. Removed dead `json` import from `core.py`. Eliminated duplicated Python error patterns: `PythonAnalyzer.error_patterns()` now returns shared `_ERROR_PATTERNS` from `utils.py` instead of defining its own identical copy. Removed duplicated `_pick_test_cmd` from `harness.py`; `_run_tests` now falls back to `PythonAnalyzer().pick_test_cmd()` when no test command is provided. Removed `analyze_imports` backward-compat wrapper from `utils.py` (not exported, only used by internal tests); callers use `PythonAnalyzer().analyze_imports()` directly. Removed obsolete `TestBackwardCompat` test from `test_analyzers.py`. All 95 tests pass. All docs updated.
- **2026-03-23:** [MAJOR] Release **2.0.0**. Reference LLM integration. Added `SYSTEM_PROMPT.md` — a reference orchestration guide for LLM clients describing the plan-verify-store loop. New MCP tools: `update_plan_status` (exposes existing store method for plan lifecycle management), `deactivate_constraint` (exposes existing store method for constraint lifecycle management). `status` tool now includes `tools` count in response. 17 MCP tools total. 4 new tests; all 96 tests pass. All docs updated.
- **2026-03-23:** [FEATURE] Release **1.9.0**. Structural recipe matching. New `recipe_files` table in SQLite schema (7 tables total) with indexes on both `file_path` and `recipe_sig` columns. `save_recipe` now populates `recipe_files` with file paths extracted from strategy steps. `_backfill_files()` auto-migrates existing databases. `retrieve_best_recipe` accepts optional `context_files` for composite scoring — weights: text similarity 0.5, file overlap via Jaccard 0.3, success ratio 0.2. Without `context_files`, scoring is backward-compatible (text-only). `Planner.decompose` passes project file context to recipe retrieval (two-phase: text-only fast path, then structural). New `list_recipes(limit=20)` method on `RecipeStore`. `get_recipe` MCP tool gains `context_files` parameter. New `list_recipes` MCP tool (16 tools total). 9 new recipe tests; all 92 tests pass. All docs updated.
- **2026-03-23:** [FEATURE] Release **1.8.0**. Multi-language support. New `trammel/analyzers.py` with `LanguageAnalyzer` protocol, `PythonAnalyzer` (AST-based symbol collection and import analysis), `TypeScriptAnalyzer` (regex-based, stdlib-only), `detect_language()`, and `get_analyzer()`. `Planner` accepts optional `analyzer` parameter and auto-detects language. `ExecutionHarness` accepts optional `analyzer` for language-specific test commands and error patterns. `_collect_python_symbols` removed from `core.py` (moved to `PythonAnalyzer`). `analyze_imports` in `utils.py` now a backward-compat wrapper delegating to `PythonAnalyzer`. `ast` import removed from `utils.py` (moved to `analyzers.py`). `analyze_failure` accepts optional `error_patterns` parameter. `_IGNORED_DIRS` expanded with `.next`, `.nuxt`, `coverage`, `.turbo`, `.parcel-cache`. `language` parameter added to `plan_and_execute`, `explore`, and MCP `decompose`/`explore` tools. `PythonAnalyzer`, `TypeScriptAnalyzer`, `detect_language` exported from `trammel.__init__`. New `tests/test_analyzers.py` with 13 tests (Python, TypeScript, detection, backward compat). All 83 tests pass. All docs updated.
- **2026-03-23:** [FEATURE] Release **1.7.0**. Pluggable strategy registry: added `register_strategy()`, `get_strategies()`, `StrategyFn` type alias, and `StrategyEntry` dataclass in `core.py`. Three built-in strategies (`bottom_up`, `top_down`, `risk_first`) auto-registered at module load with unified signature `(steps, dep_graph) -> steps`. Strategy learning: `explore_trajectories` accepts optional `store` parameter; when provided, strategies sorted by historical success rate from trajectory data. `plan_and_execute` and `explore` now pass store to `explore_trajectories`. Added `get_strategy_stats()` to `RecipeStore` (aggregates trajectory outcomes by variant). New `list_strategies` MCP tool returns strategy names with success/failure stats (14 tools total). Exported `register_strategy` and `get_strategies` from `trammel.__init__`. New `tests/test_strategies.py` with 8 tests; moved `TestBeamStrategies` from `test_trammel_extra.py`. All 70 tests pass. All docs updated.
- **2026-03-23:** [CLEANUP] Release **1.6.0**. Simplified `_collect_python_symbols` in `core.py` to return `dict[str, list[str]]` (symbol name strings) instead of `dict[str, list[dict]]` — the `file`, `type`, `line` fields in each symbol dict were collected but never consumed downstream (only `name` was used by `_generate_steps`). Removed unused `filepath` parameter from `_step_rationale`. Removed `_BEAM_STRATEGIES` module-level constant; descriptions inlined directly in `explore_trajectories`, eliminating fragile index-based coupling. Fixed duplicated extension point line in `spec-project.md`. Full codebase audit: no TODOs, no truncated code, no dead imports, all libraries/functions properly linked. All 62 tests pass. All docs updated.
- **2026-03-23:** [MAJOR] Release **1.5.0**. Five features for multi-agent production use: (1) `RecipeStore` context manager — `close()`, `__enter__`/`__exit__`, `__del__` safety net; all public API and MCP server use `with` blocks. (2) Concurrent write protection — `transaction()` context manager with `BEGIN IMMEDIATE`, exponential backoff retry on `SQLITE_BUSY`, `ROLLBACK` on error; all mutating store methods wrapped. `db_connect` sets `timeout=5.0`. (3) Inverted trigram index — new `recipe_trigrams` table with B-tree index; `retrieve_best_recipe` queries candidates by shared trigrams before exact cosine; `_backfill_trigrams` auto-migrates existing databases. (4) Constraint propagation — new `_apply_constraints` enforces `avoid` (skip file), `dependency` (inject ordering), `incompatible` (conflict metadata), `requires` (add prerequisite step); strategy output includes `constraints_applied`. (5) Constraint-aware beam strategies — `_order_bottom_up`/`_order_top_down` place skipped steps at end; `_order_risk_first` isolates incompatible steps, batches by package, sorts by coupling; `explore_trajectories` excludes skipped from edits. Schema expanded to 6 tables. All 62 tests pass. All docs updated.
- **2026-03-23:** [CLEANUP] Release **1.4.0**. Converted `mcp_stdio.py` from absolute to relative imports for consistency with the rest of the package. Removed unreachable `UnicodeDecodeError` from `_collect_python_symbols` except clause in `core.py` — files are opened with `errors="replace"`, so the exception can never be raised. Added `.chisel` to `_IGNORED_DIRS` in `utils.py` (tool cache directory, same category as `.mypy_cache`/`.ruff_cache`). Full codebase audit: no TODOs, no truncated code, no duplicated code, no dead imports, all 45 tests pass. All docs updated.
- **2026-03-23:** [CLEANUP] Release **1.3.0**. Removed unused test imports (`explore`, `synthesize`, `analyze_imports`, `cosine`, `trigram_bag_cosine`, `trigram_signature` from `test_trammel_extra.py`). Removed dead `goal_slice` parameter from `_collect_python_symbols` in `core.py` — was computed per symbol but never consumed by `_generate_steps` or any downstream code. Simplified `topological_sort` in `utils.py`: replaced redundant `rev.setdefault(t, [])` with direct `rev[t]` access since all keys are pre-initialized. Fixed `plan_and_execute` API signature in `spec-project.md` to include `test_cmd` parameter. All 45 tests pass. All docs updated.
- **2026-03-23:** [IMPROVE] Release **1.2.0**. Eliminated version duplication: `__version__` now derived from `importlib.metadata.version()` at runtime (falls back to `"dev"` when not installed). Converted `dispatch_tool` 13-branch if/elif chain to Python 3.10+ `match/case`. Made test command configurable: `ExecutionHarness` accepts `test_cmd` parameter, propagated through `plan_and_execute`, CLI (`--test-cmd`), and MCP `verify_step` schema. `retrieve_best_recipe` short-circuits on exact match (similarity 1.0). Added `tests/__init__.py`. All 45 tests pass. All docs updated.
- **2026-03-23:** [CLEANUP] Release **1.1.0**. Removed dead code: unused `advance_plan_step` method from `RecipeStore`, unused `json`/`os` imports from `mcp_server.py`, unused `json` import from `test_trammel.py`. Consolidated duplicated ignored-dirs logic: `harness.py` hardcoded skip list replaced with shared `_is_ignored_dir` helper from `utils.py`; fixed `egg-info` frozenset entry that could never match actual `*.egg-info` directories (now uses suffix check). Modernized `topological_sort` to use `collections.deque` instead of `list.pop(0)` for O(1) queue operations. Simplified `Planner.decompose`: replaced verbose loop building `all_files` with set union. All 45 tests pass. All docs updated.
- **2026-03-22:** [MAJOR] Release **1.0.0**. Full rework from scaffolding harness to cognitive planning tool. `Planner.decompose` now analyzes project imports via AST, builds dependency graph, applies topological sort, generates steps with ordering rationale and dependency tracking. `explore_trajectories` produces genuinely different beam strategies: `bottom_up` (dependencies first), `top_down` (API surface first), `risk_first` (highest coupling first). `ExecutionHarness` gains `verify_step` (single-step isolation) and `run_incremental` (step-by-step verification that aborts on first failure). Structured `analyze_failure` extracts error type, message, file, line, and suggestion from test output. New `constraints` table enables failure propagation across sessions — constraints are checked during decomposition and prevent repetition of known-bad approaches. Schema expanded from 3 to 5 tables (added `steps` and `constraints`). `recipes` now store strategy patterns with constraints and failure counts. MCP server (`trammel-mcp`) exposes 13 tools via stdio transport, matching Stele/Chisel pattern. `mcp>=1.0.0` as optional dependency. Full suite **45** tests. All docs updated.
- **2026-03-22:** [CLEANUP] Release **0.3.0**. Added min similarity threshold (0.3) to `retrieve_best_recipe` to prevent unrelated recipe matches. `_collect_python_symbols` now handles `ast.AsyncFunctionDef` and filters ignored directories (`.git`, `venv`, `node_modules`, etc.) during `os.walk`. Deduplicated trigram computation: `trigram_signature` reuses `_trigram_list`; replaced `defaultdict` with `Counter`; removed unused `defaultdict` import. Removed dead code: unnecessary lambda wrapper in `harness.py`, unreachable `if __name__` guard in `cli.py`. All docs updated. Full suite **24** tests.
- **2026-03-22:** [MAINT] Release **0.2.0**. Replaced misleading positional trigram "cosine" with `trigram_bag_cosine` (union vocabulary). `RecipeStore.retrieve_best_recipe` tie-breaks on `successes`. `db_connect` enables `PRAGMA foreign_keys=ON`. Harness uses `sys.executable` instead of bare `python`. Beam edits now include `path` aligned with harness (`content` still required for writes). Added `dumps_json`, `__version__`, CLI `--version`. Documentation: `README.md`, `COMPLETE_PROJECT_DOCUMENTATION.md`, this file. Tests: +3 (trigram bag, `save_recipe(False)`, tie-break). Full suite **24** tests. Indexed docs via Stele (`user-stele-context`).
- **2026-03-22:** [DOCS] Added **`wiki-local/`** (`index.md`, `spec-project.md`, `glossary.md`) per project doc convention; updated `README.md` and `COMPLETE_PROJECT_DOCUMENTATION.md`. Re-indexed wiki pages in Stele.
