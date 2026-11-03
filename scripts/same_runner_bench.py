#!/usr/bin/env python3
"""Same-machine Python 3.15.3 vs Rust timings for the recorded baseline list.

Corpora are sdist trees passed as RICH and FLASK. Rust must be strictly faster
on every row. Writes bench/same_runner.json.
"""
import json
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PY = os.environ.get("TRAMMEL_PY", str(Path.home() / ".local/bin/trammel"))
PY_MCP = os.environ.get("TRAMMEL_PY_MCP", str(Path.home() / ".local/bin/trammel-mcp"))
RS = os.environ.get("TRAMMEL_RS", str(ROOT / "target/release/trammel"))
RICH = os.environ["RICH"]
FLASK = os.environ["FLASK"]
SELF = str(ROOT)
RUNS = int(os.environ.get("RUNS", "7"))
STARTUP_RUNS = int(os.environ.get("STARTUP_RUNS", "10"))
GOAL_RICH = "add a max_width option to Table rendering"
GOAL_FLASK = "add request timing logging to the app context"
PY_EXE = sys.executable
rows = []


def median(samples):
    return statistics.median(samples)


def record(name, py_samples, rs_samples, **extra):
    py_m = median(py_samples)
    rs_m = median(rs_samples)
    row = {
        "name": name,
        "python_median_ms": round(py_m, 2),
        "python_min_ms": round(min(py_samples), 2),
        "python_max_ms": round(max(py_samples), 2),
        "rust_median_ms": round(rs_m, 2),
        "rust_min_ms": round(min(rs_samples), 2),
        "rust_max_ms": round(max(rs_samples), 2),
        "rust_faster": rs_m < py_m,
        "runs": len(py_samples),
    }
    row.update(extra)
    rows.append(row)
    print(json.dumps(row), flush=True)
    return row


