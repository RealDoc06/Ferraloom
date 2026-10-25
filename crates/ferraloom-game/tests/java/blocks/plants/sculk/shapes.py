#!/usr/bin/env python3
"""Capture native sculk shape virtuals and the real fluid tick queue."""
from pathlib import Path
import gzip
import os
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[6]
CACHE = Path('/home/doc/code/Ferraloom/.cache')
JDK = Path(os.environ.get('JAVA_HOME', CACHE / 'toolchains/jdk-25.0.4.1+1')) / 'bin'
VANILLA = CACHE / 'vanilla/26.3'
CLASSPATH = ':'.join(map(str, [VANILLA / 'versions/26.3/server-26.3.jar',
                               *sorted((VANILLA / 'libraries').rglob('*.jar'))]))
with tempfile.TemporaryDirectory(prefix='ferraloom-sculk-shapes-') as directory:
    subprocess.run([str(JDK / 'javac'), '-cp', CLASSPATH, '-d', directory,
                    str(ROOT / 'crates/ferraloom-game/tests/java/level/NeighborVectors.java')], check=True)
    subprocess.run([str(JDK / 'java'), '-cp', directory + ':' + CLASSPATH,
                    'NeighborVectors', 'generate', directory + '/NeighborFixture.java'], check=True, cwd=directory)
    generated = Path(directory) / 'NeighborFixture.java'
    generated.write_text(generated.read_text().replace('final class MutationFixture', 'class MutationFixture'))
    subprocess.run([str(JDK / 'javac'), '-cp', directory + ':' + CLASSPATH, '-d', directory,
                    str(generated), str(HERE / 'SculkShapeVectors.java')], check=True)
    native = subprocess.run([str(JDK / 'java'), '-Xmx512M', '-cp', directory + ':' + CLASSPATH,
                             'SculkShapeVectors'], cwd=directory, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if native.returncode:
        print(native.stdout.decode(), file=sys.stderr)
        native.check_returncode()
    vectors = b'\n'.join(line for line in native.stdout.splitlines() if line.startswith(b'shape\t')) + b'\n'
    fixture = HERE / 'shapes.tsv.gz'
    if '--check' in sys.argv:
        if gzip.decompress(fixture.read_bytes()) != vectors:
            raise SystemExit('native sculk shape vectors differ')
    else:
        fixture.write_bytes(gzip.compress(vectors, mtime=0))
    print(f'{len(vectors.splitlines())} native sculk shape/tick/RNG vectors PASS')
