#!/usr/bin/env python3
"""Run our numeric shape harness on the shared, unmodified vanilla 26.3 jar.

Only javac/java run on devbox. Rust builds/tests use ferraloom-build.
The output is test data, not Java classes or Minecraft binaries.
"""
import os
from pathlib import Path
import subprocess
import tempfile
import shutil
import sys

HERE = Path(__file__).resolve().parent
VANILLA = Path(os.environ.get("FERRALOOM_VANILLA_DIR", "/home/doc/code/Ferraloom/.cache/vanilla/26.3"))
JDK = Path(os.environ.get("JAVA_HOME", "/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1"))
CLASSPATH = ":".join(map(str, [VANILLA / "versions/26.3/server-26.3.jar", *sorted((VANILLA / "libraries").rglob("*.jar"))]))
with tempfile.TemporaryDirectory(prefix="ferraloom-shapes-java-") as directory:
    subprocess.run([str(JDK / "bin/javac"), "-cp", CLASSPATH, "-d", directory, str(HERE / "ShapeVectors.java")], check=True, cwd=directory)
    output = Path(directory) / "vanilla.bin"
    registry = "--registry-caches" in sys.argv[1:]
    command = [str(JDK / "bin/java"), "-Xmx768m", "-cp", directory + ":" + CLASSPATH, "ShapeVectors", str(output)]
    if registry:
        command.append("--registry-caches")
    subprocess.run(command, check=True, cwd=directory)
    target = HERE / ("../../../../ferraloom-data/tests/vanilla_voxel_shapes.bin" if registry else "../../../src/shapes/vanilla.bin")
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(output, target)
