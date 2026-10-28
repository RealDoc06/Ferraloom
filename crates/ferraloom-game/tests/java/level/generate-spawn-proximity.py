#!/usr/bin/env python3
"""Capture settled natural-spawn distances and actual ChunkMap player predicates."""
from pathlib import Path
import subprocess
import tempfile

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
java = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
repo = source.parents[4]
with tempfile.TemporaryDirectory(prefix='ferraloom-spawn-proximity-java-') as tmp:
    classpath = ':'.join(map(str, [tmp, vanilla / 'versions/26.3/server-26.3.jar',
        *sorted((vanilla / 'libraries').rglob('*.jar')),
        vanilla / 'tree-harness/byte-buddy-1.15.4.jar']))
    subprocess.run([str(java / 'javac'), '-cp', classpath, '-d', tmp,
        str(source / 'LoadingVectors.java'), str(source / 'SpawnProximityVectors.java'),
        str(repo / 'crates/ferraloom-server/tests/java/generation_light/PublishedLightTrace.java')], check=True)
    subprocess.run([str(java / 'java'), '-Xmx2G', '-XX:ActiveProcessorCount=4',
        '-cp', classpath, 'SpawnProximityVectors', str(source / 'spawn-proximity.tsv')],
        cwd=tmp, check=True, timeout=120)
