#!/usr/bin/env python3
"""Capture actual bookshelf getters/mutations, hit pixels and block interactions."""
from pathlib import Path
import gzip
import subprocess

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/bookshelf-hook-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [classes, vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar'))]))

def run(args):
    result = subprocess.run(args, cwd=classes, text=True, capture_output=True)
    if result.returncode:
        print(result.stdout[-2000:])
        print(result.stderr)
        raise SystemExit(result.returncode)
    return result.stdout

run([str(jdk / 'javac'), '-cp', cp, '-d', str(classes), str(source.parent.parent / 'level/NeighborVectors.java')])
run([str(jdk / 'java'), '-cp', cp, 'NeighborVectors', 'generate', str(classes / 'NeighborFixture.java')])
run([str(jdk / 'javac'), '-cp', cp, '-d', str(classes), str(classes / 'NeighborFixture.java'), str(source / 'BookShelfHookVectors.java')])
output = run([str(jdk / 'java'), '-Xmx512m', '-cp', cp, 'BookShelfHookVectors'])
rows = [line for line in output.splitlines() if line.startswith(('shelf-hit\t', 'shelf-hook\t', 'shelf-use\t', 'shelf-signal\t'))]
(source / 'bookshelf-hooks.tsv.gz').write_bytes(gzip.compress(('\n'.join(rows) + '\n').encode(), mtime=0))
from collections import Counter
print(dict(Counter(line.split('\t', 1)[0] for line in rows)))
