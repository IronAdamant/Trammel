#!/usr/bin/env python3
"""Check the vendored parser files against scripts/syntax_core_manifest.txt.

The manifest is the byte digest of the stele-context parser core copied from
commit 244561a491ac4002b2c4bc94a3884091870a6d11 (pull/5), plus the newline
index in `src/lang/python_symbols/`. A second mode, `--against DIR`, diffs
those paths against a stele checkout and will report the two symbol files.
"""
import hashlib
import sys
from pathlib import Path

manifest = Path("scripts/syntax_core_manifest.txt").read_text(encoding="utf-8").splitlines()
bad = []
for line in manifest:
    if not line.strip():
        continue
    digest, path = line.split(None, 1)
    file = Path(path)
    if not file.is_file():
        bad.append(f"missing {path}")
        continue
    # Windows checkouts may rewrite LF as CRLF. The manifest is the LF digest.
    got = hashlib.sha256(file.read_bytes().replace(b"\r\n", b"\n")).hexdigest()
    if got != digest:
        bad.append(f"changed {path}")
if bad:
    print("\n".join(bad))
    sys.exit(1)
print(f"syntax core manifest ok ({len(manifest)} files)")

if len(sys.argv) == 3 and sys.argv[1] == "--against":
    root = Path(sys.argv[2])
    for line in manifest:
        if not line.strip():
            continue
        _, path = line.split(None, 1)
        rel = path[len("src/"):] if path.startswith("src/") else path
        other = root / "src" / rel if not path.startswith("src/json") and not path.startswith("src/hash") else root / path
        # json.rs and hash.rs live at src/ in both trees; lang and syntax too.
        other = root / path
        if other.read_bytes() != Path(path).read_bytes():
            print(f"differs from stele: {path}")
            sys.exit(1)
    print("syntax core matches", root)
