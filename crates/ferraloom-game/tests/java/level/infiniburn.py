#!/usr/bin/env python3
"""Capture actual 26.3 dimension holder sets and block membership."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
CACHE = Path('/home/doc/code/Ferraloom/.cache')
JDK = Path(os.environ.get('JAVA_HOME', CACHE / 'toolchains/jdk-25.0.4.1+1')) / 'bin'
VANILLA = CACHE / 'vanilla/26.3'
CLASSPATH = ':'.join(map(str, [VANILLA / 'versions/26.3/server-26.3.jar',
                              *sorted((VANILLA / 'libraries').rglob('*.jar'))]))
with tempfile.TemporaryDirectory(prefix='ferraloom-infiniburn-') as directory:
    subprocess.run([str(JDK / 'javac'), '-cp', CLASSPATH, '-d', directory,
                    str(HERE / 'NeighborVectors.java'),
                    str(HERE / 'InfiniburnVectors.java')], check=True)
    native = subprocess.check_output([str(JDK / 'java'), '-Xmx512M', '-cp',
                                      directory + ':' + CLASSPATH, 'InfiniburnVectors'],
                                     cwd=directory)
    vectors = b'\n'.join(line for line in native.splitlines()
                         if line.startswith(b'infiniburn\t')) + b'\n'
    expected = HERE / 'infiniburn.tsv'
    if '--check' in sys.argv:
        if expected.read_bytes() != vectors:
            raise SystemExit('native infiniburn vectors differ')
    else:
        expected.write_bytes(vectors)
    print(f'{len(vectors.splitlines())} native infiniburn holder sets')
