"""npm packages are published under the iron_adamant scope."""

from __future__ import annotations

import json
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
NPM = ROOT / "npm"
SCOPE = "@iron_adamant"
PLATFORMS = (
    "trammel-linux-x64",
    "trammel-linux-arm64",
    "trammel-darwin-x64",
    "trammel-darwin-arm64",
    "trammel-windows-x64",
    "trammel-windows-arm64",
)


class TestNpmPackages(unittest.TestCase):
    def test_wrapper_name_commands_and_lookup(self) -> None:
        pkg = json.loads((NPM / "trammel" / "package.json").read_text(encoding="utf-8"))
        self.assertEqual(pkg["name"], f"{SCOPE}/trammel")
        self.assertEqual(pkg["version"], "4.0.0")
        self.assertEqual(pkg["bin"], {"trammel": "bin/trammel.js", "trammel-mcp": "bin/trammel-mcp.js"})
        self.assertEqual(
            pkg["optionalDependencies"],
            {f"{SCOPE}/{name}": "4.0.0" for name in PLATFORMS},
        )
        for script in ("trammel.js", "trammel-mcp.js"):
            text = (NPM / "trammel" / "bin" / script).read_text(encoding="utf-8")
            for name in PLATFORMS:
                self.assertIn(f"{SCOPE}/{name}", text)

    def test_platform_packages_are_scoped(self) -> None:
        for name in PLATFORMS:
            pkg = json.loads((NPM / name / "package.json").read_text(encoding="utf-8"))
            self.assertEqual(pkg["name"], f"{SCOPE}/{name}")
            self.assertEqual(pkg["version"], "4.0.0")
            self.assertIn("trammel", pkg["files"])
            self.assertIn("trammel-mcp", pkg["files"])

    def test_publish_is_public_and_registry_id_matches(self) -> None:
        workflow = (ROOT / ".github" / "workflows" / "release.yml").read_text(encoding="utf-8")
        self.assertIn('npm publish "${pkg}" --access public', workflow)
        self.assertIn("npm publish npm/trammel --access public", workflow)
        server = json.loads((ROOT / "server.json").read_text(encoding="utf-8"))
        self.assertEqual(server["packages"][0]["identifier"], f"{SCOPE}/trammel")
        self.assertEqual(server["packages"][0]["registry_type"], "npm")
