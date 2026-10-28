#!/usr/bin/env python3
"""Capture native Move/Look/Jump/BodyRotation control transitions and float bits."""
from pathlib import Path
import subprocess
import sys
source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
vanilla = cache/'vanilla/26.3'
java = cache/'toolchains/jdk-25.0.4.1+1/bin'
classes = cache/'tools/ai-control-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [vanilla/'versions/26.3/server-26.3.jar',*sorted((vanilla/'libraries').rglob('*.jar')),*sorted((vanilla/'tree-harness').rglob('*.jar'))]))
subprocess.run([str(java/'javac'),'-cp',cp,'-d',str(classes),str(source/'ControlVectors.java')],check=True)
output=classes/'controls.tsv'
subprocess.run([str(java/'java'),'-Xmx256m','-XX:+EnableDynamicAgentLoading','-Dnet.bytebuddy.experimental=true','-cp',f'{classes}:{cp}','ControlVectors',str(output)],cwd=vanilla,check=True)
fixture=source/'vectors.tsv'
if sys.argv[1:]==['--check']:
    if output.read_bytes()!=fixture.read_bytes():raise SystemExit('Native control observations differ')
elif sys.argv[1:]:raise SystemExit('usage: generate.py [--check]')
else:fixture.write_bytes(output.read_bytes())
