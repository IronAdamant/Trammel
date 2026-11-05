# Migrating from the Python version

Trammel 3.15.3 is the last Python release. The package is `trammel` on PyPI (`pip install trammel`, and `pip install trammel[mcp]` for the stdio server). Version 4.0.1 is a std-only Rust binary. The commands are still `trammel` and `trammel-mcp`.

The Python package, its unittest suite, `pyproject.toml`, and the PyPI workflow stay on the `python-legacy` branch. That branch is commit `dc7deb335e91e5a29541733a234dfc1ed24788a3`, the tip of `main` before pull request #7 removed the package. Nothing on `python-legacy` was rewritten for this note.

4.x does not publish to PyPI. Install it from npm or from a GitHub Release:

```bash
npm i -g @iron_adamant/trammel
npx -y @iron_adamant/trammel mcp
```

`npx` runs the `trammel` command. The positional argument `mcp` starts the same stdio server as `trammel-mcp`. A global install puts both `trammel` and `trammel-mcp` on `PATH`.

The npm name `trammel` belongs to another account. The packages are `@iron_adamant/trammel` and `@iron_adamant/trammel-<os>-<arch>` (pull request #4).

From a GitHub Release, without npm:

```bash
sh install.sh
```

`install.sh` supports Linux x86_64 and aarch64, and macOS x86_64 and arm64. It copies the binaries into `~/.local/bin`. `install.ps1` installs the Windows binaries into `%LOCALAPPDATA%\trammel\bin`. The release workflow builds six binaries: Linux musl x64 and arm64, macOS x64 and arm64, Windows x64 and arm64.

From a checkout: `cargo build --release`. The binaries are `target/release/trammel` and `target/release/trammel-mcp`. `[dependencies]` is empty.

An existing `trammel.db` is not the 4.x store. `trammel migrate` reads that file and its WAL, writes `.trammel/`, and leaves the SQLite file in place. Limits of that copy are under Storage.

## Marks

Each difference below has one mark:

- **Intentional change.** A 4.x document or the rewrite pull request describes the new behaviour, or the Rust source states it as the replacement.
- **Bug fixed from Python.** The 3.x result depended on something 4.x removed on purpose, and the parity notes call the new result the stable one.
- **Not supported.** 4.0.1 does not do the 3.15.3 thing. Pull request #2 does not call the drop a planned cut. These rows are the ones to review. This pass did not change the code.

## What stays the same

Checked by reading both trees, and by running the checks in the last section where a run is named.

- The 12 primary tool names match: `start_plan`, `decompose`, `get_plan`, `complete_plan`, `verify_step`, `get_recipe`, `save_recipe`, `status`, `resume`, `export_plan`, `get_constraints`, `add_constraint`. `TRAMMEL_MCP_SURFACE=all` still lists the rest. The default listing is the primary 12.
- The nine beam names and descriptions match `trammel/strategies.py` and `src/strategies.rs`: `bottom_up`, `top_down`, `risk_first`, `critical_path`, `cohesion`, `minimal_change`, `leaf_first`, `hub_first`, `test_adjacent`.
- The beam count cap is the same formula: at least 3, at most 12, and at most the CPU count.
- The language names match `trammel/tool_schemas.py` `LANGUAGES` and `src/lang/analyze.rs` `LANGUAGES`: Python, TypeScript, JavaScript, Go, Rust, C, C++, Java, Kotlin, C#, Ruby, PHP, Swift, Dart, Zig. `c` and `cpp` share one extension list. `java` and `kotlin` share one extension list.
- Project config is still `.trammel.json`, and `[tool.trammel]` in a project's `pyproject.toml` when `.trammel.json` is absent.
- `verify_step` still resolves the test command as: caller argument, then the project file, then the language default. The goal command does not. See Command line.
- `export_plan` still writes `format` `trammel.plan` and `format_version` 1. Some fields in that document are empty. See Tools.
- A step claim still expires after 600 seconds (10 minutes).
- `get_recipe` still uses a minimum similarity of 0.55. The text blend is the same weights as `trammel/text_similarity.py`: 0.3 trigram cosine, 0.4 word Jaccard, 0.3 substring, after the same verb synonyms. 4.x does not add the later file, success, and recency terms. See Tools.
- Ignored directory names match `trammel/utils.py` `_IGNORED_DIRS`, including the `*.egg-info` suffix. 4.x also ignores `target` and `.claude` (Claude Code keeps whole-repo agent worktrees under `.claude/worktrees`, which planning would read and `verify_step` would copy). See Planning.
- On the five archetype projects, with the flags in the last section, decompose matched on step files, descriptions, symbols, relevance, rationale, goal fingerprint, and the dependency edge set.

## Install and platforms

| Item | Mark | 3.15.3 | 4.0.1 |
| --- | --- | --- | --- |
| Package | Intentional change | PyPI `trammel`, Python `>=3.10`. Optional extra `mcp>=1.0.0,<3`. | npm `@iron_adamant/trammel` 4.0.1. No Python runtime. No PyPI publish from this tree (`CHANGELOG.md`). |
| Server process | Intentional change | `trammel-mcp` imports the `mcp` package. Without it, the script prints an install hint and exits. | The MCP server is inside the binary. `trammel mcp` and `trammel-mcp` both serve stdio. |
| Platforms | Intentional change | Classifiers say OS independent. The 3.15.3 changelog says CI is Ubuntu and macOS. Windows was left out of that matrix. | Release binaries: Linux musl x64 and arm64, macOS x64 and arm64, Windows x64 and arm64. CI runs `cargo test` on Linux, macOS, and Windows. |
| Library import | Not supported | `from trammel import Planner` and the other names in `trammel/__init__.py`. | No Python module and no Rust library API for those types. `register_strategy` is not exposed. |
| Editable pattern file | Intentional change | `trammel/data/patterns.json` is loaded at runtime. | `data/patterns.json` is embedded with `include_str!` in `src/implicit.rs`. Editing a file next to an installed binary does not change the rules. Rebuild to change them. |

## Command line

3.15.3 `trammel` only runs a goal (`trammel/cli.py`). 4.0.1 also runs `doctor`, `migrate`, `dump`, `mcp`, and every tool name as a subcommand (`src/cli.rs`).

| Item | Mark | 3.15.3 | 4.0.1 |
| --- | --- | --- | --- |
| `--version` | Intentional change | argparse text: program name, space, package version. `pyproject.toml` says `3.15.3`. | One JSON object: `name`, `ok`, `schema_version` `4.0.0`, `version` `4.0.1`. `-V` is accepted. |
| `--help` | Intentional change | argparse text. | JSON (`ok`, `schema_version`, `usage`, `tools`), pretty-printed. `-h` is accepted. |
| Goal flags that stay | Intentional change | `--root`, `--beams`, `--test-cmd`, `--language`, `--scope`, `--dry-run`, `--export`. | Same flags. `--beams` is sent as `num_beams`. |
| `--db` | Intentional change | Path of the SQLite file. Default `trammel.db` in the working directory. | Removed. Use `--store`, or `TRAMMEL_DB_PATH`, for the `.trammel` directory. |
| `TRAMMEL_DB_PATH` | Intentional change | SQLite file path (`trammel/mcp_stdio.py`). | Store directory. If the path's extension is `.db`, `src/root.rs` uses that file's parent plus `.trammel`, and a real SQLite file at the path triggers `legacy_sqlite` until `migrate`. |
| Flag order | Not supported | argparse accepts `trammel --root DIR "goal"`. | The first token that does not start with `-` is the command. `trammel --root /tmp/tiny-proj "update helper"` used `/tmp/tiny-proj` as the goal (run on 2026-11-04). Put the goal or the tool name before flag values. |
| Stdin | Intentional change | A JSON object with a non-empty string `goal`. Other stdin is an error on stderr and exit 1. | With no non-dash token in argv, a JSON object with `goal` works, and so does a raw goal string. Measured: `update helper` and `{"goal":"fix helper"}` with only `--dry-run`. |
| Missing project root | Not supported | `Error: project root does not exist` on stderr, exit 1. | No directory check. `trammel "update helper" --root /tmp/no-such-trammel-root --beams 1 --test-cmd true` exited 0 with `ok` true and `success` false. The trace was `No such file or directory (os error 2)`. |
| `--dry-run` | Intentional change | Calls `explore`. Prints indented JSON `{"strategy", "beams"}`. | Calls the `explore` tool. Prints the envelope. `result` is `goal`, `strategy`, `beams`. Measured on `/tmp/tiny-proj`. |
| Goal run result | Not supported | `plan_and_execute` returns `status` `ok` or `failed`, plus `plan_id`. On success it also returns `strategy`, `outcome`, and `beam`, and it saves a recipe. | `result` is `goal`, `strategy`, `beams` (a count), `success`, `outcomes`. Measured with `--test-cmd true` on `/tmp/tiny-proj`: Python `status` `ok`, `plan_id` 1. Rust `success` true, `beams` 1, no `plan_id` in the JSON. |
| Goal run test command | Not supported | `resolve_test_cmd`: explicit `--test-cmd`, then project config, then the detected language. | `src/planner.rs` `plan_and_execute` uses `--test-cmd` when set. Otherwise it calls `test_cmd` for `--language`, or for `python` when `--language` is omitted. It does not read `.trammel.json`. A Go file at `/tmp/goproj/main.go`, goal `update main`, no `--language`, produced a Python `unittest` trace (`Ran 0 tests`). |
| Goal run edits | Not supported | Each beam's `edits` are passed to the harness. Those edits are `step_index`, `path`, and `task`, with no `content`, so the harness does not write files. The plan is then `completed` or `failed`, and a passing run saves a recipe. | Each beam is verified with an empty edit list (`src/planner.rs`). The plan row is set to `running` and that function does not set `completed` or `failed`, and it does not save a recipe. |
| `--export` on the goal CLI | Not supported | Writes a `trammel.plan` document from the strategy (`trammel/export.py`). | Writes the pretty-printed tool result (`src/cli.rs`). That is not the `trammel.plan` document. The `export_plan` tool still writes `trammel.plan`. |
| `doctor`, `dump`, `migrate`, `mcp` | Intentional change | Not subcommands of `trammel`. The server is only `trammel-mcp`. | `trammel doctor` prints version, store path, languages, protocols, and next steps. `trammel dump` prints store rows. `trammel migrate` copies a SQLite file. `trammel mcp` serves stdio (pull request #8). |
| `--human` | Intentional change | Output is always `json.dumps(..., indent=2)`. | Compact one-line JSON unless `--human`, which indents. |

## Output and errors

`docs/LLM_CONTRACT.md` is the 4.x contract. `schema_version` there is the string `4.0.0`. The store file uses a different integer `schema_version` (1). They are not the same field.

| Item | Mark | 3.15.3 | 4.0.1 |
| --- | --- | --- | --- |
| Success shape | Intentional change | The handler object, indented, insertion order. CLI exit 0. | `{"ok":true,"schema_version":"4.0.0", ...}` with sorted keys, one line. CLI adds `result` or `tool` plus `result`. |
| Failure shape | Intentional change | CLI: `Error: ...` on stderr, exit 1 or 2. MCP tool failure: text `Error: ...`, `isError` true (3.15.3). | CLI: the failure object on stderr, exit 1, or exit 2 when `code` is `usage`. `--human` prints `message` only. MCP: the same object in `structuredContent`, `isError` true. |
| `decompose` rejection | Not supported | `mcp_server.py` catches `ValueError` and returns `{"error":"decompose_rejected","message":...}` as a normal tool result. | `code` `decompose_rejected`, `isError` true. |
| Codes this tree returns | Intentional change | Python exceptions and the `error` string above. | `usage`, `tool_removed`, `unknown_tool`, `unknown_argument`, `missing_argument`, `plan_not_found`, `not_found`, `legacy_sqlite`, `decompose_rejected`, `claim_held`, `claim_mismatch`, `step_not_found`, `store`, `migrate`, `export`, `stdin`, `mcp`, `run`. `hint` and `retryable` are always present. `retryable` is false in `src/contract.rs`. |

## MCP

A 4.x `initialize` for protocol `2025-06-18` returned `protocolVersion` `2025-06-18`, `serverInfo.name` `trammel`, `serverInfo.version` `4.0.1`. An unknown protocol version (`1999-01-01`) came back as `2025-11-25`. `server/discover` returned protocol `2026-07-28`. This pass did not start the Python `mcp` server, so it does not compare SDK handshake bytes. `trammel/mcp_stdio.py` does not implement `server/discover`.

| Item | Mark | 3.15.3 | 4.0.1 |
| --- | --- | --- | --- |
| Tool count | Intentional change | 33 tools in `TOOL_SCHEMAS`. | 32 names from `tools::all_names`. `verify_plan` is included. |
| Removed names | Intentional change | `create_plan` and `record_step` dispatch. | Both return `code` `tool_removed`. Measured: `create_plan` says `Call start_plan instead`. `record_step` says `Call record_steps instead`. Exit 1. |
| Added name | Intentional change | No `verify_plan`. | `verify_plan` runs `verify_step` for each plan step that has both `file` and `content`, and stops at the first failure (`src/tools.rs`). |
| `tools/list` schema | Intentional change | Full JSON Schema: properties, types, `required`, enums (`trammel/tool_schemas.py`). | Each tool has a one-line description and an `inputSchema` with the arguments its handler reads: types, `required`, and enums (`src/catalogue.rs`). Arguments Rust ignores are not listed. A test calls every tool with no arguments and checks that the arguments it asks for are exactly `required`. Measured on Windows: primary list 10,137 bytes, all 22,256 bytes (Python primary was 12,508). No `outputSchema`. `annotations` has `readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint`. The files in `schemas/` are result envelopes. |
| `structuredContent` | Intentional change | The handler value. | Always an object, as MCP requires. A tool that returns a list (`list_plans`, `list_recipes`, `list_strategies`, `get_constraints`, `failure_history`, `available_steps`) sends `{"items": [...]}`. Before this, the MCP TypeScript SDK client rejected those results (`Expected object, received array`). The CLI result is unchanged. |
| Surface aliases | Not supported | `all`, `full`, `advanced`, and `*` list every tool. | Measured: `all` lists 32, `advanced` lists 12. `src/mcp.rs` treats `all` and `full` as the full list. Any other value, including `advanced` and `*`, stays on the primary 12. |
| Tool text | Intentional change | `json.dumps(result, indent=2)`. | Compact sorted JSON. Success text is the handler value. Failure text is the failure object. |
| Protocol list | Intentional change | Whatever the installed `mcp` package negotiates. | `2026-07-28` (`server/discover`), `2025-11-25`, `2025-06-18`. |

## Planning

`scripts/parity_check.py` on `python-legacy` compares decompose with Python `skip_recipes=True`, `suppress_creation_hints=True`, and `apply_project_config=False`. Pull request #2 says the five archetypes match on files, descriptions, symbols, relevance, rationale, fingerprint, and the edge set, and that `test_farm` may differ in tied-edge order.

This pass ran that comparison with `PYTHONPATH` pointed at `trammel/` from `python-legacy`, against `target/debug/trammel` built from this tree. Goal text was `update shared helpers`.

| Archetype | Steps | Files, descriptions, symbols, relevance, rationale, fingerprint | Edge set | Edge order |
| --- | --- | --- | --- | --- |
| `legacy_ball_of_mud` | 14 | match | match | match |
| `monorepo` | 5 | match | match | match |
| `facade_service` | 13 | match | match | match |
| `generated_code` | 7 | match | match | match |
| `test_farm` | 14 | match | match (44 edges) | differs |

`tests/test_file_6.py` in `test_farm` is one key whose neighbour list has the same paths in a different order. Rust had `src/module_4.py` before `src/module_8.py`. Python had the opposite. Fingerprint on that goal was `96f2631fe9f02e46` on both sides.

| Item | Mark | Detail |
| --- | --- | --- |
| Tied implicit edges | Bug fixed from Python | Python appended equal-confidence edges in set order. That order follows `PYTHONHASHSEED`. `scripts/parity_check.py` calls the Rust order an intentional difference: higher confidence first, then discovery order from sorted maps, stable across runs. `tests/determinism.rs` checks `tests/goldens/test_farm_decompose.json`. The edge set still matches. |
| Symbol order outside the archetypes | Not supported | Pull request #2 says symbol order on other trees can differ from CPython `ast.walk`, because the vendored extractor walks in its own order. This pass did not re-run rich or flask. The five archetypes matched, including `symbols`. |
| Paths | Intentional change | `src/walk.rs` writes relative paths with `/` on every OS. The parity script says Python on Windows emits backslashes and compares both sides with `/`. This pass ran on Linux only. |
| `target` directories | Intentional change | `src/walk.rs` ignores `target` as well as the Python ignore list, so a Rust build directory is not scanned. |
| 15-file cap | Intentional change | With no scaffold, Python `core.py` caps a full-repo decompose at 15 files. Rust does the same when `expand_repo` is true (the default), including when `max_files` is higher than 15 (`src/planner.rs`). Python kept the first 15 files in dependency order, which on a large repo is close to alphabetical and often left out every file the goal names. Rust keeps the 15 files with the highest step relevance for the goal, in dependency order, and falls back to the first 15 when the goal has no keywords. `test_farm` has 14 steps, so the cap did not drop a step and the golden is unchanged. `plan_fidelity.max_files_cap` was 15. |
| `expand_repo=false` | Not supported | Python returns scaffold-only steps, or an `empty_scaffold` error when there is no scaffold. Rust has no scaffold argument. `expand_repo=false` only turns the 15-file cap off. On `test_farm` both runs still returned 14 steps. Rust `max_files_cap` was null. |
| `scaffold` | Not supported | Python `decompose` and `start_plan` take a scaffold and can plan from it. `src/tools.rs` `plan_tool` does not read `scaffold`. `Request` has no scaffold field. |
| `skip_recipes` | Not supported | The argument is not read. `decompose` does not open the store, so it never returns a cached recipe. That is the same path as Python `skip_recipes=True`, which is what the parity harness compares. Automatic recipe reuse during decompose is absent. |
| `min_relevance` | Not supported | `plan_tool` does not set it. `decompose` discards the field (`let _ = request.min_relevance`). |
| `apply_project_config` | Not supported | The argument is not copied onto `Request`, which defaults to true. `--apply-project-config false` on the parity command does not change the Rust call. The five archetypes still matched Python with the flag set false. |
| Stored constraints during decompose | Not supported | `constraints` and `constraints_applied` are empty arrays in `src/planner.rs`. An `avoid` constraint does not mark a step skipped. `SYSTEM_PROMPT.md` still describes the 3.x effects. |
| Creation hints | Not supported | Python scaffold inference (layered roles, sibling clones, partial plans) is not ported. Rust may add a smaller `creation_hints` object when the goal looks like creation and `suppress_creation_hints` is false. This pass did not compare those objects. The parity runs set the flag true. |
| Recipe index | Not supported | Python keeps trigram, TF-IDF, and MinHash tables, plus scaffold recipes. Rust `get_recipe` scans `recipes` with the text blend only. |
| `register_strategy` and win-rate order | Not supported | `explore` is called with an empty stats map, so beams stay in registry order. `list_strategies` returns `successes` 0 and `failures` 0 for all nine. Measured. |

## Storage

3.15.3 stores SQLite at `trammel.db` (plus WAL and SHM). Tables created in `trammel/store.py` and `trammel/recipe_index.py` include `recipes`, `recipe_trigrams`, `recipe_files`, `scaffold_recipes`, `scaffold_trigrams`, `failure_patterns`, `usage_events`, `plans`, `steps`, `constraints`, `trajectories`, and the recipe index tables. The 3.15.3 changelog counts 15 tables.

4.x stores a JSON snapshot at `.trammel/store`, an append journal at `.trammel/journal`, and a lock file at `.trammel/lock`. A new store's arrays are `plans`, `steps`, `constraints`, `recipes`, `failures`, `usage_events`, and `trajectories` (`src/store.rs`).

`trammel migrate` copies every non-`sqlite_` table into that JSON document. The source file stays. Measured on a database written with Python's `sqlite3`: `migrated_from`, `plans` 1, `recipes` 1, `source_untouched` true, and the `.db` file still present.

| Item | Mark | Detail |
| --- | --- | --- |
| File location | Intentional change | `.trammel/` next to the project root (`find_root` walks up to `.git` or `.trammel.json`). A leftover `trammel.db` makes most tools return `code` `legacy_sqlite` until `migrate`, unless `.trammel/store` already exists. `status`, `estimate`, and `list_strategies` are exempt (`src/tools.rs`). |
| Integer primary keys | Not supported | SQLite stores an `INTEGER PRIMARY KEY` as the rowid. `src/migrate.rs` drops the rowid. On the sample database, the imported plan, step, and `failure_patterns` row had `"id": null`. `plan_id` on the step stayed 1, because that column is not the primary key. `get_plan` looks up `id`, so that imported plan cannot be opened by id. A text primary key is kept: the imported recipe `sig` `abc` was returned by `get_recipe` for `update helper` with `similarity` 1. The unit test `reads_a_python_sqlite_database` only inserts a text primary key. |
| `failure_patterns` | Not supported | The importer keeps the table name `failure_patterns`. `failure_history` reads the array `failures`. After migrate, `failure_history` returned `[]` while `failure_patterns` held the SyntaxError row. |
| New ids after migrate | Not supported | `next_plan` starts at 1 when the imported document has no `next_plan`. The sample's new `start_plan` was `plan_id` 1, the same number as the step's `plan_id`, while the imported plan's own `id` was null. |
| `active` on constraints | Intentional change | Python stores `active` as an integer. `as_bool` accepts only JSON booleans. A non-boolean `active` is treated as active (`unwrap_or(true)` in `constraints`). |
| Telemetry | Intentional change | Python `dispatch_tool` writes a `tool_call` usage row on every call. Rust writes usage only when `TRAMMEL_TELEMETRY=1` or the project config enables telemetry. `usage_stats` on an empty log returns `enabled` false, `events` 0, and a hint. It does not accept `days`, and it does not return hit rates or win rates. |

## Tools

Arguments below are the ones Python advertises and Rust does not honour, or the result fields that changed. Tools not listed still accept the arguments `src/tools.rs` reads (goal, project root, scope, language, plan id, and so on).

| Tool | Mark | 4.0.1 behaviour |
| --- | --- | --- |
| `start_plan` | Not supported | Decompose, then insert a plan. No duplicate-goal check (Python `create_plan` returned an existing id for the same goal and strategy JSON). `persist=false` returns `plan_id` null. A `strategy` string, when set, is stored as `strategy_name` on the saved copy. |
| `decompose` | Not supported | Fresh analysis only. See Planning for the ignored arguments. Top-level keys on the archetype runs matched Python: `analysis_meta`, `constraints`, `constraints_applied`, `dependency_graph`, `expand_repo`, `goal`, `goal_fingerprint`, `plan_fidelity`, `steps`, `trammel_blind_spots`. |
| `explore` | Intentional change | Returns `goal`, `strategy`, and `beams`. Beam objects still have `beam_id`, `variant`, `variant_description`, `steps`, `edits`, and `skipped`. There is no second pass with `suppress_creation_hints` when the first decompose fails. |
| `get_plan` | Intentional change | The plan object plus its `steps`. `strategy` is parsed from the stored JSON string when that parse succeeds. |
| `resume` | Not supported | Same payload as `get_plan`. Python `get_plan_progress` returns `prior_edits`, remaining steps, and the next index. Measured keys on a migrated plan: `created`, `current_step`, `goal`, `id`, `scaffold`, `status`, `steps`, `strategy`, `total_steps`, `updated`. |
| `history` | Not supported | Always `{"plan_id": N, "trajectories": []}`, including when the store has a `trajectories` array. Measured on plan 1. Python returns the rows from `trajectories`. The goal command does not write those rows either. |
| `list_plans` | Intentional change | `db_path` is rejected. Measured: `code` `unknown_argument`, message `list_plans no longer accepts db_path`, hint `Set --store or TRAMMEL_DB_PATH`. |
| `update_plan_status` | Intentional change | Sets `status` and `updated`. Unknown id is `plan_not_found`. |
| `complete_plan` | Not supported | Sets the plan to `completed` or `failed` and saves a recipe. It does not mark pending steps, and it ignores `step_status`. Measured: after `complete_plan` with `outcome` true, the plan status was `completed` and the step status was still `pending`. Python defaults `step_status` to `passed`. |
| `export_plan` | Not supported | `format` `trammel.plan`, `format_version` 1, goal, status, plan id, steps, strategy. `dependency_graph` is `{}`, `scaffold` is null, and `constraints` is `[]` (`src/export.rs`). Step objects keep index, description, rationale, `depends_on`, status, and file. Python also copies symbols, edits, verification, and id when those fields exist. |
| `verify_step` | Intentional change | Copies the project, applies `edits` (`path` or `file`, plus `content`), runs a preflight, then the test command, with a 60 second cap. `trace` is cut at 4 KB unless `full_output` is true (`docs/LLM_CONTRACT.md`). `prior_edits` is not read. On Windows the default Python command is the first real `python3` or `python` on `PATH`, then `py -3`; a Microsoft Store alias is used only when nothing else exists. A bare program name is looked up with `PATHEXT`, so `npm` and `npx` run their `.cmd` shims. Languages without a runner use `true`, or `cmd /C exit 0` on Windows. Output is read while the test runs, so a large test log no longer stalls until the timeout. A timeout sets `trace` to the limit in seconds. |
| `verify_plan` | Intentional change | New. See MCP. It opens the store like the other plan tools. It used to skip that and panic, which ended the MCP server. |
| `record_steps` | Not supported | Sets `status` on each step id. The return value is `updated` (a count). Ids that match no step come back in `not_found` with a `hint`. Edits and verification are not stored. Python returns `ok` and `steps_updated` and stores those JSON columns. |
| `save_recipe` | Not supported | Stores `sig`, `pattern` (goal, at most 400 characters), strategy JSON, success and failure counts, and timestamps. Returns `signature`. `outcome` defaults to true. A `scaffold` argument is not written to a scaffold table. Python returns `{"ok": true}` and can also save a scaffold recipe. |
| `get_recipe` | Not supported | Text blend only, threshold 0.55. A miss is `{"recipe": null}` (measured). Python returns `match` null and `near_match_recipes`. `context_files`, `include_scaffold`, and `debug` are not read. There is no `match_components` object. |
| `list_recipes` | Not supported | Newest `updated` first. Default `limit` is 50. Python's default is 20, and the list includes file paths from `recipe_files`. |
| `prune_recipes`, `prune_plans`, `validate_recipes` | Not supported | Each returns `{"removed": 0}` and ignores `max_age_days`, `min_success_ratio`, `status`, and `project_root`. Measured. Python prune returns a real count (`pruned` for plans). |
| `list_strategies` | Not supported | Nine names and descriptions, with zero success and failure counts. See Planning. |
| `add_constraint` / `get_constraints` / `deactivate_constraint` | Intentional change | Stored and listed. Types are not checked against the Python enum. Deactivating a missing id is `not_found`. They are not applied during decompose. |
| `estimate` | Intentional change | `language`, `scope`, `matching_files`, and `recommendation` (`use scope` above 5000 files, otherwise `full analysis OK`). Same threshold as Python. |
| `status` | Intentional change | `store`, `counts` (plans, steps, recipes, constraints, failures, usage events), `primary_tools`, `advanced_tools`, `tool_surface_default`, `tool_surface_env`. It does not return `tools_by_category`. `SYSTEM_PROMPT.md` still mentions that field. |
| `usage_stats` | Not supported | See Storage. |
| `failure_history` | Not supported | Reads `failures`, default `limit` 50. Python's default is 20 and it reads `failure_patterns` (occurrences, first seen, last seen). `resolve_failure` inserts a new `failures` row. It does not update a pattern, and it does not move a step out of `running` or release a claim. Python's 3.12.2 `plan_id` / `step_id` behaviour did that. |
| `claim_step` / `release_step` | Intentional change | Claim sets `claimed_by`, `claimed_at`, and status `running`. A live claim by someone else is `claim_held`. Release by another agent is `claim_mismatch`. A `step_id` that is not a step `id` (often a `step_index`) is `step_not_found`, with a hint saying so. Python `claim_step` can also return a proximity warning when another plan touches the same file. That warning is not built here. |
| `available_steps` | Not supported | Returns pending or running steps for the plan that are not held by another agent inside 600 seconds. It does not require `depends_on` to be satisfied. Python `get_available_steps` does, and it only returns `pending` steps. |
| `merge_plans` | Not supported | `interleave` alternates steps. Any other `strategy` value, including `priority` and `unified`, appends the second plan's steps. The new plan's goal is the two goals joined with ` + `. The result is `plan_id` and `strategy`. There is no conflict report and no cycle check. |

## Checked on 2026-11-04

- Read `python-legacy` `trammel/cli.py`, `tool_schemas.py`, `mcp_server.py`, `mcp_stdio.py`, `store.py`, `core.py` (the 15-file cap), `strategies.py`, `text_similarity.py`, `export.py`, `harness.py`, `__init__.py` `plan_and_execute`, and `scripts/parity_check.py`.
- Read `src/cli.rs`, `src/tools.rs`, `src/planner.rs`, `src/store.rs`, `src/migrate.rs`, `src/mcp.rs`, `src/export.rs`, `src/similarity.rs`, `src/strategies.rs`, `src/walk.rs`, `src/root.rs`, `docs/LLM_CONTRACT.md`, and `CHANGELOG.md`.
- Pull requests: #2 (Rust 4.0 rewrite), #4 (npm scope), #7 (Python package removed from `main`), #8 (`trammel mcp` and the registry manifest).
- Built `target/debug/trammel` and `target/debug/trammel-mcp` from this tree (rustc 1.99.1). Python was the `python-legacy` package on `PYTHONPATH`, not a PyPI install. `python3 -m trammel --version` printed `__main__.py dev` because the distribution metadata was not installed. The source version in that `pyproject.toml` is 3.15.3.
- Decompose comparison above, plus the CLI, migrate, MCP, and tool calls named in the tables. Host: Linux. Windows path behaviour is from the source comments, not from a Windows run.

`SYSTEM_PROMPT.md` still tells a client to pass `scaffold`, `skip_recipes`, `prior_edits`, and `include_scaffold`, and it still says decompose applies constraints, `resume` returns `prior_edits`, and `history` returns beam outcomes. Those sentences describe 3.15.3. The rows in this file are what 4.0.1 does.
