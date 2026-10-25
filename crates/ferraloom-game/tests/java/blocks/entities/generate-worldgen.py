#!/usr/bin/env python3
"""Capture actual WorldGenRegion/ProtoChunk receivers and promotion on JDK 25."""
from pathlib import Path
import subprocess
source=Path(__file__).resolve().parent
cache=Path('/home/doc/code/Ferraloom/.cache')
java=cache/'toolchains/jdk-25.0.4.1+1/bin'
vanilla=cache/'vanilla/26.3'
classes=cache/'tools/worldgen-block-entity-vectors'
classes.mkdir(parents=True,exist_ok=True)
cp=':'.join(map(str,[classes,vanilla/'versions/26.3/server-26.3.jar',*sorted((vanilla/'libraries').rglob('*.jar')),*sorted((vanilla/'tree-harness').rglob('*.jar'))]))
for command in [[str(java/'javac'),'-cp',cp,'-d',str(classes),str(source.parent.parent/'level/NeighborVectors.java'),str(source/'WorldgenBlockEntityVectors.java')],[str(java/'java'),'-Xmx768m','-Dnet.bytebuddy.experimental=true','-XX:+EnableDynamicAgentLoading','-cp',cp,'WorldgenBlockEntityVectors']]:
    result=subprocess.run(command,cwd=vanilla,text=True,capture_output=True)
    if result.returncode:
        print(result.stdout[-4000:]);print(result.stderr);raise SystemExit(result.returncode)
rows=[line for line in result.stdout.splitlines() if line.startswith(('worldgen\t','promotion\t'))]
assert len(rows)==184,len(rows)
(source/'worldgen.tsv').write_text('\n'.join(rows)+'\n')
print(f'{len(rows)} vanilla WorldGenRegion block-entity observations')
