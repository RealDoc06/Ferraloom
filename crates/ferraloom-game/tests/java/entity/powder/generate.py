#!/usr/bin/env python3
"""Capture actual FallingBlockEntity water interception and onLand ordering."""
from pathlib import Path
import subprocess
import sys

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
java = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/entity-powder-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [classes, vanilla / 'versions/26.3/server-26.3.jar',
    *sorted((vanilla / 'libraries').rglob('*.jar')),
    *sorted((vanilla / 'tree-harness').glob('*.jar'))]))

def run(args):
    result = subprocess.run(args, cwd=classes, text=True, capture_output=True)
    if result.returncode:
        print(result.stdout[-2000:])
        print(result.stderr)
        raise SystemExit(result.returncode)
    return result.stdout

run([str(java / 'javac'), '-cp', cp, '-d', str(classes),
    str(source / 'PowderVectors.java'), str(source.parents[1] / 'level/NeighborVectors.java')])
output = run([str(java / 'java'), '-Xmx768m', '-XX:+EnableDynamicAgentLoading',
    '-Dnet.bytebuddy.experimental=true', '-cp', cp, 'PowderVectors'])
rows = [line for line in output.splitlines() if line.startswith('powder-tick\t')]
assert len(rows) == 42, len(rows)
vectors = '\n'.join(rows) + '\n'
if sys.argv[1:] == ['--check']:
    if (source / 'vectors.tsv').read_text() != vectors:
        raise SystemExit('Native falling-powder observations differ; regenerate and inspect.')
elif sys.argv[1:]:
    raise SystemExit('usage: generate.py [--check]')
else:
    (source / 'vectors.tsv').write_text(vectors)
print(f'{len(rows)} native falling-powder observations')
