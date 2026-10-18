#!/usr/bin/env python3
"""Run real 26.3 feature placement in memory; never opens a live world."""
import os
from pathlib import Path
import subprocess
import tempfile
import urllib.request
HERE = Path(__file__).resolve().parent
VANILLA = Path(os.environ.get("FERRALOOM_VANILLA_DIR", "/home/doc/code/Ferraloom/.cache/vanilla/26.3"))
JDK = Path(os.environ.get("JAVA_HOME", "/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1"))
JAR = VANILLA / "versions/26.3/server-26.3.jar"
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
CP = ":".join(map(str, [JAR, *sorted((VANILLA / "libraries").rglob("*.jar")), *mock_jars]))
with tempfile.TemporaryDirectory(prefix="ferraloom-orecave-") as directory:
    sources = [HERE.parents[1]/"WorldgenGolden.java", HERE/"OreCaveGolden.java"]
    subprocess.run([str(JDK/"bin/javac"),"-cp",CP,"-d",directory,*map(str,sources)],check=True,cwd=directory)
    subprocess.run([str(JDK/"bin/java"),"-Xmx1G","-Dnet.bytebuddy.experimental=true","-javaagent:"+str(mock_jars[0]),"-cp",directory+":"+CP,"OreCaveGolden",str(JAR),str(HERE.parents[3]/"src/feature/cave/vanilla.json.gz")],check=True,cwd=directory)
