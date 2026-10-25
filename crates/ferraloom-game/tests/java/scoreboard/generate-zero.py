#!/usr/bin/env python3
"""Capture zero-result feedback and packet bytes from the actual vanilla dispatcher."""
from pathlib import Path
import subprocess
import tempfile

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
java = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classpath = ':'.join(map(str, [vanilla/'versions/26.3/server-26.3.jar', *sorted((vanilla/'libraries').rglob('*.jar')), *sorted((vanilla/'tree-harness').glob('*.jar'))]))
with tempfile.TemporaryDirectory(prefix='scoreboard-zero-') as directory:
    subprocess.run([str(java/'javac'), '-cp', classpath, '-d', directory, str(source/'CommandVectors.java'), str(source/'ZeroScoreVectors.java')], cwd=directory, check=True)
    subprocess.run([str(java/'java'), '-Xmx512m', '-XX:+EnableDynamicAgentLoading', '-Dnet.bytebuddy.experimental=true', '-cp', f'{directory}:{classpath}', 'ZeroScoreVectors', str(source/'zero.json')], cwd=directory, check=True)
