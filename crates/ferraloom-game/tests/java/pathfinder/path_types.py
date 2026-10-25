#!/usr/bin/env python3
"""Capture native raw classification and cache/neighbor observations."""
import gzip
from pathlib import Path
import subprocess
import tempfile

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
java = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classpath = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar'))]))
with tempfile.TemporaryDirectory(prefix='path-type-vectors-') as directory:
    temporary = Path(directory)
    def run(args):
        subprocess.run(args, cwd=directory, check=True)
    run([str(java/'javac'), '-cp', classpath, '-d', directory, str(source.parent/'level/NeighborVectors.java')])
    run([str(java/'java'), '-Xmx512m', '-cp', f'{directory}:{classpath}', 'NeighborVectors', 'generate', str(temporary/'NeighborFixture.java')])
    run([str(java/'javac'), '-cp', f'{directory}:{classpath}', '-d', directory, str(temporary/'NeighborFixture.java'), str(source/'PathTypeCacheVectors.java')])
    run([str(java/'java'), '-Xmx512m', '-cp', f'{directory}:{classpath}', 'PathTypeCacheVectors', str(temporary/'types.tsv'), str(temporary/'cache.json')])
    (source/'path-types.tsv.gz').write_bytes(gzip.compress((temporary/'types.tsv').read_bytes(), mtime=0))
    (source/'path-type-cache.json').write_bytes((temporary/'cache.json').read_bytes())
