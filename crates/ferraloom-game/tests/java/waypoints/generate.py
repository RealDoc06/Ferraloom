#!/usr/bin/env python3
"""Capture native waypoint attribute observers against the pinned 26.3 jar."""
from pathlib import Path
import subprocess
import sys

source = Path(__file__).resolve().parent
root = source.parents[4]
cache = Path('/home/doc/code/Ferraloom/.cache')
vanilla = cache / 'vanilla/26.3'
java = cache / 'toolchains/jdk-25.0.4.1+1/bin'
classes = cache / 'tools/waypoint-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar',
                       *sorted((vanilla / 'libraries').rglob('*.jar')),
                       *sorted((vanilla / 'tree-harness').rglob('*.jar'))]))
subprocess.run([str(java / 'javac'), '-cp', cp, '-d', str(classes),
                str(root / 'crates/ferraloom-game/tests/java/advancements/AdvancementVectors.java'),
                str(source / 'AttributeUpdateVectors.java')], check=True)
output = classes / 'attribute-updates.tsv'
subprocess.run([str(java / 'java'), '-Xmx768m', '-XX:+EnableDynamicAgentLoading',
                '-Dnet.bytebuddy.experimental=true', '-cp', f'{classes}:{cp}',
                'AttributeUpdateVectors', str(output)], cwd=vanilla, check=True)
fixture = source / 'attribute-updates.tsv'
if sys.argv[1:] == ['--check']:
    if output.read_bytes() != fixture.read_bytes():
        raise SystemExit('Native waypoint observations differ; regenerate and inspect.')
elif sys.argv[1:]:
    raise SystemExit('usage: generate.py [--check]')
else:
    fixture.write_bytes(output.read_bytes())
