#!/usr/bin/env python3
"""Call native plant placement virtuals on the existing concrete Level fixture."""
from pathlib import Path
import gzip
import os
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[5]
CACHE = Path('/home/doc/code/Ferraloom/.cache')
JDK = Path(os.environ.get('JAVA_HOME', CACHE / 'toolchains/jdk-25.0.4.1+1')) / 'bin'
VANILLA = CACHE / 'vanilla/26.3'
CLASSPATH = ':'.join(map(str, [VANILLA / 'versions/26.3/server-26.3.jar',
                               *sorted((VANILLA / 'libraries').rglob('*.jar'))]))
with tempfile.TemporaryDirectory(prefix='ferraloom-plant-placement-') as directory:
    subprocess.run([str(JDK / 'javac'), '-cp', CLASSPATH, '-d', directory,
                    str(ROOT / 'crates/ferraloom-game/tests/java/level/NeighborVectors.java')], check=True)
    subprocess.run([str(JDK / 'java'), '-cp', directory + ':' + CLASSPATH,
                    'NeighborVectors', 'generate', directory + '/NeighborFixture.java'],
                   check=True, cwd=directory)
    subprocess.run([str(JDK / 'javac'), '-cp', directory + ':' + CLASSPATH, '-d', directory,
                    directory + '/NeighborFixture.java', str(HERE / 'PlantPlacementVectors.java')], check=True)
    native = subprocess.check_output([str(JDK / 'java'), '-Xmx512M', '-cp',
                                      directory + ':' + CLASSPATH, 'PlantPlacementVectors'], cwd=directory)
    # Native bootstrap logging is not part of the vectors.
    vectors = b'\n'.join(line for line in native.splitlines()
                         if line.startswith((b'survive\t', b'wall\t', b'wet\t'))) + b'\n'
    fixture = HERE / 'plant-placement.tsv.gz'
    if '--check' in sys.argv:
        if gzip.decompress(fixture.read_bytes()) != vectors:
            raise SystemExit('native plant placement vectors differ')
    else:
        fixture.write_bytes(gzip.compress(vectors, mtime=0))
    print(f'{len(vectors.splitlines())} native plant placement vectors')
