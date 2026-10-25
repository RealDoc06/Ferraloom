#!/usr/bin/env python3
"""Run the native piston phase trace; optionally compare a Rust replay log.

Captured observations stay under the shared cache, never in the repository.
"""
import argparse
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
parser.add_argument('--compare-rust', type=Path)
args = parser.parse_args()
log_path = CACHE / 'logs/piston-native-phase-trace.log'
with tempfile.TemporaryDirectory(dir=CACHE / 'tools', prefix='piston-phase-') as temporary:
    with log_path.open('w') as log:
        result = subprocess.run([str(JAVA / 'javac'), '-cp', CLASSPATH, '-d', temporary, str(HERE / 'RedstoneIdentityVectors.java'), str(HERE / 'PistonPhaseVectors.java')], stdout=log, stderr=subprocess.STDOUT)
        if not result.returncode:
            result = subprocess.run([str(JAVA / 'java'), '-Xmx2G', '-cp', temporary + os.pathsep + CLASSPATH, 'PistonPhaseCapture'], cwd=temporary, stdout=log, stderr=subprocess.STDOUT, timeout=180)
    if result.returncode:
        raise RuntimeError(log_path.read_text()[-8000:])

def records(path):
    output = {}
    for line in path.read_text().splitlines():
        index = line.find('PHASE\t')
        if index >= 0:
            fields = line[index:].split('\t')
            key, value = tuple(fields[1:5]), fields[5:]
            assert key not in output, f'duplicate phase {key}'
            output[key] = value
    return output

native = records(log_path)
assert len(native) == 272, f'expected 272 native phase observations, got {len(native)}'
print(f'Native piston phase trace: {len(native)} observations; {log_path}')
if args.compare_rust:
    rust = records(args.compare_rust)
    if list(native) != list(rust):
        raise SystemExit('canonical phase sequence differs (including missing or extra observations)')
    differences = [(key, native.get(key), rust.get(key)) for key in dict.fromkeys([*native, *rust]) if native.get(key) != rust.get(key)]
    if differences:
        for key, expected, actual in differences[:100]:
            print(f'{key}: native={expected}; Rust={actual}')
        raise SystemExit(f'{len(differences)} canonical phase differences')
    print(f'Canonical Rust piston phases match all {len(native)} native observations')
