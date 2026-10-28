#!/usr/bin/env python3
"""Capture actual 26.3 pack codecs and repository bootstrap behavior."""
from pathlib import Path
import subprocess
cache = Path('/home/doc/code/Ferraloom/.cache')
source = Path(__file__).resolve().parent
vanilla = cache / 'vanilla/26.3'
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
classes = cache / 'tools/datapack-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar'))]))
subprocess.run([str(jdk / 'javac'), '-cp', cp, '-d', str(classes), str(source / 'ConfigVectors.java'), str(source / 'MetadataVectors.java'), str(source / 'RepositoryVectors.java')], check=True)
subprocess.run([str(jdk / 'java'), '-Xmx1g', '-cp', f'{classes}:{cp}', 'ConfigVectors', str(source / 'config.json')], cwd=vanilla, check=True)
subprocess.run([str(jdk / 'java'), '-Xmx1g', '-cp', f'{classes}:{cp}', 'MetadataVectors', str(source / 'metadata.json')], cwd=vanilla, check=True)
subprocess.run([str(jdk / 'java'), '-Xmx1g', '-cp', f'{classes}:{cp}', 'RepositoryVectors', str(source / 'repository-input.json'), str(source / 'repository.json')], cwd=vanilla, check=True)
