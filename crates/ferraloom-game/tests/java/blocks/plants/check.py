#!/usr/bin/env python3
"""Builder-only, independent checks against one committed source snapshot."""
from pathlib import Path
import os
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
results = {}


def stage(name, command, env=None):
    print(f"PLANTS_STAGE {name}", flush=True)
    result = subprocess.run(command, env=env).returncode
    results[name] = result
    print(f"PLANTS_RESULT {name}={result}", flush=True)
    return result


with tempfile.TemporaryDirectory(prefix="ferraloom-plants-check-") as directory:
    oracle = Path(directory) / "native.tsv.gz"
    stage("oracle", [sys.executable, str(HERE / "random_ticks.py"),
                     "--extended", "--output", str(oracle)])
    baseline_env = os.environ.copy()
    baseline_env.pop("FERRALOOM_PLANTS_ORACLE", None)
    baseline_env["RUST_TEST_NOCAPTURE"] = "1"
    stage("tests", ["cargo", "test", "--workspace", "--locked", "--no-fail-fast"],
          baseline_env)
    stage("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked",
                     "--", "-D", "warnings"])
    stage("fmt", ["cargo", "fmt", "--all", "--", "--check"])
    if results["oracle"] == 0:
        native_env = baseline_env | {"FERRALOOM_PLANTS_ORACLE": str(oracle)}
        stage("native-replay", ["cargo", "test", "-p", "ferraloom-game", "--locked",
                                "native_live_oracle", "--", "--ignored", "--nocapture"],
              native_env)
    else:
        results["native-replay"] = 1
    print("PLANTS_GATE_RESULTS " + " ".join(f"{name}={code}" for name, code in results.items()),
          flush=True)
sys.exit(int(any(results.values())))
