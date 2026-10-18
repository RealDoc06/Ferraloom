#!/usr/bin/env python3
"""Regenerate focused redstone oracles using the shared 26.3 jar and JDK."""
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[6]
CACHE = Path('/home/doc/code/Ferraloom/.cache')
JAVA = CACHE / 'toolchains/jdk-25.0.4.1+1/bin'
VANILLA = CACHE / 'vanilla/26.3'
CLASSPATH = os.pathsep.join(map(str, [VANILLA / 'versions/26.3/server-26.3.jar', *sorted((VANILLA / 'libraries').rglob('*.jar'))]))
HERE = Path(__file__).resolve().parent
with tempfile.TemporaryDirectory() as temporary:
    classes = Path(temporary)
    neighbor = ROOT / 'crates/ferraloom-game/tests/java/level/NeighborVectors.java'
    subprocess.run([str(JAVA / 'javac'), '-cp', CLASSPATH, '-d', temporary, str(neighbor)], check=True)
    cp = temporary + os.pathsep + CLASSPATH
    fixture = classes / 'NeighborFixture.java'
    subprocess.run([str(JAVA / 'java'), '-Xmx512M', '-cp', cp, 'NeighborVectors', 'generate', str(fixture)], cwd=classes, check=True)
    fixture.write_text(fixture.read_text().replace('final class MutationFixture', 'class MutationFixture'))
    compiled = subprocess.run([str(JAVA / 'javac'), '-cp', cp, '-d', temporary, str(fixture), str(HERE / 'RedstoneVectors.java')], capture_output=True, text=True)
    if compiled.returncode:
        raise RuntimeError(compiled.stderr[-8000:])
    result = subprocess.run([str(JAVA / 'java'), '-Xmx512M', '-cp', cp, 'RedstoneBootstrap'], cwd=classes, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(result.stderr[-8000:])
    lines = [line for line in result.stdout.splitlines() if line.startswith(('signal:', 'relay:', 'piston-', 'wire:', 'moving:', 'push:', 'comparator:', 'placement:', 'hopper:'))]
    if not lines:
        raise RuntimeError('Vanilla emitted no redstone cases')
    (HERE / 'vectors.tsv').write_text('\n'.join(lines) + '\n')
    print(f'{len(lines)} vanilla cases')
