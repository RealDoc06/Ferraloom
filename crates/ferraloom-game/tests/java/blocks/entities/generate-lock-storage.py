#!/usr/bin/env python3
"""Capture native LockCode map/sentinel behavior without changing strict codecs."""
from pathlib import Path
import subprocess

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/be-lock-storage-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [classes, vanilla / 'versions/26.3/server-26.3.jar',
    *sorted((vanilla / 'libraries').rglob('*.jar')),
    *sorted((vanilla / 'tree-harness').rglob('*.jar'))]))
for command in [
    [str(jdk / 'javac'), '-cp', cp, '-d', str(classes),
     str(source.parent.parent / 'level/NeighborVectors.java'),
     str(source / 'FactoryBatchTwoVectors.java'), str(source / 'LockStorageVectors.java')],
    [str(jdk / 'java'), '-Xmx768m', '-XX:+EnableDynamicAgentLoading',
     '-Dnet.bytebuddy.experimental=true', '-cp', cp, 'LockStorageVectors'],
]:
    result = subprocess.run(command, cwd=classes, text=True, capture_output=True)
    if result.returncode:
        print(result.stdout[-4000:])
        print(result.stderr)
        raise SystemExit(result.returncode)
rows = [line for line in result.stdout.splitlines() if line.startswith('lock-storage\t')]
assert len(rows) == 100, len(rows)
(source / 'lock-storage.tsv').write_text('\n'.join(rows) + '\n')
print(f'{len(rows)} native lock storage observations')
