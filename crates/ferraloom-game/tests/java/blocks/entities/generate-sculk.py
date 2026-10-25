#!/usr/bin/env python3
"""Capture native sculk data/codec/default-component and proto factory values."""
from pathlib import Path
import gzip
import subprocess

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/sculk-block-entity-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [classes, vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar')), *sorted((vanilla / 'tree-harness').rglob('*.jar'))]))

def run(args):
    result = subprocess.run(args, cwd=classes, text=True, capture_output=True)
    if result.returncode:
        print(result.stdout[-2000:])
        print(result.stderr)
        raise SystemExit(result.returncode)
    return result.stdout

run([str(jdk / 'javac'), '-cp', cp, '-d', str(classes), str(source.parent.parent / 'level/NeighborVectors.java'), str(source / 'WorldgenBlockEntityVectors.java'), str(source / 'SculkDataVectors.java')])
output = run([str(jdk / 'java'), '-Xmx768m', '-XX:+EnableDynamicAgentLoading', '-Dnet.bytebuddy.experimental=true', '-cp', cp, 'SculkDataVectors'])
rows = [line for line in output.splitlines() if line.startswith(('sculk\t', 'sculk-components\t', 'sculk-worldgen\t'))]
assert len(rows) == 83, len(rows)
(source / 'sculk.tsv.gz').write_bytes(gzip.compress(('\n'.join(rows) + '\n').encode(), mtime=0))
print(f'{len(rows)} native sculk codec/component/worldgen observations')
