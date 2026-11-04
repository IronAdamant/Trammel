#!/usr/bin/env bash
# Reminder helper: check Cargo.toml version, then print the tag command.
# Pushing a v* tag runs .github/workflows/release.yml (binaries, GitHub Release, npm).
# This script does not tag, publish, or upload anything.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
V="${1:?Usage: $0 X.Y.Z   (must match version in Cargo.toml)}"
if ! grep -q "^version = \"$V\"" Cargo.toml; then
  echo "error: Cargo.toml version is not $V. Edit it first." >&2
  exit 1
fi
echo "OK: Cargo.toml version is $V"
echo "Next, from the release line, after the commit is on main:"
echo "  git tag -a v$V -m \"Release $V\" && git push origin v$V"
echo "That tag runs Release: six binaries, SHA256SUMS, a GitHub Release, then npm."
echo "workflow_dispatch with tag v$V republishes npm from an existing release."
echo "Do not run this helper as a publish step."
