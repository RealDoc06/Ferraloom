#!/usr/bin/env python3
"""Capture native end-gateway data, bounds/partial codecs, ticker and event behavior."""
from pathlib import Path
import gzip
import subprocess

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/be-end-gateway-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [classes, vanilla / 'versions/26.3/server-26.3.jar',
    *sorted((vanilla / 'libraries').rglob('*.jar')),
    *sorted((vanilla / 'tree-harness').rglob('*.jar'))]))
for command in [
    [str(jdk / 'javac'), '-cp', cp, '-d', str(classes),
     str(source.parent.parent / 'level/NeighborVectors.java'),
     str(source / 'FactoryBatchTwoVectors.java'), str(source / 'EndGatewayFactoryVectors.java')],
    [str(jdk / 'java'), '-Xmx768m', '-XX:+EnableDynamicAgentLoading',
     '-Dnet.bytebuddy.experimental=true', '-cp', cp, 'EndGatewayFactoryVectors'],
]:
    result = subprocess.run(command, cwd=classes, text=True, capture_output=True)
    if result.returncode:
        print(result.stdout[-4000:])
        print(result.stderr)
        raise SystemExit(result.returncode)
rows = [line for line in result.stdout.splitlines() if line.startswith('gateway-')]
assert len(rows) == 78, len(rows)
(source / 'end-gateway-factory.tsv.gz').write_bytes(gzip.compress(('\n'.join(rows) + '\n').encode(), mtime=0))
print(f'{len(rows)} native end-gateway observations')
