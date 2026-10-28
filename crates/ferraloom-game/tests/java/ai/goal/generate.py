#!/usr/bin/env python3
"""Capture the pinned jar's GoalSelector and Goal tick-delay behavior."""
from pathlib import Path
import subprocess
import sys
source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
vanilla = cache / 'vanilla/26.3'
java = cache / 'toolchains/jdk-25.0.4.1+1/bin'
classes = cache / 'tools/goal-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar'))]))
subprocess.run([str(java / 'javac'), '-cp', cp, '-d', str(classes), str(source/'GoalSelectorVectors.java')],check=True)
output = classes/'goals.tsv'
subprocess.run([str(java/'java'), '-Xmx256m', '-cp', f'{classes}:{cp}', 'GoalSelectorVectors', str(output)],cwd=vanilla,check=True)
fixture = source/'vectors.tsv'
if sys.argv[1:] == ['--check']:
    if output.read_bytes()!=fixture.read_bytes():
        raise SystemExit('Native goal observations differ')
elif sys.argv[1:]:
    raise SystemExit('usage: generate.py [--check]')
else:
    fixture.write_bytes(output.read_bytes())
