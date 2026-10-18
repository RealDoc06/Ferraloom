#!/usr/bin/env python3
"""Capture actual 26.3 building-block placement and shape-update vectors."""
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
with tempfile.TemporaryDirectory() as temporary:
    classes = Path(temporary)
    neighbor = ROOT / 'crates/ferraloom-game/tests/java/level/NeighborVectors.java'
    subprocess.run([str(JAVA / 'javac'), '-cp', CLASSPATH, '-d', temporary, str(neighbor)], check=True)
    cp = temporary + os.pathsep + CLASSPATH
    fixture = classes / 'NeighborFixture.java'
    subprocess.run([str(JAVA / 'java'), '-Xmx512M', '-cp', cp, 'NeighborVectors', 'generate', str(fixture)], cwd=classes, check=True)
    fixture.write_text(fixture.read_text().replace('final class MutationFixture', 'class MutationFixture'))
    compiled = subprocess.run([str(JAVA / 'javac'), '-cp', cp, '-d', temporary, str(fixture), str(HERE / 'OrientationVectors.java')], capture_output=True, text=True)
    if compiled.returncode:
        raise RuntimeError(compiled.stderr[-8000:])
    result = subprocess.run([str(JAVA / 'java'), '-Xmx512M', '-cp', cp, 'OrientationBootstrap'], cwd=classes, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(result.stderr[-8000:])
    lines = [line.removeprefix('BUILDING\t') for line in result.stdout.splitlines() if line.startswith('BUILDING\t')]
    if len(lines) != 42012:
        raise RuntimeError(f'Expected 42012 native orientation cases, got {len(lines)}')
    (HERE / 'orientation.tsv.gz').write_bytes(gzip.compress(('\n'.join(lines) + '\n').encode(), mtime=0))
    print(f'{len(lines)} native orientation/waterlogging cases')
