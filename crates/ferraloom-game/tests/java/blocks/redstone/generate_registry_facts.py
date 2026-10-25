#!/usr/bin/env python3
"""Capture or check independent vanilla bootstrap dispenser/instrument facts."""
import argparse
import json
import os
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent
CACHE = Path('/home/doc/code/Ferraloom/.cache')
JAVA = CACHE / 'toolchains/jdk-25.0.4.1+1/bin'
VANILLA = CACHE / 'vanilla/26.3'
CLASSES = CACHE / 'tools/redstone-registry-facts'
CLASSPATH = os.pathsep.join(map(str, [VANILLA / 'versions/26.3/server-26.3.jar', *sorted((VANILLA / 'libraries').rglob('*.jar'))]))
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--check', action='store_true')
args = parser.parse_args()
CLASSES.mkdir(parents=True, exist_ok=True)
logs = CACHE / 'logs'
logs.mkdir(parents=True, exist_ok=True)
output = CLASSES / 'registry_facts.json'
with (logs / 'redstone-registry-bootstrap.log').open('w') as log:
    subprocess.run([str(JAVA / 'javac'), '-cp', CLASSPATH, '-d', str(CLASSES), str(HERE / 'RegistryFactVectors.java')], stdout=log, stderr=subprocess.STDOUT, check=True)
    subprocess.run([str(JAVA / 'java'), '-Xmx512M', '-cp', str(CLASSES) + os.pathsep + CLASSPATH, 'RegistryFactVectors', str(output)], cwd=CLASSES, stdout=log, stderr=subprocess.STDOUT, check=True)
native = json.loads(output.read_text())
fixture = HERE / 'registry_facts.json'
if args.check:
    if json.loads(fixture.read_text()) != native:
        raise AssertionError('Committed dispenser/note facts differ from fresh vanilla bootstrap')
else:
    fixture.write_text(output.read_text())
print(f"Native bootstrap: {len(native['items'])} items, {len(native['blocks'])} blocks/{native['state_count']} states, {len(native['instruments'])} instruments")
