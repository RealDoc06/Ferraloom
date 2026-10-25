#!/usr/bin/env python3
"""Capture real vanilla registries/codecs/tags; shared caches stay in main checkout."""
from pathlib import Path
import os
import subprocess

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
java = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/registry-parity-vectors'
classes.mkdir(parents=True, exist_ok=True)
logs = cache / 'logs'
logs.mkdir(parents=True, exist_ok=True)
classpath = os.pathsep.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar'))]))
output = source.parents[1] / 'fixtures/registry_parity'
log = logs / 'registry-parity-vanilla-capture.log'
with log.open('w') as stream:
    for command in [
        [str(java / 'javac'), '-cp', classpath, '-d', str(classes), str(source / 'RegistryParityVectors.java')],
        [str(java / 'java'), '-Xmx1G', '-cp', str(classes) + os.pathsep + classpath, 'RegistryParityVectors', str(output)],
    ]:
        result = subprocess.run(command, cwd=classes, stdout=stream, stderr=subprocess.STDOUT)
        if result.returncode:
            raise SystemExit(f'Native capture failed ({result.returncode}); see {log}')
print(log.read_text()[-1200:])
print(f'Fixtures: {output}')
