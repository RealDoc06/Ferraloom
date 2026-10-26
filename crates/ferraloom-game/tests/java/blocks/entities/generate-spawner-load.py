#!/usr/bin/env python3
"""Capture real spawner loadAdditional update notifications and native saved state."""
from pathlib import Path
import gzip
import subprocess

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/spawner-load-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [classes, vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar')), *sorted((vanilla / 'tree-harness').rglob('*.jar'))]))

def run(args):
    result = subprocess.run(args, cwd=classes, text=True, capture_output=True)
    if result.returncode:
        print(result.stdout[-2000:])
        print(result.stderr)
        raise SystemExit(result.returncode)
    return result.stdout

run([str(jdk / 'javac'), '-cp', cp, '-d', str(classes), str(source.parent.parent / 'level/NeighborVectors.java'), str(source / 'SpawnerLoadVectors.java')])
output = run([str(jdk / 'java'), '-Xmx512m', '-XX:+EnableDynamicAgentLoading', '-Dnet.bytebuddy.experimental=true', '-cp', cp, 'SpawnerLoadVectors'])
rows = [line for line in output.splitlines() if line.startswith(('spawner-live\t',))]
assert len(rows) == 64, len(rows)
(source / 'spawner-load.tsv.gz').write_bytes(gzip.compress(('\n'.join(rows) + '\n').encode(), mtime=0))
print(f'{len(rows)} native spawner live-load and notification observations')
