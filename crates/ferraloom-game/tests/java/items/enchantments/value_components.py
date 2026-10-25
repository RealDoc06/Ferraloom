#!/usr/bin/env python3
"""Capture native enchantment value component codecs and context restrictions."""
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
with tempfile.TemporaryDirectory(prefix='ferraloom-enchantment-value-components-') as directory:
    subprocess.run([str(JDK / 'javac'), '-cp', CLASSPATH, '-d', directory,
                    str(HERE / 'ValueComponentVectors.java'),
                    str(ROOT / 'crates/ferraloom-game/tests/java/level/NeighborVectors.java')], check=True)
    result = subprocess.run([str(JDK / 'java'), '-Xmx256M', '-cp', directory + ':' + CLASSPATH,
                             'ValueComponentVectors'], cwd=directory, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if result.returncode:
        print(result.stdout.decode()[-6000:])
        raise SystemExit(result.returncode)
    vectors = b'\n'.join(line for line in result.stdout.splitlines() if line.startswith(b'component\t')) + b'\n'
    expected = HERE / 'value-components.tsv.gz'
    if '--check' in sys.argv:
        if gzip.decompress(expected.read_bytes()) != vectors:
            raise SystemExit('native enchantment value-component vectors differ')
    else:
        expected.write_bytes(gzip.compress(vectors, mtime=0))
    print(f'{len(vectors.splitlines())} native enchantment value-component vectors')
