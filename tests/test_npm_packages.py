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
        release, npm = workflow.split("  github-release:", 1)[1].split("  npm:", 1)
        self.assertNotIn("actions/checkout", release)
        for line in release.splitlines():
            stripped = line.strip()
            if stripped.startswith("#") or "gh " not in stripped:
                continue
            self.assertIn('--repo "$repo"', stripped)
        self.assertIn('scope: "@iron_adamant"', npm)
        self.assertIn("registry-url: https://registry.npmjs.org", npm)
        self.assertIn("NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}", npm)
        self.assertIn('npm publish "$dir" --access public', npm)
        platforms = npm.index("trammel-linux-x64")
        wrapper = npm.index("publish npm/trammel")
        self.assertLess(platforms, wrapper)
        self.assertIn('npm view "${name}@${version}"', npm)
        server = json.loads((ROOT / "server.json").read_text(encoding="utf-8"))
        self.assertEqual(server["packages"][0]["identifier"], f"{SCOPE}/trammel")
        self.assertEqual(server["packages"][0]["registry_type"], "npm")
