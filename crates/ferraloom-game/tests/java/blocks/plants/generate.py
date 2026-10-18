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

    bamboo='\n'.join(line for line in lines if line.startswith('bamboo-grow\t')) + '\n'
    (HERE/'bamboo.tsv.gz').write_bytes(gzip.compress(bamboo.encode(),mtime=0))
    print(f'{len(bamboo.splitlines())} vanilla bamboo crown vectors')

    clones='\n'.join(line for line in lines if line.startswith('clone\t')) + '\n'
    (HERE/'clone-items.tsv.gz').write_bytes(gzip.compress(clones.encode(),mtime=0))
    print(f'{len(clones.splitlines())} vanilla plant clone-item vectors')
    ground='\n'.join(line for line in lines if line.startswith(('nylium-alive\t','bonemeal-kind\t'))) + '\n'
    (HERE/'ground.tsv.gz').write_bytes(gzip.compress(ground.encode(),mtime=0))
    print(f'{len(ground.splitlines())} vanilla nylium and bonemeal metadata vectors')
    placement='\n'.join(line for line in lines if line.startswith('vine-place\t')) + '\n'
    (HERE/'placement.tsv.gz').write_bytes(gzip.compress(placement.encode(),mtime=0))
    print(f'{len(placement.splitlines())} vanilla vine placement vectors')
    states='\n'.join(line for line in lines if line.startswith('plant-place\t')) + '\n'
    (HERE/'placement-states.tsv.gz').write_bytes(gzip.compress(states.encode(),mtime=0))
    print(f'{len(states.splitlines())} vanilla plant placement states and RNG vectors')
    trees='\n'.join(line for line in lines if line.startswith('tree-select\t')) + '\n'
    (HERE/'tree-selection.tsv.gz').write_bytes(gzip.compress(trees.encode(),mtime=0))
    print(f'{len(trees.splitlines())} vanilla weighted tree-selection vectors')
    frost='\n'.join(line for line in lines if line.startswith('frosted-place\t')) + '\n'
    (HERE/'frosted-place.tsv.gz').write_bytes(gzip.compress(frost.encode(),mtime=0))
    print(f'{len(frost.splitlines())} vanilla frosted-ice placement schedules')

    grass='\n'.join(line for line in lines if line.startswith('grass-support\t')) + '\n'
    (HERE/'grass-support.tsv.gz').write_bytes(gzip.compress(grass.encode(),mtime=0))
    print(f'{len(grass.splitlines())} vanilla spreading snowy support vectors')

    scans='\n'.join(line for line in lines if line.startswith('eyeblossom-scan\t')) + '\n'
    (HERE/'eyeblossom-scan.tsv.gz').write_bytes(gzip.compress(scans.encode(),mtime=0))
    print(f'{len(scans.splitlines())} native section-boundary flower traversals')

    targets='\n'.join(line for line in lines if line.startswith('non-server-target\t')) + '\n'
    (HERE/'non-server-targets.tsv.gz').write_bytes(gzip.compress(targets.encode(),mtime=0))
    print(f'{len(targets.splitlines())} native non-ServerLevel bonemeal targets and RNG vectors')
