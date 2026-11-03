"""Stdio-based MCP server for Trammel using the ``mcp`` Python package.

Entry point::

    trammel-mcp              (installed via pyproject.toml console_scripts)
    python -m trammel.mcp_stdio

Requires the optional ``mcp`` dependency:
    pip install trammel[mcp]

The extra allows MCP Python SDK 1.x and 2.x (``mcp>=1.0.0,<3``). SDK 2.0
removed the low-level ``@server.list_tools`` / ``@server.call_tool``
decorators, so startup branches on which API is installed.
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
import sys
from importlib.metadata import PackageNotFoundError, version as _meta_version
from typing import Any

from .mcp_server import _schemas_for_surface, dispatch_tool
from .store import RecipeStore
from .utils import DEFAULT_DB_PATH

logging.basicConfig(level=logging.WARNING, stream=sys.stderr)
logger = logging.getLogger(__name__)

try:
    from mcp.server import Server
    from mcp.server.stdio import stdio_server
    from mcp.types import CallToolResult, TextContent, Tool

    _MCP_AVAILABLE = True
except ImportError:  # pragma: no cover - exercised only without the extra
    Server = None  # type: ignore[misc, assignment]
    stdio_server = None  # type: ignore[assignment]
    CallToolResult = None  # type: ignore[misc, assignment]
    TextContent = None  # type: ignore[misc, assignment]
    Tool = None  # type: ignore[misc, assignment]
    _MCP_AVAILABLE = False


def _trammel_version() -> str:
    """Return this package's version for ``serverInfo``, never the mcp library's."""
    try:
        return _meta_version("trammel")
    except PackageNotFoundError:
        return "dev"


def _tool_arguments(arguments: Any) -> dict[str, Any]:
    if not arguments:
        return {}
    if isinstance(arguments, dict):
        return arguments
    return dict(arguments)


def _configure_server(db_path: str) -> Any:
    """Register Trammel tools on an MCP ``Server`` for SDK 1.x or 2.x.

    Each tool call creates its own ``RecipeStore`` (and thus its own
    ``sqlite3.Connection``) inside the worker thread spawned by
    ``asyncio.to_thread``.  This avoids the *"SQLite objects created in a
    thread can only be used in that same thread"* error that occurs when a
    connection created on the event-loop thread is reused in a worker.

    Tool failures are returned as ``CallToolResult(isError=True)``. Returning
    a bare content list (SDK 1.x) is a successful tool result, and an uncaught
    exception (SDK 2.x) becomes a JSON-RPC error instead of ``isError``.
    """
    version = _trammel_version()
    surface = os.environ.get("TRAMMEL_MCP_SURFACE", "primary")
    listed_schemas = _schemas_for_surface(surface)

    def _listed_tools() -> list[Any]:
        return [
            Tool(
                name=name,
                description=schema["description"],
                inputSchema=schema["parameters"],
            )
            for name, schema in listed_schemas.items()
        ]

    async def _invoke(name: str, arguments: Any) -> Any:
        try:
            def _run() -> object:
                with RecipeStore(db_path) as thread_store:
                    return dispatch_tool(thread_store, name, _tool_arguments(arguments))

            result = await asyncio.to_thread(_run)
        except Exception as exc:
            logger.exception("Error executing tool %s", name)
            return CallToolResult(
                content=[TextContent(type="text", text=f"Error: {exc}")],
                isError=True,
            )
        text = json.dumps(result, indent=2, default=str)
        return CallToolResult(
            content=[TextContent(type="text", text=text)],
            isError=False,
        )

    # SDK 1.x: decorator registration. SDK 2.x: constructor callbacks.
    if hasattr(Server, "list_tools"):
        server = Server("trammel", version=version)

        @server.list_tools()
        async def list_tools() -> list[Any]:
            return _listed_tools()

        @server.call_tool()
        async def call_tool(name: str, arguments: dict[str, Any] | None) -> Any:
            return await _invoke(name, arguments)

        return server

    from mcp.types import ListToolsResult

    async def list_tools_v2(_ctx: Any, _params: Any) -> Any:
        return ListToolsResult(tools=_listed_tools())

    async def call_tool_v2(_ctx: Any, params: Any) -> Any:
        return await _invoke(getattr(params, "name", ""), getattr(params, "arguments", None))

    return Server(
        "trammel",
        version=version,
        on_list_tools=list_tools_v2,
        on_call_tool=call_tool_v2,
    )


async def _run_server() -> None:
    """Start the stdio MCP server and run until the client disconnects."""
    db_path = os.environ.get("TRAMMEL_DB_PATH", DEFAULT_DB_PATH)
    server = _configure_server(db_path)

    async with stdio_server() as (read_stream, write_stream):
        await server.run(
            read_stream,
            write_stream,
            server.create_initialization_options(),
        )


def main() -> None:
    """Entry point for ``trammel-mcp`` console script."""
    if not _MCP_AVAILABLE:
        print(
            "Error: The 'mcp' package is not installed.\n"
            "\n"
            "The Trammel stdio MCP server requires the 'mcp' Python package.\n"
            "Install it with:\n"
            "\n"
            "    pip install trammel[mcp]\n"
            "\n"
            "Or install the mcp package directly:\n"
            "\n"
            "    pip install 'mcp>=1.0.0,<3'\n",
            file=sys.stderr,
        )
        sys.exit(1)

    try:
        asyncio.run(_run_server())
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
