#!/usr/bin/env python3
"""Observe vanilla's EquipmentSlotGroup#test for every group/slot pair."""
from pathlib import Path
import subprocess
import sys

cache = Path('/home/doc/code/Ferraloom/.cache')
java = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/equipment-slot-vectors'
classes.mkdir(parents=True, exist_ok=True)
source = Path(__file__).resolve().parent
cp = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar'))]))
subprocess.run([str(java / 'javac'), '-cp', cp, '-d', str(classes), str(source / 'SlotGroupVectors.java')], check=True)
result = subprocess.run([str(java / 'java'), '-cp', f'{classes}:{cp}', 'SlotGroupVectors'], check=True, text=True, capture_output=True)
vectors = '\n'.join(line for line in result.stdout.splitlines() if len(line.split('\t')) == 3) + '\n'
if sys.argv[1:] == ['--check']:
    if (source / 'vectors.tsv').read_text() != vectors:
        raise SystemExit('Native slot-group observations differ from vectors.tsv.')
elif sys.argv[1:]:
    raise SystemExit('usage: generate.py [--check]')
else:
    (source / 'vectors.tsv').write_text(vectors)
print(f'{len(vectors.splitlines())} vanilla slot-group vectors')
