#!/usr/bin/env python3
"""Compile the existing concrete Level fixture and call vanilla plant methods."""
from pathlib import Path
import subprocess
import gzip
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[5]
CACHE = Path('/home/doc/code/Ferraloom/.cache')
JDK = CACHE / 'toolchains/jdk-25.0.4.1+1/bin'
VANILLA = CACHE / 'vanilla/26.3'
CLASSPATH = ':'.join(map(str, [VANILLA/'versions/26.3/server-26.3.jar', *sorted((VANILLA/'libraries').rglob('*.jar'))]))
NEIGHBORS = ROOT/'crates/ferraloom-game/tests/java/level/NeighborVectors.java'
with tempfile.TemporaryDirectory(prefix='ferraloom-plants-java-') as directory:
    subprocess.run([str(JDK/'javac'), '-cp', CLASSPATH, '-d', directory, str(NEIGHBORS)], check=True)
    subprocess.run([str(JDK/'java'), '-cp', directory+':'+CLASSPATH, 'NeighborVectors', 'generate', directory+'/NeighborFixture.java'], check=True, cwd=directory)
    fixture = Path(directory)/'NeighborFixture.java'
    fixture.write_text(fixture.read_text().replace('final class MutationFixture', 'class MutationFixture'))
    subprocess.run([str(JDK/'javac'), '-cp', directory+':'+CLASSPATH, '-d', directory, directory+'/NeighborFixture.java', str(HERE/'PlantsVectors.java')], check=True)
    with (Path(directory)/'survival.tsv').open('w') as output:
        subprocess.run([str(JDK/'java'), '-Xmx512M', '-cp', directory+':'+CLASSPATH, 'PlantsVectors'], stdout=output, check=True, cwd=directory)

    lines=(Path(directory)/'survival.tsv').read_text().splitlines()
    vectors='\n'.join(line for line in lines if line.startswith(('survive\t','update\t'))) + '\n'
    (HERE/'survival.tsv.gz').write_bytes(gzip.compress(vectors.encode(),mtime=0))
    (HERE/'survival.tsv').unlink(missing_ok=True)
    growth='\n'.join(line for line in lines if line.startswith(('grow\t','speed\t'))) + '\n'
    (HERE/'growth.tsv.gz').write_bytes(gzip.compress(growth.encode(),mtime=0))
    print(f'{len(vectors.splitlines())} vanilla plant vectors; {len(growth.splitlines())} crop growth vectors')

    shapes='\n'.join(line for line in lines if line.startswith('shape\t')) + '\n'
    (HERE/'shapes.tsv.gz').write_bytes(gzip.compress(shapes.encode(),mtime=0))
    print(f'{len(shapes.splitlines())} vanilla plant shape vectors')
