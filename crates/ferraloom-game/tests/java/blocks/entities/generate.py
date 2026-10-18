#!/usr/bin/env python3
"""Run the pinned vanilla block-entity oracle on devbox (light Java work)."""
from pathlib import Path
import subprocess
source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
jdk = cache/'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache/'vanilla/26.3'
classes = cache/'tools/block-entity-vectors'
classes.mkdir(parents=True,exist_ok=True)
cp = ':'.join(map(str,[vanilla/'versions/26.3/server-26.3.jar',*sorted((vanilla/'libraries').rglob('*.jar'))]))
def run(args):
    result=subprocess.run(args,cwd=classes,text=True,capture_output=True)
    if result.returncode:
        print(result.stdout[-2000:]);print(result.stderr);raise SystemExit(result.returncode)
    return result.stdout
run([str(jdk/'javac'),'-cp',cp,'-d',str(classes),str(source.parent.parent/'level/NeighborVectors.java')])
run([str(jdk/'java'),'-cp',f'{classes}:{cp}','NeighborVectors','generate',str(classes/'NeighborFixture.java')])
fixture=classes/'NeighborFixture.java'
fixture.write_text(fixture.read_text().replace('final class MutationFixture','class MutationFixture'))
run([str(jdk/'javac'),'-cp',f'{classes}:{cp}','-d',str(classes),str(fixture),str(source/'BlockEntityFixture.java'),str(source/'BlockEntityVectors.java')])
output=run([str(jdk/'java'),'-Xmx512m','-cp',f'{classes}:{cp}','BlockEntityVectors'])
rows=[line for line in output.splitlines() if line.startswith(('registry\t','position\t','base-','default-save\t','filled-save\t','roundtrip\t','update-tag\t','update-packet\t','loot-save\t','loot-zero\t','ticker-phase\t','scatter\t','sign-edit\t','sign-edit-updates\t','sign-editor-tick\t','menu-broadcast-reads\t','sided-slots\t','sided-rule\t','cook-time\t','sign-facing\t','pop-resource-rng\t'))]
(source/'vectors.tsv').write_text('\n'.join(rows)+'\n')
print(f'{len(rows)} vanilla block-entity vectors')
