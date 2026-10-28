#!/usr/bin/env python3
"""Capture native hopper admission across canonical item-tag reloads."""
import argparse
import gzip
import os
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
CACHE = Path('/home/doc/code/Ferraloom/.cache')
JAVA = CACHE / 'toolchains/jdk-25.0.4.1+1/bin'
VANILLA = CACHE / 'vanilla/26.3'
CLASSPATH = os.pathsep.join(map(str, [VANILLA / 'versions/26.3/server-26.3.jar', *sorted((VANILLA / 'libraries').rglob('*.jar'))]))
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--check', action='store_true')
args = parser.parse_args()
with tempfile.TemporaryDirectory(dir=CACHE / 'tools', prefix='hopper-admission-') as temporary:
    log_path = CACHE / 'logs/hopper-admission-native.log'
    with log_path.open('w') as log:
        result = subprocess.run([str(JAVA / 'javac'), '-cp', CLASSPATH, '-d', temporary, str(HERE / 'RedstoneIdentityVectors.java'), str(HERE / 'HopperAdmissionVectors.java')], stdout=log, stderr=subprocess.STDOUT)
        if result.returncode:
            raise RuntimeError(log_path.read_text()[-8000:])
        result = subprocess.run([str(JAVA / 'java'), '-Xmx768M', '-cp', temporary + os.pathsep + CLASSPATH, 'HopperAdmissionVectors'], cwd=temporary, stdout=subprocess.PIPE, stderr=log, text=True, timeout=180)
        log.write(result.stdout)
        if result.returncode:
            log.flush()
            raise RuntimeError(log_path.read_text()[-8000:])
    lines = [line.removeprefix('ADMISSION\t') for line in result.stdout.splitlines() if line.startswith('ADMISSION\t')]
    assert len(lines) == 15360, f'expected 15360 native admission cases, got {len(lines)}'
    output = '\n'.join(lines) + '\n'
    target = HERE / 'admission.tsv.gz'
    if args.check:
        assert gzip.decompress(target.read_bytes()).decode() == output, 'native admission vectors differ'
    else:
        target.write_bytes(gzip.compress(output.encode(), mtime=0))
    print(f'{len(lines)} native hopper admission observations')
