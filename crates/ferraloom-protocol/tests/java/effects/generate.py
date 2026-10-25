#!/usr/bin/env python3
"""Capture the real 26.3 recursive effect-details NBT codec."""
from pathlib import Path
import subprocess

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/effect-details-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar'))]))
subprocess.run([str(jdk / 'javac'), '-cp', cp, '-d', str(classes), str(source / 'EffectDetailsVectors.java')], cwd=classes, check=True)
result = subprocess.run([str(jdk / 'java'), '-Xmx512m', '-cp', f'{classes}:{cp}', 'EffectDetailsVectors'], cwd=classes, text=True, capture_output=True, check=True)
rows = [line for line in result.stdout.splitlines() if line.startswith('effect-details\t')]
(source / 'vectors.tsv').write_text('\n'.join(rows) + '\n')
print(f'{len(rows)} native effect-details codec cases')
