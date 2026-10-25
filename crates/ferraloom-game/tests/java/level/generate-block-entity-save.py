#!/usr/bin/env python3
"""Capture the actual 26.3 LevelChunk live, promotion and pending save paths."""
from pathlib import Path
import subprocess

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
java = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/block-entity-save-vectors'
classes.mkdir(parents=True, exist_ok=True)
classpath = ':'.join(map(str, [
    classes, vanilla / 'versions/26.3/server-26.3.jar',
    *sorted((vanilla / 'libraries').rglob('*.jar')),
    *sorted((vanilla / 'tree-harness').rglob('*.jar')),
]))
for command in [
    [str(java / 'javac'), '-cp', classpath, '-d', str(classes),
     str(source / 'NeighborVectors.java'), str(source / 'BlockEntitySaveVectors.java')],
    [str(java / 'java'), '-Xmx768m', '-Dnet.bytebuddy.experimental=true',
     '-XX:+EnableDynamicAgentLoading', '-cp', classpath, 'BlockEntitySaveVectors'],
]:
    result = subprocess.run(command, cwd=vanilla, text=True, capture_output=True)
    if result.returncode:
        print(result.stdout[-4000:])
        print(result.stderr)
        raise SystemExit(result.returncode)
rows = [line.removeprefix('VECTOR\t') for line in result.stdout.splitlines()
        if line.startswith('VECTOR\t')]
assert len(rows) == 5, len(rows)
(source / 'block-entity-save.tsv').write_text('\n'.join(rows) + '\n')
print(f'{len(rows)} actual native block-entity save observations')
