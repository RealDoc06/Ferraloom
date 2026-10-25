#!/usr/bin/env python3
"""Extract the pinned registry's item classes and exact virtual signatures."""
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
with tempfile.TemporaryDirectory(prefix='ferraloom-sound-holders-') as directory:
    subprocess.run([str(JDK / 'javac'), '-cp', CLASSPATH, '-d', directory,
                    str(HERE / 'SoundHolderVectors.java')], check=True)
    native = subprocess.check_output([str(JDK / 'java'), '-Xmx256M', '-cp', directory + ':' + CLASSPATH,
                    'SoundHolderVectors'], cwd=directory)
    vectors = b'\n'.join(line for line in native.splitlines() if line.startswith((b'sound\t', b'missing\t'))) + b'\n'
    expected = HERE / 'sound-holders.tsv'
    if '--check' in sys.argv:
        if expected.read_bytes() != vectors:
            raise SystemExit('native sound holder vectors differ')
    else:
        expected.write_bytes(vectors)
    print(f'{len(vectors.splitlines())} native sound holder vectors')
