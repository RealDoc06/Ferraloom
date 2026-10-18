#!/usr/bin/env python3
"""Capture real menu listener reentry and memoized copy delivery order."""
from pathlib import Path
import subprocess

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
java = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/inventory-listener-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar'))]))

def run(args):
    result = subprocess.run(args, cwd=classes, text=True, capture_output=True)
    if result.returncode:
        print(result.stdout[-3000:])
        print(result.stderr)
        raise SystemExit(result.returncode)
    return result.stdout

run([str(java/'javac'), '-cp', cp, '-d', str(classes), *map(str, sorted((source.parent/'entity').glob('*.java'))), str(source.parent/'level/NeighborVectors.java')])
run([str(java/'java'), '-cp', f'{classes}:{cp}', 'EntityVectors', 'generate', str(classes/'EntityFixture.java')])
run([str(java/'javac'), '-cp', f'{classes}:{cp}', '-d', str(classes), str(classes/'EntityFixture.java'), str(source/'InventoryVectors.java'), str(source/'ListenerVectors.java'), str(source/'CrafterVectors.java')])
output = run([str(java/'java'), '-Xmx768m', '-cp', f'{classes}:{cp}', 'ListenerVectors'])
rows = [line for line in output.splitlines() if line.startswith('listener\t')]
assert len(rows) == 20
(source/'listeners.tsv').write_text('\n'.join(rows)+'\n')
print(f'{len(rows)} native listener reentry timelines')
