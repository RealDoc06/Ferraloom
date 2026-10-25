#!/usr/bin/env python3
"""Capture native isPathfindable, including reloaded source/flowing fluid tags."""
import gzip
from pathlib import Path
import subprocess
import tempfile

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
java = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classpath = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar'))]))
with tempfile.TemporaryDirectory(prefix='pathfindability-vectors-') as directory:
    temporary = Path(directory)
    subprocess.run([str(java/'javac'), '-cp', classpath, '-d', directory, str(source/'PathfindabilityVectors.java')], check=True)
    subprocess.run([str(java/'java'), '-Xmx512m', '-cp', f'{directory}:{classpath}', 'PathfindabilityVectors', str(temporary/'states.tsv'), str(temporary/'tags.tsv')], cwd=directory, check=True)
    (source/'pathfindability.tsv.gz').write_bytes(gzip.compress((temporary/'states.tsv').read_bytes(), mtime=0))
    (source/'pathfindability-tags.tsv').write_bytes((temporary/'tags.tsv').read_bytes())
