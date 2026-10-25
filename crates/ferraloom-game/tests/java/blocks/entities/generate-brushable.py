#!/usr/bin/env python3
"""Capture native brushable persistence and real WorldGenRegion seed behavior."""
from pathlib import Path
import subprocess
source=Path(__file__).resolve().parent
cache=Path('/home/doc/code/Ferraloom/.cache')
java=cache/'toolchains/jdk-25.0.4.1+1/bin'
vanilla=cache/'vanilla/26.3'
classes=cache/'tools/brushable-block-entity-vectors'
classes.mkdir(parents=True,exist_ok=True)
cp=':'.join(map(str,[classes,vanilla/'versions/26.3/server-26.3.jar',*sorted((vanilla/'libraries').rglob('*.jar')),*sorted((vanilla/'tree-harness').rglob('*.jar'))]))
for command in [[str(java/'javac'),'-cp',cp,'-d',str(classes),str(source.parent.parent/'level/NeighborVectors.java'),str(source/'WorldgenBlockEntityVectors.java'),str(source/'BrushableVectors.java')],[str(java/'java'),'-Xmx768m','-Dnet.bytebuddy.experimental=true','-XX:+EnableDynamicAgentLoading','-cp',cp,'BrushableVectors']]:
    result=subprocess.run(command,cwd=vanilla,text=True,capture_output=True)
    if result.returncode:
        print(result.stdout[-2000:]);print(result.stderr);raise SystemExit(result.returncode)
rows=[line for line in result.stdout.splitlines() if line.startswith(('brushable\t','brushable-worldgen\t'))]
assert len(rows)==66,len(rows)
(source/'brushable.tsv').write_text('\n'.join(rows)+'\n')
print(f'{len(rows)} native brushable observations')
