#!/usr/bin/env python3
"""Generate exhaustive, uncommitted real-jar vectors for the ignored Rust comparison."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
VANILLA = Path(os.environ.get("FERRALOOM_VANILLA_DIR", "/home/doc/code/Ferraloom/.cache/vanilla/26.3"))
JDK = Path(os.environ.get("JAVA_HOME", "/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1"))
CLASSPATH = os.pathsep.join(map(str, [VANILLA / "versions/26.3/server-26.3.jar", *sorted((VANILLA / "libraries").rglob("*.jar"))]))
with tempfile.TemporaryDirectory(prefix="ferraloom-template-java-") as directory:
    subprocess.run([str(JDK / "bin/javac"), "-cp", CLASSPATH, "-d", directory, str(HERE / "TemplateParityGolden.java")], check=True, cwd=directory)
    subprocess.run([str(JDK / "bin/java"), "-Xmx1G", "-cp", directory + os.pathsep + CLASSPATH, "TemplateParityGolden", str(Path(sys.argv[1]).resolve())], check=True, cwd=directory)
