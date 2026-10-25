#!/usr/bin/env python3
"""Capture real value-effect results and RNG consumption, including Unbreaking."""
from pathlib import Path
import os
import gzip
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
with tempfile.TemporaryDirectory(prefix='ferraloom-enchantment-values-') as directory:
    subprocess.run([str(JDK / 'javac'), '-cp', CLASSPATH, '-d', directory,
                    str(HERE / 'ValueEffectVectors.java')], check=True)
    native = subprocess.check_output([str(JDK / 'java'), '-Xmx256M', '-cp', directory + ':' + CLASSPATH,
                    'ValueEffectVectors'], cwd=directory)
    vectors = b'\n'.join(line for line in native.splitlines() if line.startswith(b'{')) + b'\n'
    expected = HERE / 'value-effects.tsv.gz'
    if '--check' in sys.argv:
        if gzip.decompress(expected.read_bytes()) != vectors:
            raise SystemExit('native enchantment value-effect vectors differ')
    else:
        expected.write_bytes(gzip.compress(vectors, mtime=0))
    print(f'{len(vectors.splitlines())} native enchantment value-effect vectors')
