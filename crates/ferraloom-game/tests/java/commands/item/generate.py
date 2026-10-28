#!/usr/bin/env python3
"""Run the native SlotCollection and modifier operation ordering."""
from pathlib import Path
import subprocess
cache = Path('/home/doc/code/Ferraloom/.cache')
source = Path(__file__).resolve().parent
vanilla = cache / 'vanilla/26.3'
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
classes = cache / 'tools/item-slot-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar',
    *sorted((vanilla / 'libraries').rglob('*.jar')),
    *sorted((vanilla / 'tree-harness').rglob('*.jar'))]))
subprocess.run([str(jdk / 'javac'), '-cp', cp, '-d', str(classes), str(source / 'SlotVectors.java')], check=True)
subprocess.run([str(jdk / 'java'), '-Xmx1g', '-Dnet.bytebuddy.experimental=true', '-XX:+EnableDynamicAgentLoading', '-cp', f'{classes}:{cp}', 'SlotVectors', str(source / 'vectors.json')], cwd=vanilla, check=True)
