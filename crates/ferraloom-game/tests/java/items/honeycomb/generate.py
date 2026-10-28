#!/usr/bin/env python3
"""Extract the sole native wax table and compare real HoneycombItem callbacks."""
from pathlib import Path
import gzip
import os
import subprocess
import sys
import tempfile

here=Path(__file__).resolve().parent
root=here.parents[5]
cache=Path('/home/doc/code/Ferraloom/.cache')
jdk=Path(os.environ.get('JAVA_HOME',str(cache/'toolchains/jdk-25.0.4.1+1')))/'bin'
vanilla=cache/'vanilla/26.3'
classpath=':'.join(map(str,[vanilla/'versions/26.3/server-26.3.jar',*sorted((vanilla/'libraries').rglob('*.jar'))]))
with tempfile.TemporaryDirectory(prefix='honeycomb-native-') as directory:
    temp=Path(directory)
    def run(args):subprocess.run(args,cwd=temp,check=True)
    run([str(jdk/'javac'),'-cp',classpath,'-d',directory,str(here.parents[1]/'level/NeighborVectors.java')])
    run([str(jdk/'java'),'-cp',f'{directory}:{classpath}','NeighborVectors','generate',str(temp/'NeighborFixture.java')])
    fixture=temp/'NeighborFixture.java';fixture.write_text(fixture.read_text().replace('final class MutationFixture','class MutationFixture'))
    blocks=temp/'BlockItemVectors.java';blocks.write_text((here.parent/'BlockItemVectors.java').read_text().replace('static final class Fixture','static class Fixture'))
    run([str(jdk/'javac'),'-cp',f'{directory}:{classpath}','-d',directory,str(fixture),str(blocks),str(here/'HoneycombVectors.java')])
    run([str(jdk/'java'),'-Xmx512m','-cp',f'{directory}:{classpath}','HoneycombVectors',str(temp/'wax.json'),str(temp/'vectors.tsv')])
    outputs=[(root/'data/generated/copper_wax.json',(temp/'wax.json').read_bytes()),(here/'honeycomb.tsv.gz',gzip.compress((temp/'vectors.tsv').read_bytes(),mtime=0))]
    for path,actual in outputs:
        if '--capture' in sys.argv:path.write_bytes(actual)
        elif path.read_bytes()!=actual:raise SystemExit(f'native honeycomb comparison differs: {path}')
