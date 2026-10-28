#!/usr/bin/env python3
"""Capture native bookshelf Container contracts and existing hopper transfers."""
from pathlib import Path
import subprocess
from collections import Counter

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/bookshelf-container-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [classes, vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar'))]))

def run(args):
    result = subprocess.run(args, cwd=classes, text=True, capture_output=True)
    if result.returncode:
        print(result.stdout[-2000:])
        print(result.stderr)
        raise SystemExit(result.returncode)
    return result.stdout

run([str(jdk / 'javac'), '-cp', cp, '-d', str(classes), str(source.parent.parent / 'level/NeighborVectors.java')])
run([str(jdk / 'java'), '-cp', cp, 'NeighborVectors', 'generate', str(classes / 'NeighborFixture.java')])
run([str(jdk / 'javac'), '-cp', cp, '-d', str(classes), str(classes / 'NeighborFixture.java'), str(source / 'BookShelfHookVectors.java'), str(source / 'BookShelfContainerVectors.java')])
output = run([str(jdk / 'java'), '-Xmx512m', '-cp', cp, 'BookShelfContainerVectors'])
rows = [line for line in output.splitlines() if line.startswith(('shelf-admit\t', 'shelf-take\t', 'ordinary-transfer\t'))]
(source / 'bookshelf-container.tsv').write_text('\n'.join(rows) + '\n')
print(dict(Counter(line.split('\t', 1)[0] for line in rows)))
