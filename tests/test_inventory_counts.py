"""Lock public inventory counts to what the code actually creates."""

from __future__ import annotations

import os
import pathlib
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from trammel.analyzer_engine import (  # noqa: E402
    CppAnalyzer,
    CSharpAnalyzer,
    DartAnalyzer,
    GoAnalyzer,
    JavaAnalyzer,
    PhpAnalyzer,
    RubyAnalyzer,
    RustAnalyzer,
    SwiftAnalyzer,
    ZigAnalyzer,
)
from trammel.analyzer_specs import SPEC_REGISTRY  # noqa: E402
from trammel.analyzers import JavaScriptAnalyzer, PythonAnalyzer, TypeScriptAnalyzer  # noqa: E402
from trammel.store import RecipeStore  # noqa: E402
from trammel.tool_schemas import LANGUAGES, PRIMARY_TOOLS, TOOL_SCHEMAS  # noqa: E402

SQLITE_TABLES = {
    "constraints",
    "failure_patterns",
    "plans",
    "recipe_arch_signatures",
    "recipe_files",
    "recipe_index_meta",
    "recipe_signatures",
    "recipe_terms",
    "recipe_trigrams",
    "recipes",
    "scaffold_recipes",
    "scaffold_trigrams",
    "steps",
    "trajectories",
    "usage_events",
}

REGEX_ANALYZERS = (
    TypeScriptAnalyzer,
    JavaScriptAnalyzer,
    GoAnalyzer,
    RustAnalyzer,
    CppAnalyzer,
    JavaAnalyzer,
    CSharpAnalyzer,
    RubyAnalyzer,
    PhpAnalyzer,
    SwiftAnalyzer,
    DartAnalyzer,
    ZigAnalyzer,
)


def _section(text: str, start: str, end: str) -> str:
    begin = text.index(start)
    return text[begin:text.index(end, begin)]


class TestInventoryCounts(unittest.TestCase):
    def test_code_counts(self) -> None:
        self.assertEqual(len(TOOL_SCHEMAS), 33)
        self.assertEqual(len(PRIMARY_TOOLS), 12)
        self.assertEqual(len(LANGUAGES), 15)
        self.assertEqual(len(SPEC_REGISTRY), 12)
        self.assertEqual(len(REGEX_ANALYZERS), 12)
        self.assertEqual(PythonAnalyzer.name, "python")
        with tempfile.TemporaryDirectory() as tmp:
            with RecipeStore(os.path.join(tmp, "t.db")) as store:
                rows = store.conn.execute(
                    "SELECT name FROM sqlite_master "
                    "WHERE type='table' AND name NOT LIKE 'sqlite_%'"
                ).fetchall()
        names = {row["name"] for row in rows}
        self.assertEqual(names, SQLITE_TABLES)
        self.assertEqual(len(names), 15)

    def test_current_docs_match_code_counts(self) -> None:
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        layout = _section(readme, "## Project Layout", "## SQLite Schema")
        schema = _section(readme, "## SQLite Schema", "## Contributing")
        self.assertIn("(15 tables)", layout)
        self.assertIn("(33 tools)", layout)
        self.assertIn("SPEC_REGISTRY: 12 entries", layout)
        self.assertNotIn("8 tables", layout)
        self.assertNotIn("32 tools", layout)
        self.assertNotIn("13 regex", layout)
        for name in sorted(SQLITE_TABLES):
            self.assertIn(f"`{name}`", schema)

        claude = (ROOT / "CLAUDE.md").read_text(encoding="utf-8")
        self.assertIn("33 tools", claude)
        self.assertIn("SPEC_REGISTRY`, 12 entries", claude)
        self.assertNotIn("SPEC_REGISTRY`, 15 languages", claude)

        complete = (ROOT / "COMPLETE_PROJECT_DOCUMENTATION.md").read_text(encoding="utf-8")
        body = complete.split("## Changelog", 1)[0]
        self.assertIn("33 tools", body)
        self.assertIn("15 tables", body)
        self.assertIn("12 entries", body)
        self.assertNotIn("31 tools", body)
        self.assertNotIn("31 handlers", body)
        self.assertNotIn("13 regex", body)
        self.assertIn('`trammel` 3.15.3', body)

        changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
        self.assertIn("## 3.15.3", changelog)
        pyproject = (ROOT / "pyproject.toml").read_text(encoding="utf-8")
        self.assertIn('version = "3.15.3"', pyproject)
        self.assertIn('"mcp>=1.0.0,<3"', pyproject)
