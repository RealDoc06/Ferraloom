#!/usr/bin/env python3
"""Call native BlockItem virtuals on the existing concrete Level fixture."""
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
with tempfile.TemporaryDirectory(prefix='ferraloom-block-item-') as directory:
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
                    directory + '/NeighborFixture.java', str(HERE / 'BlockItemVectors.java')], check=True)
    native = subprocess.check_output([str(JDK / 'java'), '-Xmx512M', '-cp',
                                      directory + ':' + CLASSPATH, 'BlockItemVectors'], cwd=directory)
    # Native bootstrap logging is not part of the vectors.
    vectors = b'\n'.join(line for line in native.splitlines()
                         if line.startswith((b'place\t', b'plant\t', b'property\t', b'duplicate\t', b'be\t'))) + b'\n'
    fixture = HERE / 'block-item.tsv.gz'
    if '--check' in sys.argv:
        if gzip.decompress(fixture.read_bytes()) != vectors:
            raise SystemExit('native BlockItem vectors differ')
    else:
        fixture.write_bytes(gzip.compress(vectors, mtime=0))
    print(f'{len(vectors.splitlines())} native BlockItem vectors')
