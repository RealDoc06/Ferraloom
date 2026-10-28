#!/usr/bin/env python3
"""Capture 26.3 monster constructors and concrete pre-parent cube NBT semantics."""
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent
CACHE = Path('/home/doc/code/Ferraloom/.cache')
VANILLA = CACHE / 'vanilla/26.3'
JDK = CACHE / 'toolchains/jdk-25.0.4.1+1/bin'
CLASSES = CACHE / 'tools/monster-vectors'
CLASSES.mkdir(parents=True, exist_ok=True)
CP = ':'.join(map(str, [VANILLA / 'versions/26.3/server-26.3.jar',
                      *sorted((VANILLA / 'libraries').rglob('*.jar')),
                      *sorted((VANILLA / 'tree-harness').glob('*.jar'))]))


def run(args):
    result = subprocess.run(args, cwd=CLASSES, text=True, capture_output=True)
    if result.returncode:
        print(result.stdout[-4000:])
        print(result.stderr[-8000:])
        raise SystemExit(result.returncode)
    return result.stdout


sources = sorted(HERE.parent.glob('*.java'))
run([str(JDK / 'javac'), '-cp', CP, '-d', str(CLASSES),
     *map(str, sources), str(HERE.parents[1] / 'level/NeighborVectors.java')])
run([str(JDK / 'java'), '-Xmx512m', '-cp', str(CLASSES) + ':' + CP,
     'EntityVectors', 'generate', str(CLASSES / 'EntityFixture.java')])
run([str(JDK / 'javac'), '-cp', str(CLASSES) + ':' + CP, '-d', str(CLASSES),
     str(CLASSES / 'EntityFixture.java'), *map(str, sorted(HERE.glob('*.java')))])
output = run([str(JDK / 'java'), '-Xmx512m', '-cp', str(CLASSES) + ':' + CP,
              'MonsterVectors'])
vectors = '\n'.join(line for line in output.splitlines()
                    if line.startswith(('monster-', 'cube-load\t', 'zombie-villager-random\t', 'zombie-nbt\t'))) + '\n'
path = HERE / 'vectors.tsv'
if sys.argv[1:] == ['--check']:
    if path.read_text() != vectors:
        raise SystemExit('native monster observations differ')
elif sys.argv[1:]:
    raise SystemExit('usage: generate.py [--check]')
else:
    path.write_text(vectors)
print(f'{len(vectors.splitlines())} native monster observations')
