#!/usr/bin/env python3
"""Capture actual 26.3 pathfinder classes with a synthetic NodeEvaluator only."""
from pathlib import Path
import subprocess
import tempfile

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
java = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classpath = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar'))]))
with tempfile.TemporaryDirectory(prefix='pathfinder-vectors-') as directory:
    subprocess.run([str(java/'javac'), '-cp', classpath, '-d', directory, str(source/'CoreVectors.java')], check=True)
    subprocess.run([str(java/'java'), '-Xmx512m', '-cp', f'{directory}:{classpath}', 'CoreVectors', str(source/'core.json')], cwd=directory, check=True)
