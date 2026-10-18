#!/usr/bin/env python3
"""Capture native Enchantment.applyEffects ordering and shared predicate RNG."""
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
with tempfile.TemporaryDirectory(prefix='ferraloom-enchantment-effect-lists-') as directory:
    subprocess.run([str(JDK / 'javac'), '-cp', CLASSPATH, '-d', directory,
                    str(HERE / 'EffectListVectors.java'),
                    str(ROOT / 'crates/ferraloom-game/tests/java/level/NeighborVectors.java')], check=True)
    result = subprocess.run([str(JDK / 'java'), '-Xmx256M', '-cp', directory + ':' + CLASSPATH,
                             'EffectListVectors'], cwd=directory, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if result.returncode:
        print(result.stdout.decode()[-6000:])
        raise SystemExit(result.returncode)
    vectors = b'\n'.join(line for line in result.stdout.splitlines() if line.startswith(b'list\t')) + b'\n'
    expected = HERE / 'effect-lists.tsv.gz'
    if '--check' in sys.argv:
        if gzip.decompress(expected.read_bytes()) != vectors:
            raise SystemExit('native enchantment effect-list vectors differ')
    else:
        expected.write_bytes(gzip.compress(vectors, mtime=0))
    print(f'{len(vectors.splitlines())} native enchantment effect-list vectors')
