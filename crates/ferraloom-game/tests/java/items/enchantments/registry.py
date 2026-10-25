#!/usr/bin/env python3
"""Compare the real vanilla bootstrap's enchantment component presence."""
from pathlib import Path
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
with tempfile.TemporaryDirectory(prefix='ferraloom-enchantment-registry-') as directory:
    subprocess.run([str(JDK / 'javac'), '-cp', CLASSPATH, '-d', directory,
                    str(HERE / 'RegistryVectors.java'),
                    str(ROOT / 'crates/ferraloom-game/tests/java/level/NeighborVectors.java')], check=True)
    result = subprocess.run([str(JDK / 'java'), '-Xmx256M', '-cp', directory + ':' + CLASSPATH,
                             'RegistryVectors'], cwd=directory, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if result.returncode:
        print(result.stdout.decode()[-6000:])
        raise SystemExit(result.returncode)
    rows = [line for line in result.stdout.splitlines() if line.startswith(b'enchantment-registry\t')]
    vectors = b'\n'.join(rows) + b'\n'
    expected = HERE / 'registry.tsv'
    if sys.argv[1:] == ['--check']:
        if expected.read_bytes() != vectors:
            raise SystemExit('native enchantment registry component presence differs')
    elif sys.argv[1:]:
        raise SystemExit('usage: registry.py [--check]')
    else:
        expected.write_bytes(vectors)
    print(f'{len(rows)} native enchantment registry entries')
