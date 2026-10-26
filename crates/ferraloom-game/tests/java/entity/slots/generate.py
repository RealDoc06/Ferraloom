#!/usr/bin/env python3
"""Capture actual entity SlotAccess dispatch and player ender storage."""
from pathlib import Path
import subprocess
import sys
source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
java = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/entity-slot-vectors'
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
    *map(str, sorted(source.glob('*.java'))), str(source.parents[1] / 'level/NeighborVectors.java')])
rows = []
for name, prefix in [('EnderVectors', 'ender-nbt\t'), ('SlotVectors', 'slot-'), ('EquipVectors', 'slot-equip\t'), ('SlotCraftVectors', 'slot-craft\t')]:
    output = run([str(java / 'java'), '-Xmx768m', '-XX:+EnableDynamicAgentLoading',
        '-Dnet.bytebuddy.experimental=true', '-cp', cp, name])
    rows.extend(line for line in output.splitlines() if line.startswith(prefix))
assert len(rows) == 1314, len(rows)
vectors = '\n'.join(rows) + '\n'
if sys.argv[1:] == ['--check']:
    if (source / 'vectors.tsv').read_text() != vectors:
        raise SystemExit('Native entity slot observations differ; regenerate and inspect.')
elif sys.argv[1:]:
    raise SystemExit('usage: generate.py [--check]')
else:
    (source / 'vectors.tsv').write_text(vectors)
print(f'{len(rows)} native entity slot observations')
