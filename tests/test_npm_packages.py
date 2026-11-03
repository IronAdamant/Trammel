"""The npm wrapper is trammel-mcp. The registry name trammel is taken."""

from __future__ import annotations

import json
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
NPM = ROOT / "npm"
PLATFORMS = (
    "trammel-linux-x64",
    "trammel-linux-arm64",
    "trammel-darwin-x64",
    "trammel-darwin-arm64",
    "trammel-windows-x64",
    "trammel-windows-arm64",
)


class TestNpmPackages(unittest.TestCase):
    def test_wrapper_name_and_commands(self) -> None:
        self.assertFalse((NPM / "trammel").exists())
        pkg = json.loads((NPM / "trammel-mcp" / "package.json").read_text(encoding="utf-8"))
        self.assertEqual(pkg["name"], "trammel-mcp")
        self.assertEqual(pkg["version"], "4.0.0")
        self.assertEqual(pkg["bin"], {"trammel": "bin/trammel.js", "trammel-mcp": "bin/trammel-mcp.js"})
        self.assertEqual(pkg["optionalDependencies"], {name: "4.0.0" for name in PLATFORMS})

    def test_platform_packages_keep_their_names(self) -> None:
        for name in PLATFORMS:
            pkg = json.loads((NPM / name / "package.json").read_text(encoding="utf-8"))
            self.assertEqual(pkg["name"], name)
            self.assertEqual(pkg["version"], "4.0.0")
            self.assertIn("trammel", pkg["files"])
            self.assertIn("trammel-mcp", pkg["files"])

    def test_publish_paths_and_registry_id(self) -> None:
        workflow = (ROOT / ".github" / "workflows" / "release.yml").read_text(encoding="utf-8")
        self.assertIn("npm/trammel-mcp", workflow)
        self.assertNotIn("prefix npm/trammel\n", workflow)
        self.assertNotIn("publish npm/trammel ", workflow)
        server = json.loads((ROOT / "server.json").read_text(encoding="utf-8"))
        self.assertEqual(server["packages"][0]["identifier"], "trammel-mcp")
        self.assertEqual(server["packages"][0]["registry_type"], "npm")
