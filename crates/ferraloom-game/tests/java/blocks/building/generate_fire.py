#!/usr/bin/env python3
"""Capture actual vanilla fire/lava ticks, player gates and portal construction."""
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
parser.add_argument('family', choices=['radius', 'portal', 'lava', 'tick', 'place', 'contact', 'rain'])
parser.add_argument('--check', action='store_true')
args = parser.parse_args()
expected = {'radius': 896, 'portal': 2592, 'lava': 576, 'tick': 1920, 'place': 864, 'contact': 1026, 'rain': 960}[args.family]
with tempfile.TemporaryDirectory(dir=CACHE / 'tools', prefix='building-fire-') as temporary:
    with (CACHE / f'logs/building-fire-{args.family}-native.log').open('w') as log:
        sources = [HERE.parent / 'redstone/RedstoneIdentityVectors.java', HERE / 'AnvilVectors.java', HERE / 'FireLifecycleVectors.java']
        result = subprocess.run([str(JAVA / 'javac'), '-cp', CLASSPATH, '-d', temporary, *map(str, sources)], stdout=log, stderr=subprocess.STDOUT)
        if result.returncode:
            raise RuntimeError((CACHE / f'logs/building-fire-{args.family}-native.log').read_text()[-8000:])
        result = subprocess.run([str(JAVA / 'java'), '-Xmx768M', '-cp', temporary + os.pathsep + CLASSPATH, 'FireLifecycleCapture', args.family], cwd=temporary, stdout=subprocess.PIPE, stderr=log, text=True, timeout=300)
        log.write(result.stdout)
        if result.returncode:
            log.flush()
            raise RuntimeError((CACHE / f'logs/building-fire-{args.family}-native.log').read_text()[-8000:])
    lines = [line.removeprefix('BUILDING\t') for line in result.stdout.splitlines() if line.startswith('BUILDING\t')]
    assert len(lines) == expected, f'{args.family}: expected {expected}, got {len(lines)}'
    output = '\n'.join(lines) + '\n'
    target = HERE / f'fire-{args.family}.tsv.gz'
    if args.check:
        assert gzip.decompress(target.read_bytes()).decode() == output, f'native {args.family} vectors differ'
    else:
        target.write_bytes(gzip.compress(output.encode(), mtime=0))
    print(f'{len(lines)} native fire {args.family} observations')
