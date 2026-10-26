#!/usr/bin/env python3
"""Capture/recheck real vanilla PathFinder and concrete evaluator outcomes."""
from pathlib import Path
import gzip
import os
import subprocess
import sys
import tempfile

here = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
java = Path(os.environ.get('JAVA_HOME', str(cache/'toolchains/jdk-25.0.4.1+1')))/'bin'
vanilla = Path(os.environ.get('FERRALOOM_VANILLA_DIR', str(cache/'vanilla/26.3')))
classpath = ':'.join(map(str, [vanilla/'versions/26.3/server-26.3.jar', *sorted((vanilla/'libraries').rglob('*.jar'))]))
with tempfile.TemporaryDirectory(prefix='pathfinder-evaluators-') as directory:
    temp = Path(directory)
    def run(arguments):
        subprocess.run(arguments, cwd=temp, check=True)
    neighbors = here.parents[1]/'level/NeighborVectors.java'
    run([str(java/'javac'), '-cp', classpath, '-d', directory, str(neighbors)])
    run([str(java/'java'), '-Xmx512m', '-cp', f'{temp}:{classpath}', 'NeighborVectors', 'generate', str(temp/'NeighborFixture.java')])
    run([str(java/'javac'), '-cp', f'{temp}:{classpath}', '-d', directory, str(temp/'NeighborFixture.java'), str(here/'EvaluatorVectors.java')])
    run([str(java/'java'), '-Xmx768m', '-cp', f'{temp}:{classpath}', 'EvaluatorVectors', str(temp/'vectors.json')])
    actual = (temp/'vectors.json').read_bytes()
    fixture = here/'evaluators.json.gz'
    if '--capture' in sys.argv:
        fixture.write_bytes(gzip.compress(actual, mtime=0))
    elif actual != gzip.decompress(fixture.read_bytes()):
        raise SystemExit('native evaluator vectors differ')
