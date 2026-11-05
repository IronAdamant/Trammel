# Trammel

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![npm](https://img.shields.io/npm/v/%40iron_adamant%2Ftrammel)](https://www.npmjs.com/package/@iron_adamant/trammel)

**Trammel helps coding agents plan, verify, and hand off multi-file changes.**

Give it a goal and a project. It reads the imports and symbols, orders the files to change by dependency, checks edits by running the tests on a temporary copy of the project, and keeps plans, rules, failures, and successful strategies in `.trammel/` so the next session or another agent can pick the work up. It is one std-only Rust binary (`trammel`, `trammel-mcp`) with no dependencies and no network access.

> Trammel is a tool for LLMs, not a tool that calls LLMs. It does not write code; the agent does.

![Trammel Plan & Explore Demo](docs/demo-plan-explore.png)

**15 languages:** Python, TypeScript, JavaScript, Go, Rust, C/C++, Java/Kotlin, C#, Ruby, PHP, Swift, Dart, Zig. The old Python version is on the `python-legacy` branch.

## What an agent gets

| Call | What it saves the agent |
|---|---|
| `start_plan` / `decompose` | One call returns the files a goal touches, in dependency order, with their symbols and goal relevance. |
| `verify_step` | Edits applied to a throwaway copy, a syntax preflight, and the project's tests, without touching the working tree. Output is capped at 4 KB. |
| `get_plan` / `add_note` | A saved plan with step status, claims, and the previous agent's notes: how a new session or a sub-agent picks up work cold. |
| `add_constraint` / `get_constraints` | Rules and known-bad approaches that every later agent reads before editing. |
| `claim_step` / `available_steps` / `record_steps` | Several agents on one plan without doing the same step twice. Claims expire after 10 minutes. |
| `complete_plan` / `get_recipe` | A strategy that worked, matched to later goals by text similarity. |

## Install

```bash
npm i -g @iron_adamant/trammel        # puts trammel and trammel-mcp on PATH
npx -y @iron_adamant/trammel mcp      # or run the MCP server without installing
```

Without npm, from a GitHub Release: `sh install.sh` (Linux or macOS, into `~/.local/bin`) or `install.ps1` (Windows, into `%LOCALAPPDATA%\trammel\bin`). From this checkout: `cargo build --release`, which builds `target/release/trammel` and `target/release/trammel-mcp`.

## Use it from an MCP client

Claude Code:

```bash
claude mcp add trammel -- npx -y @iron_adamant/trammel mcp
```

Any client with a JSON config (Cursor, Claude Desktop, and others):

```json
{
  "mcpServers": {
    "trammel": {
      "command": "npx",
      "args": ["-y", "@iron_adamant/trammel", "mcp"]
    }
  }
}
```

The server speaks stdio (protocols `2026-07-28`, `2025-11-25`, `2025-06-18`, `2025-03-26`, `2024-11-05`) and works on the project in its working directory; every tool also takes `project_root`. `tools/list` shows 13 primary tools with full input schemas. Set `TRAMMEL_MCP_SURFACE=all` in the server's environment to list all 33, including the multi-agent tools.

A typical loop:

1. `get_constraints` and `get_recipe` for anything learned earlier.
2. `start_plan` with the goal. Keep the `plan_id`.
3. For each step: edit, then `verify_step` with the changed files. Fix and repeat until it passes.
4. `complete_plan` saves the strategy as a recipe.

To hand off, `add_note` what you did and what is left, and give the next agent the `plan_id`; it calls `get_plan` and reads the notes. [SYSTEM_PROMPT.md](SYSTEM_PROMPT.md) is a fuller prompt for MCP clients. [docs/LLM_CONTRACT.md](docs/LLM_CONTRACT.md) is the JSON contract.

## Use it from the command line

Every tool is also a subcommand. Output is one JSON object on stdout; `--human` indents it.

```bash
trammel doctor                                   # store path, languages, next steps
trammel decompose --goal "add retries to the HTTP client" --project-root .
trammel start_plan --goal "add retries to the HTTP client" --summary_only true
trammel get_plan --plan_id 1
trammel "refactor auth module" --dry-run         # plan only, nine orderings
trammel "fix tests" --test-cmd pytest -x -q      # plan, then run the tests per ordering
```

`trammel migrate` imports an old Python `trammel.db` into `.trammel/` and leaves the SQLite file in place.

## How it works

1. **Decompose.** Parse the project's imports and symbols, add edges inferred from naming and shared-state rules (`data/patterns.json`), sort files by dependency, and score each file's relevance to the goal. A whole-repo plan keeps the 15 most relevant files unless `expand_repo` is false.
2. **Explore.** Nine orderings (bottom-up, top-down, risk-first, and others) of the same steps, for when the default order is not the one you want.
3. **Verify.** Copy the project to a temporary directory, apply the edits, check syntax, run the test command, and report pass or fail with capped output.
4. **Constrain.** Store rules and failures so later agents see them.
5. **Remember.** Save a successful strategy; `get_recipe` matches later goals by a text blend (trigram cosine, word Jaccard, substring).

State lives in `.trammel/` in the project. Several processes can share it; writes take a file lock.

## Configuration

Optional. `.trammel.json` in the project root (or `[tool.trammel]` in `pyproject.toml`):

```json
{
  "default_scope": "src/",
  "focus_keywords": ["auth", "login"],
  "max_files": 50,
  "test_cmd": ["cargo", "test"]
}
```

`verify_step` takes its test command from the call's `test_cmd`, then this file, then the language default (`python -m unittest`, `cargo test`, `npm test`, `go test ./...`, and so on). `TRAMMEL_TELEMETRY=1` records local usage counts; nothing leaves the machine.

## Benchmarks

Figures below were measured, not estimated. Rust is faster on every row than both the recorded baseline and a same-machine rerun of Python 3.15.3.

The recorded column is `bench/python_baseline.json`: Trammel 3.15.2, Python 3.13.5, mcp 1.30.0, Linux 6.12.94 x86_64, 8 vCPU Intel Xeon, 15.6 GB, 2026-11-04. That box is not the one that produced the other two columns.

The same-runner columns are `bench/same_runner.json`, measured 2026-11-03 on Linux 6.12.94 x86_64, 4 vCPU Intel Xeon, 15.6 GB, rustc 1.99.1, Python 3.12.3, Trammel 3.15.3, mcp 2.3.0. rich is the 13.9.4 git tag (190 `.py` files). flask is the 3.1.0 sdist (83 `.py` files). Startup and cold MCP rows are 10 runs. The flask full run is 3. Every other row is 7. Values are medians in milliseconds.

| Benchmark | Recorded Python | Same-runner Python | Same-runner Rust |
|---|---:|---:|---:|
| CLI `--version` | 78.1 | 87.19 | 0.70 |
| CLI `--help` | 78.7 | 91.88 | 0.71 |
| CLI dry-run rich | 469.9 | 97.86 | 19.89 |
| CLI dry-run flask | 253.7 | 318.88 | 10.41 |
| CLI dry-run trammel-src | 332.4 | 109.34 | 11.87 |
| CLI full flask, test command `true` | 321.1 | 356.13 | 28.44 |
| MCP primary `initialize` | 306.6 | 568.73 | 0.94 |
| MCP primary `tools/list` | 1.8 | 1.72 | 0.10 |
| MCP all `initialize` | 308.8 | 559.88 | 0.94 |
| MCP all `tools/list` | 2.1 | 2.00 | 0.32 |
| MCP status, empty store | 18.8 | 12.74 | 0.16 |
| MCP estimate rich | 4.5 | 4.16 | 1.01 |
| MCP decompose rich | 425.6 | 550.84 | 20.85 |
| MCP decompose rich, summary only | 400.0 | 566.02 | 19.76 |
| MCP decompose flask | 171.3 | 211.98 | 8.81 |
| MCP explore rich, 3 beams | 427.7 | 538.33 | 20.29 |
| MCP start_plan rich | 417.5 | 566.15 | 20.60 |
| MCP get_plan | 5.9 | 3.61 | 0.54 |
| MCP resume | 6.4 | 3.46 | 0.49 |
| MCP verify_step, rich import | 183.3 | 172.86 | 135.34 |
| MCP verify_step, Trammel unit tests | 3179.7 | 3417.21 | 2767.64 |
| MCP record_steps | 7.6 | 5.65 | 1.65 |
| MCP add_constraint | 6.4 | 2.69 | 0.94 |
| MCP get_constraints | 3.9 | 2.96 | 0.24 |
| MCP export_plan | 4.2 | 2.92 | 0.58 |
| MCP complete_plan | 10.7 | 5.82 | 1.14 |
| MCP get_recipe | 7.9 | 4.37 | 0.53 |
| MCP list_plans | 5.2 | 2.76 | 0.34 |
| MCP status after the flow | 4.3 | 2.44 | 0.26 |
| MCP VmHWM after the flow (MB) | 66.6 | 75.30 | 8.09 |

`tools/list` payload, one measurement of the JSON line: Python primary 12,508 bytes and 12 tools, Python all 23,515 bytes and 33 tools, Rust primary 3,340 bytes and 12 tools, Rust all 8,777 bytes and 32 tools. This x86_64 release build is `trammel` 2,131,840 bytes and `trammel-mcp` 2,032,264 bytes. The CI floor in `examples/bench_gate.rs` fails a build whose `--version`, `--help`, or MCP `initialize` median is at or above the recorded Python median. The regression rule (an A/A run passes, an injected slowdown fails, no percentage tolerance) is `cargo test --lib benchstat`.

Since those runs, `tools/list` carries full input schemas, so its payload is larger; [MIGRATION.md](MIGRATION.md) has the current sizes.

## Coming from the Python version

Trammel 3.15.3 was the last Python release; it is on the `python-legacy` branch, and its release notes are in [docs/PYTHON_CHANGELOG.md](docs/PYTHON_CHANGELOG.md). The commands, the 12 primary tool names of 3.x (4.x adds `add_note`), the nine beam names, the 15 languages, `.trammel.json`, `trammel.plan` exports, and the 10 minute claim are unchanged. `trammel migrate` imports an old store. [MIGRATION.md](MIGRATION.md) lists every difference, each marked as an intentional change, a bug fixed from Python, or not supported.

## Development

```bash
cargo test --locked                       # library, CLI, MCP, and repo-check tests
cargo run --example check_zero_deps       # Cargo.lock names only trammel
cargo run --example check_no_todos        # no TODO or FIXME left in src/
cargo run --example check_syntax_core     # vendored parser files match the manifest
cargo build --release
cargo run --release --example bench_gate -- target/release/trammel
```

CI runs these on Linux, macOS, and Windows. The crate has no dependencies and `[dependencies]` stays empty; the repo checks are std-only Rust too. [AGENTS.md](AGENTS.md) maps the modules and explains how to add a language or a tool.

| Path | Contents |
|---|---|
| `src/` | Library and the `trammel` / `trammel-mcp` binaries |
| `examples/` | Repo checks and the benchmark gate |
| `schemas/` | Result schema per MCP tool |
| `data/patterns.json` | Implicit-edge rules, embedded at compile time |
| `tests/`, `parity/fixtures/` | Rust tests, sample projects, decompose golden |
| `npm/` | The npm wrapper and six platform packages |
| `bench/` | Recorded Python and Rust timings |

Releases: a `v*` tag runs `.github/workflows/release.yml`, which checks that the tag matches the version in Cargo.toml, the npm packages and server.json, builds six binaries (Linux musl, macOS, and Windows, each x64 and arm64), uploads them with `SHA256SUMS`, and publishes the npm packages. `scripts/release.sh` checks the version and prints the tag command; it does not publish. `server.json` is the MCP Registry manifest for `io.github.IronAdamant/trammel`. Changes are in [CHANGELOG.md](CHANGELOG.md).

Trammel works on its own. With [Stele](https://github.com/IronAdamant/stele-context) and [Chisel](https://github.com/IronAdamant/Chisel) the three meet at the MCP layer, with no code dependency between them.

## License

MIT. See [LICENSE](LICENSE).
