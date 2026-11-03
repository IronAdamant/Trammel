#!/usr/bin/env python3
"""Time the Rust binary and fail if a tracked command is slower than the
recorded Python median. Also self-checks the regression rule by calling the
Rust benchstat tests (A/A passes, an injected slowdown fails).

Usage:
  python3 scripts/bench_gate.py target/release/trammel
  python3 scripts/bench_gate.py --self-test
"""
import json
import statistics
import subprocess
import sys
import time
from pathlib import Path

FLOORS = {
    "version": 78.1,
    "help": 78.7,
    "mcp-initialize": 306.6,
}

def time_ms(argv, runs):
    samples = []
    for _ in range(runs):
        t0 = time.perf_counter()
        proc = subprocess.run(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        samples.append((time.perf_counter() - t0) * 1000)
        if proc.returncode not in (0, 2):
            raise SystemExit(f"{argv} exited {proc.returncode}")
    return samples

def mcp_init(argv, runs):
    samples = []
    for _ in range(runs):
        t0 = time.perf_counter()
        proc = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        proc.stdin.write(b'{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"bench","version":"0"}}}\n')
        proc.stdin.flush()
        line = proc.stdout.readline()
        elapsed = (time.perf_counter() - t0) * 1000
        proc.kill()
        if b"trammel" not in line:
            raise SystemExit("mcp initialize did not answer")
        samples.append(elapsed)
    return samples

def main():
    if "--self-test" in sys.argv:
        subprocess.check_call(["cargo", "test", "--offline", "--lib", "benchstat", "--", "--test-threads", "8"])
        print("A/A passes and injected slowdown fails")
        return
    binary = sys.argv[1]
    runs = 10
    results = {
        "version": time_ms([binary, "--version"], runs),
        "help": time_ms([binary, "--help"], runs),
        "mcp-initialize": mcp_init([binary, "mcp"], runs),
    }
    # mcp is the same binary with argv0; call `trammel mcp` is slower. Use the dedicated binary when present.
    report = {}
    failed = False
    for name, samples in results.items():
        med = statistics.median(samples)
        floor = FLOORS[name]
        report[name] = {"median_ms": round(med, 2), "min_ms": round(min(samples), 2), "max_ms": round(max(samples), 2), "python_median_ms": floor, "beats_python": med < floor}
        if med >= floor:
            failed = True
        print(json.dumps({"name": name, **report[name]}))
    Path("bench").mkdir(exist_ok=True)
    Path("bench/rust_measured.json").write_text(json.dumps(report, indent=1) + "\n", encoding="utf-8")
    if failed:
        sys.exit(1)

if __name__ == "__main__":
    main()
