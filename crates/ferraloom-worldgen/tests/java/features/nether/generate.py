#!/usr/bin/env python3
"""Run real feature placement on a controlled level; never open a saved world."""
import os
from pathlib import Path
import subprocess
import tempfile
import urllib.request

HERE = Path(__file__).resolve().parent
VANILLA = Path(os.environ.get('FERRALOOM_VANILLA_DIR', '/home/doc/code/Ferraloom/.cache/vanilla/26.3'))
JDK = Path(os.environ.get('JAVA_HOME', '/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1'))
with tempfile.TemporaryDirectory(prefix='ferraloom-nether-java-') as directory:
    directory = Path(directory)
    # Mockito supplies only a ServerLevel shell for entity constructors. Features,
    # blocks, providers, block entities, crystals and their RNG are real vanilla.
    artifacts = [
        ('org/mockito/mockito-core/5.20.0', 'mockito-core-5.20.0.jar'),
        ('net/bytebuddy/byte-buddy/1.17.7', 'byte-buddy-1.17.7.jar'),
        ('net/bytebuddy/byte-buddy-agent/1.17.7', 'byte-buddy-agent-1.17.7.jar'),
        ('org/objenesis/objenesis/3.3', 'objenesis-3.3.jar'),
    ]
    for path, name in artifacts:
        urllib.request.urlretrieve(f'https://repo.maven.apache.org/maven2/{path}/{name}', directory / name)
    classpath = ':'.join(map(str, [VANILLA / 'versions/26.3/server-26.3.jar', *sorted((VANILLA / 'libraries').rglob('*.jar')), *sorted(directory.glob('*.jar'))]))
    sources = [HERE.parent.parent / 'WorldgenGolden.java', HERE.parent / 'FeaturesGolden.java', HERE.parent / 'PlacementGolden.java', HERE.parent / 'StateProviderGolden.java', HERE.parent / 'SorterGolden.java', HERE / 'NetherEndGolden.java']
    subprocess.run([str(JDK / 'bin/javac'), '-cp', classpath, '-d', str(directory), *map(str, sources)], check=True, cwd=directory)
    out = HERE.parents[3] / 'src/feature/misc/testdata'
    out.mkdir(exist_ok=True)
    subprocess.run([str(JDK / 'bin/java'), '-Xmx1G', '-XX:+EnableDynamicAgentLoading', '-cp', str(directory) + ':' + classpath, 'NetherEndGolden', str(out), str(out / 'template.snbt')], check=True, cwd=directory)
