#!/usr/bin/env python3
"""Compile our Java harness against the real 26.3 jar. Run on devbox only."""
import os
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
VANILLA = Path(os.environ.get("FERRALOOM_VANILLA_DIR", "/home/doc/code/Ferraloom/.cache/vanilla/26.3"))
JDK = Path(os.environ.get("JAVA_HOME", "/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1"))
CLASSPATH = ":".join(map(str, [VANILLA / "versions/26.3/server-26.3.jar", *sorted((VANILLA / "libraries").rglob("*.jar"))]))
with tempfile.TemporaryDirectory(prefix="ferraloom-worldgen-java-") as directory:
    subprocess.run([str(JDK / "bin/javac"), "-cp", CLASSPATH, "-d", directory, str(HERE / "WorldgenGolden.java")], check=True, cwd=directory)
    subprocess.run([str(JDK / "bin/java"), "-Xmx1G", "-cp", directory + ":" + CLASSPATH, "WorldgenGolden", str(HERE.parent / "golden")], check=True, cwd=directory)
