#!/usr/bin/env python3
"""Native JSON/NBT codec observations; compiled Java stays in the shared cache."""
from pathlib import Path
import subprocess

cache=Path('/home/doc/code/Ferraloom/.cache')
jdk=cache/'toolchains/jdk-25.0.4.1+1/bin'
vanilla=cache/'vanilla/26.3'
classes=cache/'tools/loot-input-vectors'
classes.mkdir(parents=True,exist_ok=True)
source=Path(__file__).resolve().parent
cp=':'.join(map(str,[vanilla/'versions/26.3/server-26.3.jar',*sorted((vanilla/'libraries').rglob('*.jar')),*sorted((vanilla/'tree-harness').rglob('*.jar'))]))
subprocess.run([str(jdk/'javac'),'-cp',cp,'-d',str(classes),str(source/'LootInputVectors.java'),str(source.parent/'LootVectors.java')],check=True)
subprocess.run([str(jdk/'java'),'-Xmx1g','-Dnet.bytebuddy.experimental=true','-cp',f'{classes}:{cp}','LootInputVectors',str(source/'vectors.json')],cwd=vanilla,check=True)