def timed(argv, runs, env=None, cwd=None):
    samples = []
    for _ in range(runs):
        t0 = time.perf_counter()
        proc = subprocess.run(argv, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        samples.append((time.perf_counter() - t0) * 1000)
        if proc.returncode not in (0, 2):
            raise SystemExit(f"{argv} -> {proc.returncode}\n{proc.stderr[-400:]!r}")
    return samples


class Session:
    def __init__(self, argv, env):
        self.p = subprocess.Popen(argv, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        self.buf = b""
        self.n = 0

    def request(self, method, params=None):
        self.n += 1
        body = json.dumps({"jsonrpc": "2.0", "id": self.n, "method": method, "params": params or {}}) + "\n"
        t0 = time.perf_counter()
        self.p.stdin.write(body.encode())
        self.p.stdin.flush()
        while b"\n" not in self.buf:
            chunk = self.p.stdout.read(1)
            if not chunk:
                raise RuntimeError("server closed")
            self.buf += chunk
            more = os.read(self.p.stdout.fileno(), 1 << 20) if False else b""
        # read the rest of the line already buffered plus any waiting bytes without blocking forever
        while b"\n" not in self.buf:
            self.buf += os.read(self.p.stdout.fileno(), 1 << 20)
        line, self.buf = self.buf.split(b"\n", 1)
        ms = (time.perf_counter() - t0) * 1000
        msg = json.loads(line)
        return ms, line, msg

    def call(self, tool, args):
        ms, line, msg = self.request("tools/call", {"name": tool, "arguments": args})
        result = msg.get("result", {})
        text = "".join(c.get("text", "") for c in result.get("content", []))
        err = bool(msg.get("error") or result.get("isError"))
        return ms, err, text

    def close(self):
        try:
            self.p.stdin.close()
        except Exception:
            pass
        self.p.kill()
        self.p.wait(timeout=5)


def read_line(proc, buf):
    while b"\n" not in buf:
        chunk = os.read(proc.stdout.fileno(), 1 << 20)
        if not chunk:
            raise RuntimeError("closed")
        buf += chunk
    line, buf = buf.split(b"\n", 1)
    return line, buf


def mcp_cold(argv, env, runs):
    cold, listed = [], []
    tools = None
    for _ in range(runs):
        t0 = time.perf_counter()
        proc = subprocess.Popen(argv, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        proc.stdin.write(b'{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"bench","version":"0"}}}\n')
        proc.stdin.flush()
        buf = b""
        line, buf = read_line(proc, buf)
        cold.append((time.perf_counter() - t0) * 1000)
        proc.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
        proc.stdin.flush()
        t1 = time.perf_counter()
        proc.stdin.write(b'{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}\n')
        proc.stdin.flush()
        line, buf = read_line(proc, buf)
        # the notification has no reply; the next line is tools/list
        if b"tools" not in line:
            line, buf = read_line(proc, buf)
        listed.append((time.perf_counter() - t1) * 1000)
        msg = json.loads(line)
        tools = len(msg["result"]["tools"])
        proc.kill()
        proc.wait(timeout=5)
    return cold, listed, tools


def hwm(pid):
    try:
        for line in open(f"/proc/{pid}/status"):
            if line.startswith("VmHWM"):
                return int(line.split()[1]) / 1024
    except OSError:
        return None
    return None


def flow(kind, db_arg_env):
    """One persistent session. Returns dict label -> [ms]."""
    per = {}
    order = []
    hwms = []
    for i in range(RUNS):
        if kind == "py":
            db = Path(tempfile.gettempdir()) / f"trammel-py-flow-{i}.db"
            for suf in ("", "-wal", "-shm"):
                p = Path(str(db) + suf)
                if p.exists():
                    p.unlink()
            env = dict(os.environ, TRAMMEL_DB_PATH=str(db), TRAMMEL_MCP_SURFACE="all")
            argv = [PY_MCP]
        else:
            db = Path(tempfile.gettempdir()) / f"trammel-rs-flow-{i}"
            shutil.rmtree(db, ignore_errors=True)
            env = dict(os.environ, TRAMMEL_DB_PATH=str(db), TRAMMEL_MCP_SURFACE="all")
            argv = [RS, "mcp"]
        proc = subprocess.Popen(argv, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        buf = b""
        def request(method, params=None):
            nonlocal buf
            request.n += 1
            body = json.dumps({"jsonrpc": "2.0", "id": request.n, "method": method, "params": params or {}}) + "\n"
            t0 = time.perf_counter()
            proc.stdin.write(body.encode())
            proc.stdin.flush()
            line, buf2 = read_line(proc, buf)
            buf = buf2
            return (time.perf_counter() - t0) * 1000, json.loads(line)
        request.n = 0
        request("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "bench", "version": "0"}})
        proc.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
        proc.stdin.flush()

        def rec(label, tool, args):
            ms, msg = request("tools/call", {"name": tool, "arguments": args})
            result = msg.get("result", {})
            text = "".join(c.get("text", "") for c in result.get("content", []))
            if msg.get("error") or result.get("isError"):
                print("ERR", kind, label, text[:400], file=sys.stderr)
            per.setdefault(label, []).append(ms)
            if label not in order:
                order.append(label)
            try:
                return json.loads(text)
            except Exception:
                return None

        rec("status empty", "status", {})
        rec("estimate rich", "estimate", {"project_root": RICH})
        rec("decompose rich", "decompose", {"goal": GOAL_RICH, "project_root": RICH})
        rec("decompose rich summary_only", "decompose", {"goal": GOAL_RICH, "project_root": RICH, "summary_only": True})
        rec("decompose flask", "decompose", {"goal": GOAL_FLASK, "project_root": FLASK})
        rec("explore rich", "explore", {"goal": GOAL_RICH, "project_root": RICH, "num_beams": 3})
        sp = rec("start_plan rich", "start_plan", {"goal": GOAL_RICH, "project_root": RICH}) or {}
        pid = sp.get("plan_id") or (sp.get("plan") or {}).get("plan_id") or (sp.get("plan") or {}).get("id")
        plan = rec("get_plan", "get_plan", {"plan_id": pid}) or {}
        steps = plan.get("steps") or (plan.get("strategy") or {}).get("steps") or []
        rec("resume", "resume", {"plan_id": pid})
        table = Path(RICH, "rich", "table.py").read_text(encoding="utf-8", errors="replace") + f"\n# bench edit {i}\n"
        rec("verify_step rich import", "verify_step", {
            "edits": [{"path": "rich/table.py", "content": table}],
            "project_root": RICH,
            "test_cmd": [PY_EXE, "-c", "import rich.table"],
        })
        con = Path(SELF, "trammel", "constraints.py").read_text(encoding="utf-8", errors="replace") + f"\n# bench edit {i}\n"
        rec("verify_step trammel tests", "verify_step", {
            "edits": [{"path": "trammel/constraints.py", "content": con}],
            "project_root": SELF,
            "test_cmd": [PY_EXE, "-m", "unittest", "discover", "-s", "tests", "-p", "test_*.py"],
        })
        if steps:
            sid = steps[0].get("id") or steps[0].get("step_id")
            if sid:
                rec("record_steps", "record_steps", {"steps": [{"step_id": sid, "status": "passed"}]})
        rec("add_constraint", "add_constraint", {"constraint_type": "avoid", "description": "do not touch rich/_windows.py", "context": {"file": "rich/_windows.py"}, "plan_id": pid})
        rec("get_constraints", "get_constraints", {})
        rec("export_plan", "export_plan", {"plan_id": pid})
        rec("complete_plan", "complete_plan", {"plan_id": pid, "outcome": True})
        rec("get_recipe", "get_recipe", {"goal": "add a max width option to Table rendering"})
        rec("list_plans", "list_plans", {})
        rec("status after", "status", {})
        peak = hwm(proc.pid)
        if peak:
            hwms.append(peak)
        proc.kill()
        proc.wait(timeout=5)
    return per, order, hwms


def main():
    py_env = dict(os.environ)
    rs_env = dict(os.environ)
    record("cli --version", timed([PY, "--version"], STARTUP_RUNS), timed([RS, "--version"], STARTUP_RUNS))
    record("cli --help", timed([PY, "--help"], RUNS), timed([RS, "--help"], RUNS))
    py_db = str(Path(tempfile.gettempdir()) / "trammel-py-cli.db")
    rs_store = str(Path(tempfile.gettempdir()) / "trammel-rs-cli")
    record("cli dry-run rich",
           timed([PY, GOAL_RICH, "--root", RICH, "--dry-run", "--db", py_db], RUNS),
           timed([RS, GOAL_RICH, "--root", RICH, "--dry-run", "--store", rs_store], RUNS))
    record("cli dry-run flask",
           timed([PY, GOAL_FLASK, "--root", FLASK, "--dry-run", "--db", py_db], RUNS),
           timed([RS, GOAL_FLASK, "--root", FLASK, "--dry-run", "--store", rs_store], RUNS))
    record("cli dry-run trammel-src",
           timed([PY, "add a timeout option to verify_step", "--root", SELF, "--dry-run", "--db", py_db], RUNS),
           timed([RS, "add a timeout option to verify_step", "--root", SELF, "--dry-run", "--store", rs_store], RUNS))
    record("cli full flask true",
           timed([PY, GOAL_FLASK, "--root", FLASK, "--db", py_db, "--test-cmd", "true"], 3),
           timed([RS, GOAL_FLASK, "--root", FLASK, "--store", rs_store, "--test-cmd", "true"], 3))

    for surface in ("primary", "all"):
        py_c, py_l, py_n = mcp_cold([PY_MCP], dict(os.environ, TRAMMEL_MCP_SURFACE=surface, TRAMMEL_DB_PATH=str(Path(tempfile.gettempdir())/"py-cold.db")), STARTUP_RUNS)
        rs_c, rs_l, rs_n = mcp_cold([RS, "mcp"], dict(os.environ, TRAMMEL_MCP_SURFACE=surface, TRAMMEL_DB_PATH=str(Path(tempfile.gettempdir())/f"rs-cold-{surface}")), STARTUP_RUNS)
        record(f"mcp[{surface}] initialize", py_c, rs_c, python_tools=py_n, rust_tools=rs_n)
        record(f"mcp[{surface}] tools/list", py_l, rs_l, python_tools=py_n, rust_tools=rs_n)

    py_per, order, py_hwm = flow("py", None)
    rs_per, _, rs_hwm = flow("rs", None)
    for label in order:
        if label in rs_per and len(py_per[label]) == len(rs_per[label]):
            record("mcp " + label, py_per[label], rs_per[label])
    if py_hwm and rs_hwm:
        record("mcp VmHWM MB", py_hwm, rs_hwm)

    out = ROOT / "bench" / "same_runner.json"
    out.write_text(json.dumps({"rows": rows}, indent=1) + "\n")
    slow = [r["name"] for r in rows if not r["rust_faster"]]
    print("SLOWER", slow or "none")
    if slow:
        sys.exit(1)


if __name__ == "__main__":
    main()
