#!/usr/bin/env python3
"""Run authored structure scenarios against the official jar on devbox."""
import os
from pathlib import Path
import subprocess
import tempfile
import urllib.request

HERE = Path(__file__).resolve().parent
VANILLA = Path(os.environ.get("FERRALOOM_VANILLA_DIR", "/home/doc/code/Ferraloom/.cache/vanilla/26.3"))
JDK = Path(os.environ.get("JAVA_HOME", "/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1"))
# Strict mocks provide only the level/manager boundary for the real locate code.
CACHE = VANILLA / "tree-harness"
CACHE.mkdir(parents=True, exist_ok=True)
MOCKS = [("org/mockito/mockito-core", "5.14.2"), ("net/bytebuddy/byte-buddy", "1.15.4"), ("net/bytebuddy/byte-buddy-agent", "1.15.4"), ("org/objenesis/objenesis", "3.3")]
mock_jars = []
for artifact, version in MOCKS:
    filename = artifact.split("/")[-1] + "-" + version + ".jar"
    destination = CACHE / filename
    if not destination.exists():
        urllib.request.urlretrieve("https://repo.maven.apache.org/maven2/" + artifact + "/" + version + "/" + filename, destination)
    mock_jars.append(destination)
CLASSPATH = ":".join(map(str, [VANILLA / "versions/26.3/server-26.3.jar", *sorted((VANILLA / "libraries").rglob("*.jar")), *mock_jars]))
JAVA_FLAGS = ["-Dnet.bytebuddy.experimental=true", "-javaagent:" + str(mock_jars[0])]

with tempfile.TemporaryDirectory(prefix="ferraloom-structures-java-") as directory:
    subprocess.run([str(JDK / "bin/javac"), "-cp", CLASSPATH, "-d", directory, *map(str, sorted(HERE.glob("*.java")))], check=True, cwd=directory)
    subprocess.run([str(JDK / "bin/java"), "-Xmx1G", *JAVA_FLAGS, "-cp", directory + ":" + CLASSPATH, "StructureGolden", str(HERE.parents[2] / "src/structure/testdata")], check=True, cwd=directory)
