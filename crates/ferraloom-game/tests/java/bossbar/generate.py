#!/usr/bin/env python3
"""Capture real 26.3 boss-event model and packet behavior, without a Rust build."""
from pathlib import Path
import subprocess
import tempfile

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
java = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
jars = [vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar')), *sorted((vanilla / 'tree-harness').glob('*.jar'))]
classpath = ':'.join(map(str, jars))
with tempfile.TemporaryDirectory(prefix='boss-event-vectors-') as directory:
    subprocess.run([str(java/'javac'), '-cp', classpath, '-d', directory, str(source/'BossEventVectors.java')], cwd=directory, check=True)
    subprocess.run([str(java/'java'), '-Xmx512m', '-XX:+EnableDynamicAgentLoading', '-Dnet.bytebuddy.experimental=true', '-cp', f'{directory}:{classpath}', 'BossEventVectors', str(source/'events.json')], cwd=directory, check=True)
