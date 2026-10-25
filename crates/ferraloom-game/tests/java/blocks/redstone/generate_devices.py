#!/usr/bin/env python3
"""Capture native redstone device methods on a real ServerLevel."""
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
    compiled = subprocess.run([str(JAVA / 'javac'), '-cp', CLASSPATH, '-d', temporary, str(HERE / 'RedstoneIdentityVectors.java'), str(HERE / 'DeviceVectors.java')], capture_output=True, text=True)
    if compiled.returncode:
        raise RuntimeError(compiled.stderr[-8000:])
    result = subprocess.run([str(JAVA / 'java'), '-Xmx768M', '-cp', temporary + os.pathsep + CLASSPATH, 'DeviceCapture'], cwd=temporary, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(result.stderr[-8000:])
    lines = [line.removeprefix('DEVICE\t') for line in result.stdout.splitlines() if line.startswith('DEVICE\t')]
    if not lines:
        raise RuntimeError('Vanilla emitted no device cases')
    import gzip
    with gzip.GzipFile(filename=HERE / 'devices.tsv.gz', mode='wb', mtime=0) as output:
        output.write(('\n'.join(lines)+'\n').encode())
    print(f'{len(lines)} native redstone device cases')
