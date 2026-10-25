#!/usr/bin/env python3
"""Call native item placement-subclass stages on the concrete Level fixture."""
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
with tempfile.TemporaryDirectory(prefix='ferraloom-item-placement-') as directory:
    subprocess.run([str(JDK / 'javac'), '-cp', CLASSPATH, '-d', directory,
                    str(ROOT / 'crates/ferraloom-game/tests/java/level/NeighborVectors.java')], check=True)
    subprocess.run([str(JDK / 'java'), '-cp', directory + ':' + CLASSPATH,
                    'NeighborVectors', 'generate', directory + '/NeighborFixture.java'],
                   check=True, cwd=directory)
    # Permit an oracle subclass of the generated test fixture, without changing
    # the shared fixture generator or any native method.
    generated = Path(directory) / 'NeighborFixture.java'
    generated.write_text(generated.read_text().replace('final class MutationFixture', 'class MutationFixture'))
    subprocess.run([str(JDK / 'javac'), '-cp', directory + ':' + CLASSPATH, '-d', directory,
                    directory + '/NeighborFixture.java', str(HERE / 'BlockItemVectors.java'), str(HERE / 'PlacementVectors.java')], check=True)
    native = subprocess.check_output([str(JDK / 'java'), '-Xmx512M', '-cp',
                                      directory + ':' + CLASSPATH, 'PlacementVectors'], cwd=directory)
    # Native bootstrap logging is not part of the vectors.
    vectors = b'\n'.join(line for line in native.splitlines()
                         if line.startswith((b'standing\t', b'double\t', b'scaffolding\t', b'water-use-on\t'))) + b'\n'
    fixture = HERE / 'placement.tsv.gz'
    if '--check' in sys.argv:
        if gzip.decompress(fixture.read_bytes()) != vectors:
            raise SystemExit('native item placement-subclass vectors differ')
    else:
        fixture.write_bytes(gzip.compress(vectors, mtime=0))
    print(f'{len(vectors.splitlines())} native item placement-subclass vectors')
