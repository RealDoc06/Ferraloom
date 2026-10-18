#!/usr/bin/env python3
"""Heavy, explicitly gated normal-preset captures. Run only on the builder."""
import os
import base64
from pathlib import Path
import subprocess
import sys
import tempfile

if os.environ.get("FERRALOOM_CAPTURE_GENERATED_STRUCTURES") != "1":
    raise SystemExit("Set FERRALOOM_CAPTURE_GENERATED_STRUCTURES=1 on the builder to capture real generated chunks")
HERE = Path(__file__).resolve().parent
scope = {"__file__": str(HERE / "generate.py")}
exec((HERE / "generate.py").read_text().split("with tempfile.TemporaryDirectory")[0], scope)
with tempfile.TemporaryDirectory(prefix="ferraloom-generated-structures-java-") as directory:
    subprocess.run([str(scope["JDK"] / "bin/javac"), "-cp", scope["CLASSPATH"], "-d", directory, *map(str, sorted(HERE.glob("*.java")))], check=True, cwd=directory)
    result = subprocess.run([str(scope["JDK"] / "bin/java"), "-Xmx4G", "-XX:ActiveProcessorCount=4", "-cp", directory + ":" + scope["CLASSPATH"], "GeneratedChunksGolden", str(Path(directory) / "captures")], cwd=directory)

    # Export authored goldens through the wrapper's stdout. The builder checkout
    # remains untouched; the caller decodes these records into its own worktree.
    for path in sorted((Path(directory) / "captures").glob("*.json.gz")):
        print("FERRALOOM_STRUCTURE_FIXTURE " + path.name + " " + base64.b64encode(path.read_bytes()).decode(), flush=True)

    # Keep completed dimensions/cases useful if a later capture fails, while
    # preserving the Java failure for the gate.
    sys.exit(result.returncode)
