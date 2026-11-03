#!/usr/bin/env python3
"""Compare Rust decompose output with Python 3.15.3 on the five archetypes.

Step files, descriptions, symbols, relevance, rationale, fingerprint, and the
dependency edge set must match. Edge order among equal-confidence implicit
edges is reported and does not fail the run: CPython set order depends on
PYTHONHASHSEED, and the Rust side uses a stable sort.
"""
import json
import subprocess
import sys
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


def rust(project: str) -> dict:
    out = subprocess.check_output(
        [BIN, "decompose", "--goal", GOAL, "--project-root", project,
         "--suppress-creation-hints", "true", "--apply-project-config", "false",
         "--store", f"/tmp/trammel-rs-{Path(project).name}"],
        text=True,
    )
    return json.loads(out)["result"]


def edge_sets(graph: dict) -> dict:
    return {k: set(v) for k, v in graph.items()}


def main() -> int:
    failed = 0
    for name in ARCHETYPES:
        project = f"tests/archetypes/{name}"
        py = Planner(store=RecipeStore(f"/tmp/trammel-parity-{name}.db")).decompose(
            GOAL, project, skip_recipes=True, suppress_creation_hints=True, apply_project_config=False,
        )
        rs = rust(project)
        checks = {
            "files": [s["file"] for s in py["steps"]] == [s["file"] for s in rs["steps"]],
            "descriptions": [s["description"] for s in py["steps"]] == [s["description"] for s in rs["steps"]],
            "symbols": [s["symbols"] for s in py["steps"]] == [s["symbols"] for s in rs["steps"]],
            "relevance": [s.get("relevance") for s in py["steps"]] == [s.get("relevance") for s in rs["steps"]],
            "rationale": [s["rationale"] for s in py["steps"]] == [s["rationale"] for s in rs["steps"]],
            "fingerprint": py["goal_fingerprint"] == rs["goal_fingerprint"],
            "edge_set": edge_sets(py["dependency_graph"]) == edge_sets(rs["dependency_graph"]),
        }
        order = py["dependency_graph"] == rs["dependency_graph"]
        if all(checks.values()):
            print(f"{name}: match" + ("" if order else " (edge order differs, edge set matches)"))
        else:
            failed += 1
            bad = [k for k, ok in checks.items() if not ok]
            print(f"{name}: FAIL {bad}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
