#!/usr/bin/env python3
"""Capture immutable GameEvent notification radii from the real registry."""
from pathlib import Path
import json
import os
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[6]
CACHE = Path('/home/doc/code/Ferraloom/.cache')
VANILLA = Path(os.environ.get('FERRALOOM_VANILLA_DIR', CACHE / 'vanilla/26.3'))
JDK = Path(os.environ.get('JAVA_HOME', CACHE / 'toolchains/jdk-25.0.4.1+1')) / 'bin'
CLASSPATH = ':'.join(map(str, [VANILLA / 'versions/26.3/server-26.3.jar',
                              *sorted((VANILLA / 'libraries').rglob('*.jar'))]))
with tempfile.TemporaryDirectory(prefix='ferraloom-game-events-') as directory:
    subprocess.run([str(JDK / 'javac'), '-cp', CLASSPATH, '-d', directory,
                    str(HERE / 'ExtractGameEvents.java')], check=True)
    result = subprocess.run([str(JDK / 'java'), '-Xmx256M', '-cp', directory + ':' + CLASSPATH,
                             'ExtractGameEvents'], capture_output=True, text=True, check=True, cwd=directory)
    values = json.loads(next(line.removeprefix('facts\t') for line in result.stdout.splitlines()
                             if line.startswith('facts\t')))
    expected = ROOT / 'data/generated/game_events.json'
    if '--check' in sys.argv:
        if json.loads(expected.read_text()) != values:
            raise SystemExit('native game event notification radii differ')
    else:
        expected.write_text(json.dumps(values, indent=2) + '\n')
    print(f'{len(values)} native game event notification radii')
