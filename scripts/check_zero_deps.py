#!/usr/bin/env python3
"""Fail if Cargo.lock names any package other than trammel."""
import re
import sys
from pathlib import Path

text = Path("Cargo.lock").read_text(encoding="utf-8")
names = re.findall(r'^name = "([^"]+)"', text, re.M)
extra = sorted({n for n in names if n != "trammel"})
if extra:
    print("Cargo.lock lists packages other than trammel:", ", ".join(extra))
    sys.exit(1)
toml = Path("Cargo.toml").read_text(encoding="utf-8")
section = toml.split("[dependencies]", 1)[-1].split("[", 1)[0]
if section.strip():
    print("[dependencies] is not empty")
    sys.exit(1)
print("zero dependencies")
