#!/usr/bin/env python3
"""Run real vanilla feature placement on an unlit controlled level; no world IO."""
from pathlib import Path
import os
import subprocess
import tempfile
import urllib.request

HERE = Path(__file__).resolve().parent
VANILLA = Path(os.environ.get("FERRALOOM_VANILLA_DIR", "/home/doc/code/Ferraloom/.cache/vanilla/26.3"))
JDK = Path(os.environ.get("JAVA_HOME", "/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1"))
CACHE = VANILLA / "tree-harness"
CACHE.mkdir(parents=True, exist_ok=True)
MOCKS = [("org/mockito/mockito-core", "5.14.2"), ("net/bytebuddy/byte-buddy", "1.15.4"), ("net/bytebuddy/byte-buddy-agent", "1.15.4"), ("org/objenesis/objenesis", "3.3")]
mock_jars = []
for artifact, version in MOCKS:
    filename = artifact.split('/')[-1] + '-' + version + '.jar'
    destination = CACHE / filename
    if not destination.exists():
        urllib.request.urlretrieve('https://repo.maven.apache.org/maven2/' + artifact + '/' + version + '/' + filename, destination)
    mock_jars.append(destination)
CLASSPATH = ":".join(map(str, [VANILLA / "versions/26.3/server-26.3.jar", *sorted((VANILLA / "libraries").rglob("*.jar")), *mock_jars]))
OUTPUT = HERE.parents[3] / "src/feature/vegetation/cases.json.gz"
with tempfile.TemporaryDirectory(prefix="ferraloom-vegetation-java-") as directory:
    sources = [HERE.parents[1] / "WorldgenGolden.java", *sorted(HERE.glob("*.java"))]
    subprocess.run([str(JDK / "bin/javac"), "-cp", CLASSPATH, "-d", directory, *map(str, sources)], check=True, cwd=directory)
    subprocess.run([str(JDK / "bin/java"), "-Xmx1G", "-Dnet.bytebuddy.experimental=true", "-javaagent:" + str(mock_jars[0]), "-cp", directory + ":" + CLASSPATH, "VegetationGolden", str(OUTPUT)], check=True, cwd=directory)
