#!/usr/bin/env python3
"""Capture live-level building lifecycle vectors from vanilla 26.3."""
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
parser.add_argument('family', nargs='?', choices=['anvil', 'powder-land', 'workstations'], default='anvil')
parser.add_argument('--check', action='store_true')
args = parser.parse_args()
sources, entry, expected = {
    'anvil': (['AnvilVectors.java'], 'AnvilCapture', 645),
    'powder-land': (['AnvilVectors.java', 'PowderLandingVectors.java'], 'PowderLandingCapture', 5120),
    'workstations': (['WorkstationVectors.java'], 'WorkstationCapture', 245),
}[args.family]
with tempfile.TemporaryDirectory(dir=CACHE / 'tools', prefix='building-lifecycle-') as temporary:
    cp = temporary + os.pathsep + CLASSPATH
    log_path = CACHE / f'logs/building-{args.family}-native.log'
    with log_path.open('w') as log:
        compiled = subprocess.run([str(JAVA / 'javac'), '-cp', CLASSPATH, '-d', temporary, str(HERE.parent / 'redstone/RedstoneIdentityVectors.java'), *[str(HERE / source) for source in sources]], stdout=log, stderr=subprocess.STDOUT)
        if compiled.returncode:
            raise RuntimeError(log_path.read_text()[-8000:])
        result = subprocess.run([str(JAVA / 'java'), '-Xmx768M', '-cp', cp, entry], cwd=temporary, stdout=subprocess.PIPE, stderr=log, text=True, timeout=120)
        log.write(result.stdout)
        if result.returncode:
            log.flush()
            raise RuntimeError(log_path.read_text()[-8000:])
    lines = [line.removeprefix('BUILDING\t') for line in result.stdout.splitlines() if line.startswith('BUILDING\t')]
    output = '\n'.join(lines) + '\n'
    assert len(lines) == expected, f'{args.family}: expected {expected}, got {len(lines)}'
    target = HERE / f'{args.family}.tsv'
    if args.check:
        assert target.read_text() == output, f'native {args.family} vectors differ'
    else:
        target.write_text(output)
    print(f'{len(lines)} native {args.family} lifecycle observations')
