# Trammel glossary

Terms for the 4.0.1 binary. Python-only names (the importable package, `trammel.db` as the live store, `record_step`, `create_plan`) are on the `python-legacy` branch.

| Term | Definition |
|------|------------|
| **Beam** | One ordering of the same steps. Nine names: bottom_up, top_down, risk_first, critical_path, cohesion, minimal_change, leaf_first, hub_first, test_adjacent. |
| **Strategy** | The plan payload: steps, dependency graph, constraints, goal fingerprint. |
| **Step** | One file to change, with symbols, description, rationale, and `depends_on`. |
| **Dependency graph** | File to the files it depends on, from imports plus implicit edges. |
| **Implicit edge** | A coupling that is not an import. Rules are `data/patterns.json`. Higher confidence sorts first. |
| **Harness** | Temp copy of the project, edits applied, preflight, then the test command. |
| **Recipe** | A stored strategy for a goal, with success and failure counts. |
| **Plan** | A persisted strategy plus step status. Status is pending, running, completed, or failed. |
| **Constraint** | A recorded failure. Types: avoid, dependency, incompatible, requires. |
| **Trajectory** | One beam run: outcome, steps completed, failure reason. |
| **Primary tools** | The 12 tools listed by default. `TRAMMEL_MCP_SURFACE=all` lists all 32. |
| **tool_removed** | The error code for `record_step` and `create_plan`. Replacements are `record_steps` and `start_plan`. |
| **Store** | `.trammel/store` snapshot plus `journal` and `lock`. |
| **Migrate** | Read-only import of an old SQLite `trammel.db` into the store. |
| **Claim** | An agent holds a step. Claims expire after 10 minutes. |
| **Export** | `export_plan` writes `format=trammel.plan`, `format_version` 1. |
| **Doctor** | Local report: version, store path, languages, protocols, next step. No network. |
| **Floor** | A recorded Python median in `scripts/bench_gate.py`. A Rust median at or above the floor fails CI. |
