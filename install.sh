#!/bin/sh
# Install a published Trammel binary into ~/.local/bin. No root.
# Release assets are not published from the rewrite branch. This script
# refuses to guess a URL until a GitHub release exists.
set -eu
echo "trammel 4.0.0 has no GitHub release yet. Build from this checkout:" >&2
echo "  cargo build --release && cp target/release/trammel target/release/trammel-mcp \"\$HOME/.local/bin/\"" >&2
exit 1
