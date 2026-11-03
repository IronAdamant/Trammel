# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

Trammel is a dependency-aware task planner for LLM coding assistants: it decomposes goals into ordered steps, explores strategy variants (beam search), verifies steps, learns constraints from failures, and stores successful strategies as reusable recipes. It ships as a Python library, a CLI, and an MCP server (33 tools; ~12 primary advertised by default via `TRAMMEL_MCP_SURFACE`).

Note: the global `~/CLAUDE.md` describes how to *use* the Trammel MCP server as a client. This repo is Trammel's *source* — don't confuse the two when "trammel" tools appear in your MCP tool list.

**Hard constraint: the core is stdlib-only.** Do not add third-party dependencies. The only dependency is the optional `mcp` package (extra: `trammel[mcp]`) used by the MCP server entry point.

## Commands

```bash
pip install -e '.[mcp]'                                     # dev install

# Tests (unittest, same command CI runs on Python 3.10–3.13)
python -m unittest discover -v -s tests -p 'test_*.py'      # full suite
python -m unittest tests.test_strategies -v                 # one module
python -m unittest tests.test_trammel.SomeCase.test_name    # one test
pytest                                                      # also configured (pyproject excludes tests/archetypes)

# Run
python -m trammel "some goal" --root /path/to/project --dry-run   # CLI (explore only)
trammel-mcp                                                       # MCP server on stdio; TRAMMEL_DB_PATH overrides db location
```

Releases: bump `version` in `pyproject.toml` (see `scripts/release.sh`); publishing to PyPI happens via GitHub release → `publish.yml` (OIDC trusted publishing, no tokens).

## Architecture

All state lives in one SQLite database (`DEFAULT_DB_PATH = "trammel.db"`, relative to cwd; `*.db` is gitignored). Layers, top to bottom:

**MCP layer** — `mcp_stdio.py` (entry point) → `dispatch_tool()` in `mcp_server.py` → per-tool `_handle_*` functions. Tool JSON schemas live in `tool_schemas.py`. The two registries (`_TOOL_SCHEMAS`, `_DISPATCH`) are diffed at module import and a mismatch raises `RuntimeError` — **to add an MCP tool you must touch both files**, then add a smoke test in `tests/test_mcp_dispatch.py` (which also asserts parity). `coerce_int_params()` casts string→int per schema because MCP clients may stringify numbers.

**Planning core** — `core.py` (`Planner`: decompose → recipe lookup → step generation → constraint application), `strategies.py` (`_STRATEGY_REGISTRY` of step orderings: bottom-up, top-down, risk-first, critical-path, etc.), `planner_helpers.py` (beam variants), `goal_nlp.py` (goal parsing/intent), `scoring.py` (step relevance), `constraints.py` (avoid/dependency/incompatible/requires rules, mutate steps in place).

**Store layer** — `store.py` composes mixins into `RecipeStore` (context manager): `store_plans.py`, `store_recipes.py`, `store_retrieval.py` (similarity lookup), `store_scaffolds.py`, `store_agents.py` (multi-agent step claiming, 10-min claim expiry), `store_telemetry.py`. Schema "migrations" are additive `CREATE TABLE IF NOT EXISTS` in `_init_schema()` — there is no migration tool or schema versioning, so schema changes must be backward-compatible additions.

**Language analysis** — `analyzer_engine.py` (generic regex engine) driven by declarative per-language specs in `analyzer_specs.py` (`SPEC_REGISTRY`, 12 entries; C shares the C++ spec and Kotlin shares the Java spec), wrapped by `analyzers.py` (15 languages total: those 12 plus AST Python and regex TypeScript/JavaScript); import resolution per ecosystem in `analyzer_resolvers.py`. The language list in `tool_schemas.py` (`_LANGUAGES`) is validated against the analyzer registry at import — adding a language means updating spec, registry, and `_LANGUAGES` together. The store schema is 15 SQLite tables.

**Recipes & inference** — `recipe_fingerprints.py` + `recipe_index.py` (inverted index + MinHash LSH, stdlib-only) for retrieval; `implicit_deps*.py` + `pattern_learner.py` infer non-import couplings (naming conventions, shared state); heuristics/weights are externalized to `trammel/data/patterns.json`, loaded by `pattern_config.py`.

**Execution** — `harness.py` (`ExecutionHarness`) runs real tests in isolated temp copies. The MCP `verify_step` tool uses the same harness: it returns static analysis, preflight syntax checks, and import-integrity results plus a real isolated test run (slow per-step; manual test runs are often faster in single-agent workflows). The full incremental multi-step verification loop (`run_incremental`) is only driven via the Python API (`plan_and_execute`).

`scaffold_*.py` handle greenfield planning (files that don't exist yet): decompose can't infer new files without explicit scaffold entries, which is why scaffold is mandatory for new-feature plans.

## Conventions

- **Max ~500 LOC per file** — this is why store/analyzer/scaffold logic is split into mixin/spec modules. Split rather than grow.
- **`.gitignore` excludes `*.md`** — tracked docs (README, SYSTEM_PROMPT, wiki-local/, etc.) were force-added. New docs that should be committed need `git add -f`.
- On code changes, update `COMPLETE_PROJECT_DOCUMENTATION.md` (file table) and the relevant `wiki-local/` pages; `LLM_Development.md` is the chronological dev log.
- `tests/archetypes/` contains sample projects for regression tests, not tests — it's excluded from discovery in `pyproject.toml`.
- `SYSTEM_PROMPT.md` is the orchestration guide shipped to MCP users; update it when tool behavior or recommended workflows change.
- **Wikifier doc health is active here**: `monitored_paths.txt` covers `trammel/` plus README/SYSTEM_PROMPT/CLAUDE.md, with per-file wiki entries in `docs/wiki/<basename>.wiki.md`. After editing a monitored file: `wikifier record-change "<file>" "<reason>"`, update its wiki entry, then `wikifier mark-green "<file>" "<evidence>"`; run `wikifier update-maps` when imports change. The `/wikifier` skill (or the `wikifier` MCP server) has the full protocol. `file_health.md`, `pending_updates.md`, and `library.md` are managed artifacts — never hand-edit.
