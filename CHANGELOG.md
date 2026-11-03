# Changelog

Older releases are recorded in `README.md` (Full Changelog) and `LLM_Development.md`.

## 3.15.3

### Fixed

- **`trammel-mcp` crash on MCP Python SDK 2.x.** A fresh `pip install trammel[mcp]` resolved `mcp` 2.x. SDK 2.0 removed the low-level `@server.list_tools()` / `@server.call_tool()` decorators, so startup raised `AttributeError: 'Server' object has no attribute 'list_tools'`. The stdio server now uses those decorators on SDK 1.x and `on_list_tools` / `on_call_tool` on SDK 2.x. The extra is `mcp>=1.0.0,<3` (same range stele-context uses). Verified against the latest 2.x resolver result and against mcp 1.30.0.
- **Tool failures looked like successes.** Exceptions inside a tool call were returned as ordinary text content. They now come back as a tool result with `isError: true`.
- **`initialize` reported the mcp library version.** `serverInfo.version` is Trammel's own version.
- **Doc counts.** Current layout, schema, and architecture notes now match the code: 33 MCP tools (12 primary), 15 SQLite tables, 12 `SPEC_REGISTRY` entries (10 spec-backed engine classes, with C and Kotlin aliasing C++ and Java, plus regex TypeScript and JavaScript analyzers).

### Changed

- CI tests Python 3.10–3.13 on Ubuntu and macOS. Python 3.12 jobs also perform a fresh virtualenv install of `trammel[mcp]` and drive `initialize` plus `tools/list` over stdio. Ubuntu 3.12 additionally does the same against mcp 1.30.0. Windows was tried and left out: tests assert POSIX relative paths, and temp-directory cleanup hits `WinError 32` while SQLite still holds the database file.
