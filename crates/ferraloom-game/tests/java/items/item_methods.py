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
with tempfile.TemporaryDirectory(prefix='ferraloom-item-methods-') as directory:
    subprocess.run([str(JDK / 'javac'), '-cp', CLASSPATH, '-d', directory,
                    str(HERE / 'ExtractItemMethods.java')], check=True)
    output = Path(directory) / 'item_methods.json'
    subprocess.run([str(JDK / 'java'), '-Xmx512M', '-cp', directory + ':' + CLASSPATH,
                    'ExtractItemMethods', str(output)], check=True, cwd=directory)
    expected = ROOT / 'data/generated/item_methods.json'
    if '--check' in sys.argv:
        if expected.read_bytes() != output.read_bytes():
            raise SystemExit('native item method metadata differs')
    else:
        expected.write_bytes(output.read_bytes())
