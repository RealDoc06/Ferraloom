#!/usr/bin/env python3
"""Capture Cow's actual ServerEntity pairing and ongoing living update packets."""
from pathlib import Path
import subprocess
import sys

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
vanilla = cache / 'vanilla/26.3'
java = cache / 'toolchains/jdk-25.0.4.1+1/bin'
classes = cache / 'tools/living-tracker-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar',
                       *sorted((vanilla / 'libraries').rglob('*.jar')),
                       *sorted((vanilla / 'tree-harness').rglob('*.jar'))]))
subprocess.run([str(java / 'javac'), '-cp', cp, '-d', str(classes),
                str(source.parents[1] / 'advancements/AdvancementVectors.java'),
                str(source.parent / 'TrackingVectors.java'),
                str(source / 'LivingTrackingVectors.java')], check=True)
output = classes / 'living.tsv'
subprocess.run([str(java / 'java'), '-Xmx768m', '-XX:+EnableDynamicAgentLoading',
                '-Dnet.bytebuddy.experimental=true', '-cp', f'{classes}:{cp}',
                'LivingTrackingVectors', str(output)], cwd=vanilla, check=True)
fixture = source / 'living.tsv'
if sys.argv[1:] == ['--check']:
    if output.read_bytes() != fixture.read_bytes():
        raise SystemExit('Living tracker observations differ; regenerate and inspect.')
elif sys.argv[1:]:
    raise SystemExit('usage: generate-living.py [--check]')
else:
    fixture.write_bytes(output.read_bytes())
