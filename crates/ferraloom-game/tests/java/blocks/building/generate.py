#!/usr/bin/env python3
"""Capture actual 26.3 building-block placement and shape-update vectors."""
import argparse
import gzip
import os
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[5]
CACHE = Path('/home/doc/code/Ferraloom/.cache')
JAVA = CACHE / 'toolchains/jdk-25.0.4.1+1/bin'
VANILLA = CACHE / 'vanilla/26.3'
CLASSPATH = os.pathsep.join(map(str, [VANILLA / 'versions/26.3/server-26.3.jar', *sorted((VANILLA / 'libraries').rglob('*.jar'))]))
parser = argparse.ArgumentParser()
parser.add_argument('family', nargs='?', choices=['orientation', 'stairs-slabs', 'connected', 'attachments', 'decorative', 'fire-powder'], default='orientation')
family = parser.parse_args().family
source, bootstrap, expected = {
    'orientation': ('OrientationVectors.java', 'OrientationBootstrap', 42012),
    'stairs-slabs': ('StairSlabVectors.java', 'StairSlabBootstrap', 281554),
    'connected': ('ConnectedVectors.java', 'ConnectedBootstrap', 84864),
    'attachments': ('AttachmentVectors.java', 'AttachmentBootstrap', 268940),
    'decorative': ('DecorativeVectors.java', 'DecorativeBootstrap', 35092),
    'fire-powder': ('FirePowderVectors.java', 'FirePowderBootstrap', 48298),
}[family]
with tempfile.TemporaryDirectory() as temporary:
    classes = Path(temporary)
    neighbor = ROOT / 'crates/ferraloom-game/tests/java/level/NeighborVectors.java'
    subprocess.run([str(JAVA / 'javac'), '-cp', CLASSPATH, '-d', temporary, str(neighbor)], check=True)
    cp = temporary + os.pathsep + CLASSPATH
    fixture = classes / 'NeighborFixture.java'
    subprocess.run([str(JAVA / 'java'), '-Xmx512M', '-cp', cp, 'NeighborVectors', 'generate', str(fixture)], cwd=classes, check=True)
    fixture.write_text(fixture.read_text().replace('final class MutationFixture', 'class MutationFixture'))
    dependencies = [str(HERE / 'AttachmentVectors.java')] if family in ('decorative', 'fire-powder') else []
    compiled = subprocess.run([str(JAVA / 'javac'), '-cp', cp, '-d', temporary, str(fixture), *dependencies, str(HERE / source)], capture_output=True, text=True)
    if compiled.returncode:
        raise RuntimeError(compiled.stderr[-8000:])
    result = subprocess.run([str(JAVA / 'java'), '-Xmx512M', '-cp', cp, bootstrap], cwd=classes, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(result.stderr[-8000:])
    lines = [line.removeprefix('BUILDING\t') for line in result.stdout.splitlines() if line.startswith('BUILDING\t')]
    if expected is not None and len(lines) != expected:
        from collections import Counter
        raise RuntimeError(f'Expected {expected} native {family} cases, got {len(lines)}: {Counter(line.split(chr(9))[0] for line in lines)}')
    (HERE / f'{family}.tsv.gz').write_bytes(gzip.compress(('\n'.join(lines) + '\n').encode(), mtime=0))
    print(f'{len(lines)} native {family} cases')
    if family == 'connected':
        print(next(line for line in result.stdout.splitlines() if line.startswith('Native atlas:')))
