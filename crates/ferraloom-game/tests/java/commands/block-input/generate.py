#!/usr/bin/env python3
"""Capture real BlockInput placement and canonical BE state without a server boot."""
from pathlib import Path
import subprocess

cache = Path('/home/doc/code/Ferraloom/.cache')
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/block-input-vectors'
classes.mkdir(parents=True, exist_ok=True)
source = Path(__file__).resolve().parent
cp = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar',
    *sorted((vanilla / 'libraries').rglob('*.jar')),
    *sorted((vanilla / 'tree-harness').rglob('*.jar'))]))
subprocess.run([str(jdk / 'javac'), '-cp', cp, '-d', str(classes),
    str(source / 'BlockInputVectors.java')], check=True)
subprocess.run([str(jdk / 'java'), '-Xmx1g', '-Dnet.bytebuddy.experimental=true',
    '-XX:+EnableDynamicAgentLoading', '-cp', f'{classes}:{cp}',
    'BlockInputVectors', str(source / 'vectors.json')], cwd=vanilla, check=True)
