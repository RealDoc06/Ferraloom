#!/usr/bin/env python3
"""Capture native CustomData.CODEC traversal; Java outputs stay in the shared cache."""
from pathlib import Path
import subprocess

cache = Path('/home/doc/code/Ferraloom/.cache')
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/custom-data-order-vectors'
classes.mkdir(parents=True, exist_ok=True)
source = Path(__file__).resolve().parent
cp = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar'))]))
subprocess.run([str(jdk / 'javac'), '-cp', cp, '-d', str(classes), str(source / 'CustomDataOrderVectors.java')], check=True)
subprocess.run([str(jdk / 'java'), '-Xmx256m', '-cp', f'{classes}:{cp}', 'CustomDataOrderVectors', str(source / 'vectors.json')], cwd=vanilla, check=True)
print('Captured eight native CustomData codec traversal cases')
