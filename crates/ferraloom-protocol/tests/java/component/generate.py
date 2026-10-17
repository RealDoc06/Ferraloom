#!/usr/bin/env python3
"""Run on devbox. All compiled Java and runtime logs stay outside the checkout."""
from pathlib import Path
import os, subprocess, tempfile
root = Path(__file__).resolve().parents[3]
cache = Path('/home/doc/code/Ferraloom/.cache')
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
jars = list(vanilla.glob('versions/**/*.jar')) + list(vanilla.glob('libraries/**/*.jar'))
classpath = os.pathsep.join(map(str, jars))
with tempfile.TemporaryDirectory(prefix='ferraloom-components-') as build:
    subprocess.run([str(jdk / 'javac'), '-cp', classpath, '-d', build, str(Path(__file__).with_name('ComponentFixtures.java'))], check=True)
    subprocess.run([str(jdk / 'java'), '-cp', build + os.pathsep + classpath, 'ComponentFixtures', str(root / 'src/component/fixtures.json')], cwd=build, check=True)
