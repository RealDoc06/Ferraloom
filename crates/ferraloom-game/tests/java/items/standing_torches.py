#!/usr/bin/env python3
"""Capture native standing-torch support and all-direction shape updates."""
from pathlib import Path
import os
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
CACHE = Path('/home/doc/code/Ferraloom/.cache')
JDK = Path(os.environ.get('JAVA_HOME', CACHE / 'toolchains/jdk-25.0.4.1+1')) / 'bin'
VANILLA = CACHE / 'vanilla/26.3'
CLASSPATH = ':'.join(map(str, [VANILLA / 'versions/26.3/server-26.3.jar',
                               *sorted((VANILLA / 'libraries').rglob('*.jar'))]))
with tempfile.TemporaryDirectory(prefix='ferraloom-standing-torches-') as directory:
    subprocess.run([str(JDK / 'javac'), '-cp', CLASSPATH, '-d', directory,
                    str(ROOT / 'crates/ferraloom-game/tests/java/level/NeighborVectors.java')], check=True)
    subprocess.run([str(JDK / 'java'), '-cp', directory + ':' + CLASSPATH,
                    'NeighborVectors', 'generate', directory + '/NeighborFixture.java'], check=True, cwd=directory)
    subprocess.run([str(JDK / 'javac'), '-cp', directory + ':' + CLASSPATH, '-d', directory,
                    directory + '/NeighborFixture.java', str(HERE / 'StandingTorchVectors.java')], check=True)
    native = subprocess.run([str(JDK / 'java'), '-Xmx512M', '-cp', directory + ':' + CLASSPATH,
                             'StandingTorchVectors'], cwd=directory, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if native.returncode:
        print(native.stdout.decode(), file=sys.stderr)
        native.check_returncode()
    vectors = b'\n'.join(line for line in native.stdout.splitlines() if line.startswith(b'torch\t')) + b'\n'
    fixture = HERE / 'standing-torches.tsv'
    if '--check' in sys.argv:
        if fixture.read_bytes() != vectors:
            raise SystemExit('native standing-torch vectors differ')
    else:
        fixture.write_bytes(vectors)
    print(f'{len(vectors.splitlines())} native standing-torch support/update/RNG vectors PASS')
