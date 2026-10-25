#!/usr/bin/env python3
"""Capture real chest virtual and six-side alias states, scheduled water and RNG."""
from pathlib import Path
import gzip
import subprocess

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/chest-shape-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar'))]))

def run(args):
    result = subprocess.run(args, cwd=classes, text=True, capture_output=True)
    if result.returncode:
        print(result.stdout[-2000:])
        print(result.stderr)
        raise SystemExit(result.returncode)
    return result.stdout

run([str(jdk / 'javac'), '-cp', cp, '-d', str(classes), str(source.parent.parent / 'level/NeighborVectors.java')])
run([str(jdk / 'java'), '-cp', f'{classes}:{cp}', 'NeighborVectors', 'generate', str(classes / 'NeighborFixture.java')])
run([str(jdk / 'javac'), '-cp', f'{classes}:{cp}', '-d', str(classes), str(classes / 'NeighborFixture.java'), str(source / 'ChestShapeVectors.java')])
output = run([str(jdk / 'java'), '-Xmx512m', '-cp', f'{classes}:{cp}', 'ChestShapeVectors'])
rows = [line for line in output.splitlines() if line.startswith('chest-shape\t')]
(source / 'chest-shapes.tsv.gz').write_bytes(gzip.compress(('\n'.join(rows) + '\n').encode(), mtime=0))
print(f'{len(rows)} native chest shape/neighbor-alias observations')
