# Grok Connectors / BYO MCP Support

This project is ready for Grok Connectors (Bring Your Own MCP) and Grok Build.

The MCP server is the std-only Rust binary. It works with any MCP client, including Grok Build local agents and remote MCP connections in Grok.

The old Python version lives on the `python-legacy` branch.

## For Grok Connectors (BYO MCP)

When Grok supports adding custom MCP servers:

1. Start the server locally:

```bash
trammel mcp
```

`npx -y @iron_adamant/trammel mcp` is the same server. `trammel-mcp` is the same server with no extra argument. Install with `npm i -g @iron_adamant/trammel`.

2. For remote access from Grok, expose the endpoint over HTTPS using a secure tunnel (for example Cloudflare Tunnel).

3. Add to Grok Connectors / remote MCP config:
   - **server_url**: `https://your-https-tunnel/mcp` (or the specific MCP endpoint)
   - **server_label**: `trammel`
   - **server_description**: "Planning harness for coding agents: dependency-aware decomposition, beam strategy, verification, and recipe memory"

Grok discovers the tools from the server. The default list is 13 primary tools. `TRAMMEL_MCP_SURFACE=all` lists all 33.

## For Grok Build (Local)

Use the MCP server alongside Grok Build agents for planning, verification, and multi-step refactors. The project provides dependency-aware task graphs and incremental verification.

See README.md for install and `SYSTEM_PROMPT.md` for the tool loop.
