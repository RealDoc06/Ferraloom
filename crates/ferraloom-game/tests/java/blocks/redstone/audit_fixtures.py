#!/usr/bin/env python3
"""Check live redstone fixture assumptions against native level code."""
import os
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent
CACHE = Path('/home/doc/code/Ferraloom/.cache')
JAVA = CACHE / 'toolchains/jdk-25.0.4.1+1/bin'
VANILLA = CACHE / 'vanilla/26.3'
CLASSES = CACHE / 'tools/redstone-fixture-audit'
CLASSPATH = os.pathsep.join(map(str, [VANILLA / 'versions/26.3/server-26.3.jar', *sorted((VANILLA / 'libraries').rglob('*.jar'))]))
CLASSES.mkdir(parents=True, exist_ok=True)
logs = CACHE / 'logs'
logs.mkdir(parents=True, exist_ok=True)
with (logs / 'redstone-fixture-native-audit.log').open('w') as log:
    subprocess.run([str(JAVA / 'javac'), '-cp', CLASSPATH, '-d', str(CLASSES), *map(str, [HERE / 'RedstoneIdentityVectors.java', HERE / 'DeviceVectors.java', HERE / 'FixtureSetupVectors.java'])], stdout=log, stderr=subprocess.STDOUT, check=True)
    subprocess.run([str(JAVA / 'java'), '-Xmx768M', '-cp', str(CLASSES) + os.pathsep + CLASSPATH, 'FixtureSetupVectors'], cwd=CLASSES, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=180)
print('Native redstone fixture checks passed')
