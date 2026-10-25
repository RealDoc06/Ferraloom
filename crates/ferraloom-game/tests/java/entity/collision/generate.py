#!/usr/bin/env python3
"""Capture 26.3 BlockCollisions' full-block shortcut and epsilon boundaries."""
from pathlib import Path
import subprocess
import sys

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
java = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/block-collision-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar'))]))
subprocess.run([str(java / 'javac'), '-cp', cp, '-d', str(classes), str(source / 'BlockCollisionsVectors.java')], cwd=classes, check=True)
result = subprocess.run([str(java / 'java'), '-Xmx256m', '-cp', f'{classes}:{cp}', 'BlockCollisionsVectors'], cwd=classes, check=True, text=True, capture_output=True)
rows = [line for line in result.stdout.splitlines() if line.startswith('block-collision-epsilon\t')]
if len(rows) != 15:
    raise SystemExit(f'Expected 15 native cases, observed {len(rows)}: {result.stdout}\n{result.stderr}')
vectors = '\n'.join(rows) + '\n'
if sys.argv[1:] == ['--check']:
    if (source / 'vectors.tsv').read_text() != vectors:
        raise SystemExit('Native block collision observations differ; regenerate and inspect.')
elif sys.argv[1:]:
    raise SystemExit('usage: generate.py [--check]')
else:
    (source / 'vectors.tsv').write_text(vectors)
print(f'{len(rows)} vanilla block collision vectors')
