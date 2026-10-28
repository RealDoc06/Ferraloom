#!/usr/bin/env python3
"""Run the native plain function compilation and UTF-8 file reading."""
from pathlib import Path
import subprocess
cache = Path('/home/doc/code/Ferraloom/.cache')
source = Path(__file__).resolve().parent
vanilla = cache / 'vanilla/26.3'
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
classes = cache / 'tools/function-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar',
    *sorted((vanilla / 'libraries').rglob('*.jar')),
    *sorted((vanilla / 'tree-harness').rglob('*.jar'))]))
subprocess.run([str(jdk / 'javac'), '-cp', cp, '-d', str(classes), str(source / 'PlainVectors.java'), str(source / 'CallVectors.java'), str(source / 'AccumulatorVectors.java'), str(source / 'FunctionVectors.java')], check=True)
subprocess.run([str(jdk / 'java'), '-Xmx1g', '-Dnet.bytebuddy.experimental=true', '-XX:+EnableDynamicAgentLoading', '-cp', f'{classes}:{cp}', 'PlainVectors', str(source / 'plain.json'), str(source / 'readers.json')], cwd=vanilla, check=True)
subprocess.run([str(jdk / 'java'), '-Xmx1g', '-Dnet.bytebuddy.experimental=true', '-XX:+EnableDynamicAgentLoading', '-cp', f'{classes}:{cp}', 'CallVectors', str(source / 'calls.json')], cwd=vanilla, check=True)
subprocess.run([str(jdk / 'java'), '-Xmx1g', '-cp', f'{classes}:{cp}', 'AccumulatorVectors', str(source / 'accumulators.json')], cwd=vanilla, check=True)
subprocess.run([str(jdk / 'java'), '-Xmx1g', '-Dnet.bytebuddy.experimental=true', '-XX:+EnableDynamicAgentLoading', '-cp', f'{classes}:{cp}', 'FunctionVectors', str(source / 'functions.json')], cwd=vanilla, check=True)
