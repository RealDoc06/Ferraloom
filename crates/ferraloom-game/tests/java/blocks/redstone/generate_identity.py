#!/usr/bin/env python3
"""Capture native hopper/dispenser receiver identity across synchronous callbacks."""
import os
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
CACHE = Path('/home/doc/code/Ferraloom/.cache')
JAVA = CACHE / 'toolchains/jdk-25.0.4.1+1/bin'
VANILLA = CACHE / 'vanilla/26.3'
CLASSPATH = os.pathsep.join(map(str, [VANILLA / 'versions/26.3/server-26.3.jar', *sorted((VANILLA / 'libraries').rglob('*.jar'))]))
with tempfile.TemporaryDirectory() as temporary:
    subprocess.run([str(JAVA / 'javac'), '-cp', CLASSPATH, '-d', temporary, str(HERE / 'RedstoneIdentityVectors.java')], check=True)
    result = subprocess.run([str(JAVA / 'java'), '-Xmx768M', '-cp', temporary + os.pathsep + CLASSPATH, 'RedstoneIdentityCapture'], cwd=temporary, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(result.stderr[-8000:])
    lines = [line.removeprefix('IDENTITY\t') for line in result.stdout.splitlines() if line.startswith('IDENTITY\t')]
    if len(lines) != 18:
        raise RuntimeError(f'Expected 18 native receiver cases, got {len(lines)}: {result.stdout[-8000:]}')
    (HERE / 'identity.tsv').write_text('\n'.join(lines) + '\n')
    print(f'{len(lines)} native retained-receiver cases')
