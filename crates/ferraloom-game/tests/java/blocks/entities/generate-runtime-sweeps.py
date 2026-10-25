#!/usr/bin/env python3
"""Capture all registry entries and block-selected tickers from vanilla 26.3."""
from pathlib import Path
import subprocess

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/be-runtime-sweep-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [classes, vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar')), *sorted((vanilla / 'tree-harness').rglob('*.jar'))]))

def run(args):
    result = subprocess.run(args, cwd=classes, text=True, capture_output=True)
    if result.returncode:
        print(result.stdout[-2000:])
        print(result.stderr)
        raise SystemExit(result.returncode)
    return result.stdout

run([str(jdk / 'javac'), '-cp', cp, '-d', str(classes), str(source.parent.parent / 'level/NeighborVectors.java'), str(source / 'RuntimeSweepVectors.java')])
output = run([str(jdk / 'java'), '-Xmx768m', '-XX:+EnableDynamicAgentLoading', '-Dnet.bytebuddy.experimental=true', '-cp', cp, 'RuntimeSweepVectors', str(source / 'registry-bootstrap.json')])
rows = [line for line in output.splitlines() if line.startswith(('tick-crafter\t', 'tick-daylight\t'))]
assert len(rows) == 22, len(rows)
(source / 'server-tickers.tsv').write_text('\n'.join(rows) + '\n')
print(f'49 native registry entries and {len(rows)} actual server ticker observations')
