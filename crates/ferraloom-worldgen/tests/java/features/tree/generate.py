#!/usr/bin/env python3
"""Compile our controlled-level harness against vanilla without starting a server."""
import os
from pathlib import Path
import subprocess
import tempfile
import urllib.request

MOCKS = [("org/mockito/mockito-core", "5.14.2"), ("net/bytebuddy/byte-buddy", "1.15.4"), ("net/bytebuddy/byte-buddy-agent", "1.15.4"), ("org/objenesis/objenesis", "3.3")]
HERE = Path(__file__).resolve().parent
VANILLA = Path(os.environ.get('FERRALOOM_VANILLA_DIR', '/home/doc/code/Ferraloom/.cache/vanilla/26.3'))
JDK = Path(os.environ.get('JAVA_HOME', '/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1'))
CACHE = VANILLA / 'tree-harness'
CACHE.mkdir(parents=True, exist_ok=True)
mock_jars = []
for artifact, version in MOCKS:
    filename = artifact.split('/')[-1] + '-' + version + '.jar'
    destination = CACHE / filename
    if not destination.exists():
        urllib.request.urlretrieve('https://repo.maven.apache.org/maven2/' + artifact + '/' + version + '/' + filename, destination)
    mock_jars.append(destination)
CLASSPATH = ':'.join(map(str, [VANILLA / 'versions/26.3/server-26.3.jar', *sorted((VANILLA / 'libraries').rglob('*.jar')), *mock_jars]))
with tempfile.TemporaryDirectory(prefix='ferraloom-tree-java-') as directory:
    sources = [HERE.parent.parent / 'WorldgenGolden.java', HERE.parent / 'FeaturesGolden.java', *sorted(HERE.parent.glob('*Golden.java')), HERE / 'TreeGolden.java']
    sources = list(dict.fromkeys(sources))
    subprocess.run([str(JDK / 'bin/javac'), '-cp', CLASSPATH, '-d', directory, *map(str, sources)], check=True, cwd=directory)
    subprocess.run([str(JDK / 'bin/java'), '-Xmx1G', '-Dnet.bytebuddy.experimental=true', '-javaagent:' + str(mock_jars[0]), '-cp', directory + ':' + CLASSPATH, 'TreeGolden', str(HERE.parent.parent.parent.parent / 'src/feature/tree/cases.json.gz')], check=True, cwd=directory)
