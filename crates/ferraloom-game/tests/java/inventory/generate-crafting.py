#!/usr/bin/env python3
"""Capture server-side crafting menus against the real 26.3 recipe registry."""
from pathlib import Path
import subprocess
import runpy
source=Path(__file__).resolve().parent
runpy.run_path(str(source/'generate.py'))
cache=Path('/home/doc/code/Ferraloom/.cache')
vanilla=cache/'vanilla/26.3'
java=cache/'toolchains/jdk-25.0.4.1+1/bin'
classes=cache/'tools/inventory-vectors'
cp=':'.join(map(str,[classes,vanilla/'versions/26.3/server-26.3.jar',*sorted((vanilla/'libraries').rglob('*.jar')),*sorted((vanilla/'tree-harness').rglob('*.jar'))]))
for args in [[str(java/'javac'),'-cp',cp,'-d',str(classes),str(source/'CraftingMenuVectors.java'),str(source/'ClearGridVectors.java'),str(source/'ProducerVectors.java')],[str(java/'java'),'-Xmx768m','-Dnet.bytebuddy.experimental=true','-XX:+EnableDynamicAgentLoading','-cp',cp,'CraftingMenuVectors']]:
    result=subprocess.run(args,cwd=vanilla,text=True,capture_output=True)
    if result.returncode:
        print(result.stdout[-3000:]);print(result.stderr);raise SystemExit(result.returncode)
rows=[line for line in result.stdout.splitlines() if line.split('\t',1)[0] in {'start','click','place'}]
(source/'crafting.tsv').write_text('\n'.join(rows)+'\n')
print(f'{len(rows)} vanilla server crafting menu vectors')

clear_rows=[line for line in result.stdout.splitlines() if line.startswith('clear-grid\t')]
assert len(clear_rows)==5
(source/'clear-grid.tsv').write_text('\n'.join(clear_rows)+'\n')
print(f'{len(clear_rows)} native full-inventory clearGrid timelines')

producer_rows=[line for line in result.stdout.splitlines() if line.startswith('producer\t')]
assert len(producer_rows)==5
(source/'producers.tsv').write_text('\n'.join(producer_rows)+'\n')
print(f'{len(producer_rows)} native advancement producer timelines')
