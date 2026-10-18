#!/usr/bin/env python3
"""Capture real container/furniture placement states; Java only, no Rust builds."""
from pathlib import Path
import gzip
import subprocess

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/container-placement-vectors'
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
run([str(jdk / 'javac'), '-cp', f'{classes}:{cp}', '-d', str(classes), str(classes / 'NeighborFixture.java'), str(source / 'ContainerPlacementVectors.java'), str(source / 'FurniturePlacementVectors.java')])
output = run([str(jdk / 'java'), '-Xmx512m', '-cp', f'{classes}:{cp}', 'ContainerPlacementVectors'])
rows = [line for line in output.splitlines() if line.startswith('placement\t')]
(source / 'placement.tsv.gz').write_bytes(gzip.compress(('\n'.join(rows) + '\n').encode(), mtime=0))
print(f'{len(rows)} native container/furniture placement masks')

output = run([str(jdk / 'java'), '-Xmx512m', '-cp', f'{classes}:{cp}', 'FurniturePlacementVectors'])
rows = [line for line in output.splitlines() if line.startswith('furniture\t')]
(source / 'furniture-placement.tsv.gz').write_bytes(gzip.compress(('\n'.join(rows) + '\n').encode(), mtime=0))
print(f'{len(rows)} native sign/furniture placement masks')
