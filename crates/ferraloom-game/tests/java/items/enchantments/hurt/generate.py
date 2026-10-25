#!/usr/bin/env python3
"""Capture actual ItemStack overloads and Unbreaking from vanilla 26.3."""
from pathlib import Path
import gzip
import subprocess
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[6]
CACHE = Path('/home/doc/code/Ferraloom/.cache')
JDK = CACHE / 'toolchains/jdk-25.0.4.1+1/bin'
VANILLA = CACHE / 'vanilla/26.3'
CLASSES = CACHE / 'tools/item-hurt-and-break-vectors'
CLASSES.mkdir(parents=True, exist_ok=True)
CP = ':'.join(map(str,[VANILLA / 'versions/26.3/server-26.3.jar',
                     *sorted((VANILLA / 'libraries').rglob('*.jar')),
                     *sorted((VANILLA / 'tree-harness').glob('*.jar'))]))
ENTITY = ROOT / 'crates/ferraloom-game/tests/java/entity'

def run(args):
    result = subprocess.run(args, cwd=CLASSES, text=True, capture_output=True)
    if result.returncode:
        print(result.stdout[-4000:])
        print(result.stderr[-8000:])
        raise SystemExit(result.returncode)
    return result.stdout

run([str(JDK/'javac'),'-cp',CP,'-d',str(CLASSES),*map(str,sorted(ENTITY.glob('*.java'))),str(ENTITY.parent/'level/NeighborVectors.java')])
run([str(JDK/'java'),'-cp',str(CLASSES)+':'+CP,'EntityVectors','generate',str(CLASSES/'EntityFixture.java')])
run([str(JDK/'javac'),'-cp',str(CLASSES)+':'+CP,'-d',str(CLASSES),str(CLASSES/'EntityFixture.java'),str(HERE/'HurtAndBreakVectors.java')])
output=run([str(JDK/'java'),'-Xmx768m','-XX:+EnableDynamicAgentLoading','-Dnet.bytebuddy.experimental=true','-cp',str(CLASSES)+':'+CP,'HurtAndBreakVectors'])
rows=[row for row in output.splitlines() if row.startswith(('hurt-break\t','break-cleanup\t','glider-wear\t','equipment-wear\t'))]
vectors=('\n'.join(rows)+'\n').encode()
path=HERE/'vectors.tsv.gz'
if sys.argv[1:]==['--check']:
    if gzip.decompress(path.read_bytes())!=vectors: raise SystemExit('native hurtAndBreak observations differ')
elif sys.argv[1:]: raise SystemExit('usage: generate.py [--check]')
else: path.write_bytes(gzip.compress(vectors,mtime=0))
print(f'{len(rows)} native durability and cleanup observations')
