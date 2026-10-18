#!/usr/bin/env python3
"""Run independent checks on one committed sculk source snapshot on the builder."""
from pathlib import Path
import os
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
results = {}


def stage(name, command, env=None):
    print(f"SCULK_STAGE {name}", flush=True)
    code = subprocess.run(command, env=env).returncode
    results[name] = code
    print(f"SCULK_RESULT {name}={code}", flush=True)


with tempfile.TemporaryDirectory(prefix="ferraloom-sculk-check-") as directory:
    oracle = Path(directory) / "sculk.jsonl.gz"
    stage("oracle", [sys.executable, str(HERE / "sculk.py"), "--output", str(oracle)])
    env = os.environ.copy()
    env["RUST_TEST_NOCAPTURE"] = "1"
    if results["oracle"] == 0:
        env["FERRALOOM_SCULK_ORACLE"] = str(oracle)
    stage("tests", ["cargo", "test", "--workspace", "--locked", "--no-fail-fast"], env)
    stage("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"])
    stage("fmt", ["cargo", "fmt", "--all", "--", "--check"])
print("SCULK_GATE_RESULTS " + " ".join(f"{name}={code}" for name, code in results.items()), flush=True)
sys.exit(int(any(results.values())))
