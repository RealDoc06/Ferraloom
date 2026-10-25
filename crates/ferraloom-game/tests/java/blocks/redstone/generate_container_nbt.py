#!/usr/bin/env python3
"""Capture or check native dispenser/dropper/hopper saveAdditional NBT."""
import argparse
import os
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent
CACHE = Path('/home/doc/code/Ferraloom/.cache')
JAVA = CACHE / 'toolchains/jdk-25.0.4.1+1/bin'
VANILLA = CACHE / 'vanilla/26.3'
CLASSES = CACHE / 'tools/redstone-container-nbt'
CLASSPATH = os.pathsep.join(map(str, [VANILLA / 'versions/26.3/server-26.3.jar', *sorted((VANILLA / 'libraries').rglob('*.jar'))]))
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--check', action='store_true')
args = parser.parse_args()
CLASSES.mkdir(parents=True, exist_ok=True)
logs = CACHE / 'logs'
logs.mkdir(parents=True, exist_ok=True)
output = CLASSES / 'container_nbt.tsv'
with (logs / 'redstone-container-nbt-native.log').open('w') as log:
    subprocess.run([str(JAVA / 'javac'), '-cp', CLASSPATH, '-d', str(CLASSES), str(HERE / 'ContainerNbtVectors.java'), str(HERE.parent.parent / 'level/NeighborVectors.java')], stdout=log, stderr=subprocess.STDOUT, check=True)
    subprocess.run([str(JAVA / 'java'), '-Xmx512M', '-cp', str(CLASSES) + os.pathsep + CLASSPATH, 'ContainerNbtVectors', str(output)], cwd=CLASSES, stdout=log, stderr=subprocess.STDOUT, check=True)
fixture = HERE / 'container_nbt.tsv'
if args.check:
    if fixture.read_text() != output.read_text():
        raise AssertionError('Container NBT differs from fresh vanilla capture')
else:
    fixture.write_text(output.read_text())
print(f'Native container NBT: {len(output.read_text().splitlines())} cases')
