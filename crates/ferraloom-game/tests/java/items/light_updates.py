#!/usr/bin/env python3
"""Capture native LightBlock water scheduling, state updates and RNG."""
from pathlib import Path
import gzip
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
with tempfile.TemporaryDirectory(prefix='ferraloom-light-updates-') as directory:
    subprocess.run([str(JDK / 'javac'), '-cp', CLASSPATH, '-d', directory,
                    str(ROOT / 'crates/ferraloom-game/tests/java/level/NeighborVectors.java')], check=True)
    subprocess.run([str(JDK / 'java'), '-cp', directory + ':' + CLASSPATH,
                    'NeighborVectors', 'generate', directory + '/NeighborFixture.java'], check=True, cwd=directory)
    generated = Path(directory) / 'NeighborFixture.java'
    generated.write_text(generated.read_text().replace('final class MutationFixture', 'class MutationFixture'))
    subprocess.run([str(JDK / 'javac'), '-cp', directory + ':' + CLASSPATH, '-d', directory,
                    directory + '/NeighborFixture.java', str(HERE / '../blocks/plants/sculk/SculkShapeVectors.java'), str(HERE / 'LightUpdateVectors.java')], check=True)
    native = subprocess.run([str(JDK / 'java'), '-Xmx512M', '-cp', directory + ':' + CLASSPATH,
                             'LightUpdateVectors'], cwd=directory, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if native.returncode:
        print(native.stdout.decode(), file=sys.stderr)
        native.check_returncode()
    vectors = b'\n'.join(line for line in native.stdout.splitlines() if line.startswith(b'light\t')) + b'\n'
    fixture = HERE / 'light-updates.tsv.gz'
    if '--check' in sys.argv:
        if gzip.decompress(fixture.read_bytes()) != vectors:
            raise SystemExit('native light update vectors differ')
    else:
        fixture.write_bytes(gzip.compress(vectors, mtime=0))
    print(f'{len(vectors.splitlines())} native light update state/ticks/RNG vectors PASS')
