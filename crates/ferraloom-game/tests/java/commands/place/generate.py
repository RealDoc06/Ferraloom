#!/usr/bin/env python3
"""Capture real PlaceCommand vectors against the cached native 26.3 classes."""
from pathlib import Path
import subprocess

cache = Path('/home/doc/code/Ferraloom/.cache')
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
source = Path(__file__).resolve().parent
classes = cache / 'tools/place-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar',
    *sorted((vanilla / 'libraries').rglob('*.jar')),
    *sorted((vanilla / 'tree-harness').rglob('*.jar'))]))
subprocess.run([str(jdk / 'javac'), '-cp', cp, '-d', str(classes),
    str(source / 'PlaceVectors.java'), str(source.parents[1] / 'loot/LootVectors.java')], check=True)
subprocess.run([str(jdk / 'java'), '-Xmx1g', '-Dnet.bytebuddy.experimental=true',
    '-XX:+EnableDynamicAgentLoading', '-cp', f'{classes}:{cp}', 'PlaceVectors',
    str(source / 'vectors.json')], cwd=vanilla, check=True)
