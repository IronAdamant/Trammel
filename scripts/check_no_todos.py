#!/usr/bin/env python3
"""Fail on leftover TODO or FIXME markers in the Rust tree."""
import sys
from pathlib import Path

hits = []
for path in Path("src").rglob("*.rs"):
    for n, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
        if "TODO" in line or "FIXME" in line:
            # The harness looks for these tokens in project source. That is the
            # detector, not a leftover task.
            if ".matches(" in line or "markers" in line:
                continue
            hits.append(f"{path}:{n}:{line.strip()}")
if hits:
    print("\n".join(hits))
    sys.exit(1)
print("no leftover markers")
