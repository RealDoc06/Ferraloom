#!/usr/bin/env python3
"""Capture real workstation menus' range, replacement and NULL access behavior."""
from pathlib import Path
import subprocess

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/inventory-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [classes, vanilla / 'versions/26.3/server-26.3.jar',
                       *sorted((vanilla / 'libraries').rglob('*.jar')),
                       *sorted((vanilla / 'tree-harness').rglob('*.jar'))]))

def run(args):
    result = subprocess.run(args, cwd=classes, text=True, capture_output=True)
    if result.returncode:
        print(result.stdout[-3000:])
        print(result.stderr)
        raise SystemExit(result.returncode)
    return result.stdout

tests = source.parent.parent
run([str(jdk / 'javac'), '-cp', cp, '-d', str(classes),
     *map(str, sorted((tests / 'entity').glob('*.java'))), str(tests / 'level/NeighborVectors.java')])
run([str(jdk / 'java'), '-cp', cp, 'EntityVectors', 'generate', str(classes / 'EntityFixture.java')])
run([str(jdk / 'javac'), '-cp', cp, '-d', str(classes),
     str(classes / 'EntityFixture.java'), str(tests / 'inventory/InventoryVectors.java'),
     str(source / 'WorkstationAccessVectors.java'), str(source / 'WorkstationRemovedVectors.java')])
output = run([str(jdk / 'java'), '-Xmx512m', '-cp', cp, 'WorkstationAccessVectors'])
rows = [line for line in output.splitlines() if line.startswith('access\t')]
assert len(rows) == 258, len(rows)
(source / 'workstation-access.tsv').write_text('\n'.join(rows) + '\n')
print(f'{len(rows)} native captured workstation access observations')
output = run([str(jdk / 'java'), '-Xmx512m', '-Dnet.bytebuddy.experimental=true',
              '-XX:+EnableDynamicAgentLoading', '-cp', cp, 'WorkstationRemovedVectors'])
rows = [line for line in output.splitlines() if line.startswith('removed\t')]
assert len(rows) == 84, len(rows)
(source / 'workstation-removed.tsv').write_text('\n'.join(rows) + '\n')
print(f'{len(rows)} native workstation close/return/drop observations')
rows = [line for line in output.splitlines() if line.startswith('grid\t')]
assert len(rows) == 4, len(rows)
(source / 'workstation-grid.tsv').write_text('\n'.join(rows) + '\n')
print(f'{len(rows)} native captured CraftingMenu slotsChanged/finish observations')
