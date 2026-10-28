#!/usr/bin/env python3
"""Capture independent vanilla pointed-dripstone mutation, event, queue and RNG vectors."""
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
CP = os.pathsep.join(map(str, [VANILLA / 'versions/26.3/server-26.3.jar', *sorted((VANILLA / 'libraries').rglob('*.jar'))]))
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--check', action='store_true')
args = parser.parse_args()
with tempfile.TemporaryDirectory(dir=CACHE / 'tools', prefix='dripstone-') as temporary:
    log_path = CACHE / 'logs/building-dripstone-native.log'
    with log_path.open('w') as log:
        result = subprocess.run([str(JAVA / 'javac'), '-cp', CP, '-d', temporary,
            str(HERE.parent / 'redstone/RedstoneIdentityVectors.java'), str(HERE / 'AnvilVectors.java'),
            str(HERE / 'DripstoneVectors.java')], stdout=log, stderr=subprocess.STDOUT)
        if result.returncode:
            raise RuntimeError(log_path.read_text()[-8000:])
        result = subprocess.run([str(JAVA / 'java'), '-Xmx768M', '-cp', temporary + os.pathsep + CP,
            'DripstoneCapture'], cwd=temporary, stdout=subprocess.PIPE, stderr=log, text=True, timeout=300)
        log.write(result.stdout)
        if result.returncode:
            log.flush()
            raise RuntimeError(log_path.read_text()[-8000:])
    lines = [line.removeprefix('BUILDING\t') for line in result.stdout.splitlines() if line.startswith('BUILDING\t')]
    modes = [line.split('\t', 1)[0] for line in lines]
    assert modes.count('transfer') == 12832
    assert modes.count('grow') == 3208
    assert modes.count('random') == 3636
    output = ('\n'.join(lines) + '\n').encode()
    target = HERE / 'dripstone.tsv.gz'
    if args.check:
        assert gzip.decompress(target.read_bytes()) == output, 'native dripstone vectors differ'
    else:
        target.write_bytes(gzip.compress(output, mtime=0))
    print(f'{len(lines)} native pointed-dripstone observations')
