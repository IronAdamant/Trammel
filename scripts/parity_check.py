#!/usr/bin/env python3
"""Compare Rust decompose output with Python 3.15.3 on the five archetypes.

Step files, descriptions, symbols, relevance, rationale, fingerprint, and the
dependency edge set must match.

Tied implicit edges are an intentional difference. Python appends those edges
in set-iteration order, which depends on PYTHONHASHSEED and the OS. Rust keeps
a stable sort: higher confidence first, and equal confidences keep discovery
order from sorted maps. The edge set still matches. `test_farm` is the
archetype where that tie shows up; every other compared field matches exactly,
and a second Rust run reproduces the same list. `tests/determinism.rs` checks
the same bytes against a golden on every OS.
"""
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

from trammel.core import Planner
from trammel.store import RecipeStore

BIN = sys.argv[1] if len(sys.argv) > 1 else "target/release/trammel"
ARCHETYPES = [
    "legacy_ball_of_mud",
    "monorepo",
    "facade_service",
    "test_farm",
    "generated_code",
]
GOAL = "update shared helpers"
TMP = Path(tempfile.gettempdir())


def slash(value):
    """Python on Windows emits backslashes. Both sides are compared with `/`."""
    if isinstance(value, str):
        return value.replace("\\", "/")
    if isinstance(value, list):
        return [slash(item) for item in value]
    if isinstance(value, dict):
        return {slash(key): slash(item) for key, item in value.items()}
    return value


def rust(project: str, goal: str) -> dict:
    store = TMP / f"trammel-rs-{Path(project).name}"
    out = subprocess.check_output(
        [BIN, "decompose", "--goal", goal, "--project-root", project,
         "--suppress-creation-hints", "true", "--apply-project-config", "false",
         "--store", str(store)],
        text=True,
    )
    return json.loads(out)["result"]


def edge_sets(graph: dict) -> dict:
    return {k: set(v) for k, v in graph.items()}


def compare(name: str, project: str, goal: str) -> bool:
    db = TMP / f"trammel-parity-{name}.db"
    py = Planner(store=RecipeStore(str(db))).decompose(
        goal, project, skip_recipes=True, suppress_creation_hints=True, apply_project_config=False,
    )
    rs = rust(project, goal)
    py_files = slash([s["file"] for s in py["steps"]])
    rs_files = slash([s["file"] for s in rs["steps"]])
    checks = {
        "files": py_files == rs_files,
        "descriptions": slash([s["description"] for s in py["steps"]]) == slash([s["description"] for s in rs["steps"]]),
        "symbols": [s["symbols"] for s in py["steps"]] == [s["symbols"] for s in rs["steps"]],
        "relevance": [s.get("relevance") for s in py["steps"]] == [s.get("relevance") for s in rs["steps"]],
        "rationale": slash([s["rationale"] for s in py["steps"]]) == slash([s["rationale"] for s in rs["steps"]]),
        "fingerprint": py["goal_fingerprint"] == rs["goal_fingerprint"],
        "edge_set": edge_sets(slash(py["dependency_graph"])) == edge_sets(slash(rs["dependency_graph"])),
    }
    order = slash(py["dependency_graph"]) == slash(rs["dependency_graph"])
    if all(checks.values()):
        if name == "test_farm" and not order:
            again = rust(project, goal)
            if slash(again["dependency_graph"]) != slash(rs["dependency_graph"]):
                print("test_farm: FAIL rust edge order changed between runs")
                return False
            print("test_farm: match except tied-edge order (stable across rust runs; edge sets match python)")
            return True
        print(f"{name}: match" + ("" if order else " (edge order differs, edge set matches)"))
        return True
    bad = [k for k, ok in checks.items() if not ok]
    print(f"{name}: FAIL {bad}")
    return False


def main() -> int:
    failed = 0
    for name in ARCHETYPES:
        if not compare(name, f"tests/archetypes/{name}", GOAL):
            failed += 1
    extra = os.environ.get("PARITY_EXTRA", "")
    for item in extra.split(";"):
        if not item.strip():
            continue
        goal, project = item.split("|", 1)
        label = Path(project).name
        if not compare(label, project, goal):
            failed += 1
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
