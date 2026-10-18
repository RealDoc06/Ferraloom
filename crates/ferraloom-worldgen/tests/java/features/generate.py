#!/usr/bin/env python3
"""Compile our feature harness against the real jar on devbox, without world IO."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
VANILLA = Path(os.environ.get("FERRALOOM_VANILLA_DIR", "/home/doc/code/Ferraloom/.cache/vanilla/26.3"))
JDK = Path(os.environ.get("JAVA_HOME", "/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1"))
CLASSPATH = ":".join(map(str, [VANILLA / "versions/26.3/server-26.3.jar", *sorted((VANILLA / "libraries").rglob("*.jar"))]))
with tempfile.TemporaryDirectory(prefix="ferraloom-features-java-") as directory:
    sources = [HERE.parent / "WorldgenGolden.java", *sorted(HERE.glob("*.java"))]
    subprocess.run([str(JDK / "bin/javac"), "-cp", CLASSPATH, "-d", directory, *map(str, sources)], check=True, cwd=directory)
    subprocess.run([str(JDK / "bin/java"), "-Xmx1G", "-cp", directory + ":" + CLASSPATH, "FeaturesGolden", str(HERE.parent.parent / "golden"), *sys.argv[1:]], check=True, cwd=directory)
