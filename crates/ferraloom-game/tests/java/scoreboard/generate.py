#!/usr/bin/env python3
"""Run the real 26.3 scoreboard classes and read a real vanilla-written saved-data file."""
from pathlib import Path
import subprocess
import tempfile

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
java = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
with tempfile.TemporaryDirectory(prefix='scoreboard-vectors-') as temp:
    classes = Path(temp)
    jars = [vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar')), *sorted((vanilla / 'tree-harness').glob('*.jar'))]
    cp = ':'.join(map(str,jars))
    def run(args):
        result = subprocess.run(args, cwd=classes, text=True, capture_output=True)
        if result.returncode:
            print(result.stdout[-2000:]); print(result.stderr); raise SystemExit(result.returncode)
        return result.stdout
    run([str(java/'javac'),'-cp',cp,'-d',str(classes),str(source/'ScoreboardVectors.java')])
    output = run([str(java/'java'),'-Xmx512m','-XX:+EnableDynamicAgentLoading','-Dnet.bytebuddy.experimental=true','-cp',f'{classes}:{cp}','ScoreboardVectors',str(classes/'scoreboard.dat')])
    rows = [line for line in output.splitlines() if line.split('\t')[0] in {'membership','allied','collision','saved','codec','criterion'}]
    (source/'vectors.tsv').write_text('\n'.join(rows)+'\n')
    print(f'{len(rows)} vanilla scoreboard cases')
