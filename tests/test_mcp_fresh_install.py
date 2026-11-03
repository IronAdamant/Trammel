"""Fresh-venv MCP handshake against the latest allowed SDK, and SDK 1.30.

Skipped unless ``TRAMMEL_FRESH_MCP_INSTALL=1`` (set on CI for Python 3.12).
The 1.30 pin runs only when ``TRAMMEL_FRESH_MCP_V1=1``.
"""

from __future__ import annotations

import json
import os
import pathlib
import queue
import subprocess
import sys
import tempfile
import threading
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]


def _pyproject_version() -> str:
    for line in (ROOT / "pyproject.toml").read_text(encoding="utf-8").splitlines():
        if line.startswith("version = "):
            return line.split("=", 1)[1].strip().strip('"')
    raise AssertionError("version is not declared in pyproject.toml")


class TestFreshMcpInstall(unittest.TestCase):
    """A clean install must answer initialize and tools/list."""

    def test_latest_allowed_mcp_handshake(self) -> None:
        if os.environ.get("TRAMMEL_FRESH_MCP_INSTALL") != "1":
            self.skipTest("set TRAMMEL_FRESH_MCP_INSTALL=1 to pip-install mcp and handshake")
        self._handshake([".[mcp]"], expect_major=2)

    def test_mcp_1_30_handshake(self) -> None:
        if os.environ.get("TRAMMEL_FRESH_MCP_V1") != "1":
            self.skipTest("set TRAMMEL_FRESH_MCP_V1=1 to handshake against mcp 1.30.0")
        self._handshake([".[mcp]", "mcp==1.30.0"], expect_prefix="1.30.")

    def _handshake(self, pip_args: list[str], expect_major: int | None = None, expect_prefix: str | None = None) -> None:
        declared = _pyproject_version()
        with tempfile.TemporaryDirectory(prefix="trammel-mcp-") as tmp:
            venv = pathlib.Path(tmp) / "venv"
            db_path = pathlib.Path(tmp) / "trammel.db"
            subprocess.run(
                [sys.executable, "-m", "venv", str(venv)],
                check=True,
                cwd=ROOT,
            )
            py = venv / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
            install = subprocess.run(
                [str(py), "-m", "pip", "install", "--disable-pip-version-check", *pip_args],
                cwd=ROOT,
                check=False,
                capture_output=True,
                text=True,
                timeout=300,
            )
            if install.returncode != 0:
                self.fail(
                    "pip install failed\n"
                    + install.stdout[-4000:]
                    + "\n"
                    + install.stderr[-4000:]
                )
            versions = subprocess.check_output(
                [
                    str(py),
                    "-c",
                    "import importlib.metadata as m; "
                    "print(m.version('mcp')); print(m.version('trammel'))",
                ],
                text=True,
            ).split()
            mcp_version, trammel_version = versions[0], versions[1]
            if expect_major is not None:
                self.assertEqual(int(mcp_version.split(".", 1)[0]), expect_major, mcp_version)
            if expect_prefix is not None:
                self.assertTrue(mcp_version.startswith(expect_prefix), mcp_version)
            self.assertEqual(trammel_version, declared)
            self.assertNotEqual(trammel_version, mcp_version)

            env = os.environ.copy()
            env.pop("PYTHONPATH", None)
            env["PYTHONUTF8"] = "1"
            env["PYTHONIOENCODING"] = "utf-8"
            env["TRAMMEL_DB_PATH"] = str(db_path)
            env["VIRTUAL_ENV"] = str(venv)
            proc = subprocess.Popen(
                [str(py), "-m", "trammel.mcp_stdio"],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                cwd=tmp,
                env=env,
            )
            assert proc.stdin and proc.stdout and proc.stderr
            messages: queue.Queue[str | None] = queue.Queue()
            stderr_parts: list[str] = []

            def _read_stdout() -> None:
                try:
                    assert proc.stdout is not None
                    for raw in iter(proc.stdout.readline, b""):
                        messages.put(raw.decode("utf-8"))
                finally:
                    messages.put(None)

            def _read_stderr() -> None:
                assert proc.stderr is not None
                for raw in iter(proc.stderr.readline, b""):
                    stderr_parts.append(raw.decode("utf-8", errors="replace"))

            threading.Thread(target=_read_stdout, daemon=True).start()
            threading.Thread(target=_read_stderr, daemon=True).start()
            try:
                self._speak(proc, messages, stderr_parts, trammel_version, mcp_version)
            finally:
                if proc.stdin and not proc.stdin.closed:
                    proc.stdin.close()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait(timeout=5)
                if proc.stdout:
                    proc.stdout.close()
                if proc.stderr:
                    proc.stderr.close()

    def _speak(
        self,
        proc: subprocess.Popen[bytes],
        messages: queue.Queue[str | None],
        stderr_parts: list[str],
        trammel_version: str,
        mcp_version: str,
    ) -> None:
        def send(payload: dict[str, object]) -> None:
            assert proc.stdin is not None
            proc.stdin.write((json.dumps(payload) + "\n").encode("utf-8"))
            proc.stdin.flush()

        def recv(timeout: float = 30) -> dict[str, object]:
            try:
                line = messages.get(timeout=timeout)
            except queue.Empty as exc:
                self.fail(f"timed out waiting for MCP message\n{''.join(stderr_parts)[-4000:]}")
                raise exc
            if line is None:
                self.fail(f"server exited\n{''.join(stderr_parts)[-4000:]}")
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                self.fail(f"non-JSON stdout: {line!r}\n{''.join(stderr_parts)[-4000:]}")
                raise

        send({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "trammel-fresh-install", "version": "0"},
            },
        })
        init = recv()
        if "error" in init and "2025-11-25" in json.dumps(init["error"]):
            send({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "trammel-fresh-install", "version": "0"},
                },
            })
            init = recv()
        self.assertNotIn("error", init, init)
        result = init["result"]
        self.assertIsInstance(result, dict)
        assert isinstance(result, dict)
        info = result.get("serverInfo")
        self.assertIsInstance(info, dict, result)
        assert isinstance(info, dict)
        self.assertEqual(info.get("name"), "trammel")
        self.assertEqual(info.get("version"), trammel_version)
        self.assertNotEqual(info.get("version"), mcp_version)
        self.assertTrue(info.get("version"))

        send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        send({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
        listed = recv()
        self.assertNotIn("error", listed, listed)
        tools_result = listed["result"]
        self.assertIsInstance(tools_result, dict)
        assert isinstance(tools_result, dict)
        tools = tools_result.get("tools")
        self.assertIsInstance(tools, list)
        assert isinstance(tools, list)
        names = {tool["name"] for tool in tools}
        self.assertIn("start_plan", names)
        self.assertIn("status", names)
        self.assertEqual(len(names), 12)

        send({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "status", "arguments": {}},
        })
        ok = recv()
        self.assertNotIn("error", ok, ok)
        ok_result = ok["result"]
        self.assertIsInstance(ok_result, dict)
        assert isinstance(ok_result, dict)
        self.assertIsNot(ok_result.get("isError"), True)
        content = ok_result.get("content")
        self.assertIsInstance(content, list)
        assert isinstance(content, list)
        body = json.loads(content[0]["text"])
        self.assertEqual(body["tools"], 33)
        self.assertEqual(body["primary_tool_count"], 12)

        send({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {"name": "not_a_real_tool", "arguments": {}},
        })
        bad = recv()
        self.assertNotIn("error", bad, bad)
        bad_result = bad["result"]
        self.assertIsInstance(bad_result, dict)
        assert isinstance(bad_result, dict)
        self.assertIs(bad_result.get("isError"), True)
        bad_content = bad_result.get("content")
        self.assertIsInstance(bad_content, list)
        assert isinstance(bad_content, list)
        self.assertIn("not_a_real_tool", bad_content[0]["text"])

        proc.stdin.close()  # type: ignore[union-attr]
