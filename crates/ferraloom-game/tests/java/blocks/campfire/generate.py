#!/usr/bin/env python3
"""Observe CampfireBlock.isSmokeyPos against the pinned native jar."""
from pathlib import Path
import subprocess
import sys

source=Path(__file__).resolve().parent
root=source.parents[5]
cache=Path('/home/doc/code/Ferraloom/.cache')
vanilla=cache/'vanilla/26.3'
java=cache/'toolchains/jdk-25.0.4.1+1/bin'
classes=cache/'tools/smokey-vectors'
classes.mkdir(parents=True,exist_ok=True)
cp=':'.join(map(str,[vanilla/'versions/26.3/server-26.3.jar',*sorted((vanilla/'libraries').rglob('*.jar')),*sorted((vanilla/'tree-harness').rglob('*.jar'))]))
subprocess.run([str(java/'javac'),'-cp',cp,'-d',str(classes),str(root/'crates/ferraloom-game/tests/java/advancements/AdvancementVectors.java'),str(source/'SmokeyVectors.java')],check=True)
output=classes/'smokey.tsv'
subprocess.run([str(java/'java'),'-Xmx768m','-XX:+EnableDynamicAgentLoading','-Dnet.bytebuddy.experimental=true','-cp',f'{classes}:{cp}','SmokeyVectors',str(output)],cwd=vanilla,check=True)
if sys.argv[1:]==['--check']:
    if output.read_bytes()!=(source/'smokey.tsv').read_bytes():raise SystemExit('Native smokey observations differ')
elif sys.argv[1:]:raise SystemExit('usage: generate.py [--check]')
else:(source/'smokey.tsv').write_bytes(output.read_bytes())
