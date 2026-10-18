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
    run([str(java/'javac'),'-cp',cp,'-d',str(classes),str(source/'ScoreboardVectors.java'),str(source/'CommandVectors.java'),str(source/'TeamFeedbackVectors.java'),str(source/'TeamSingleFeedbackVectors.java'),str(source/'DynamicCommandVectors.java')])
    output = run([str(java/'java'),'-Xmx512m','-XX:+EnableDynamicAgentLoading','-Dnet.bytebuddy.experimental=true','-cp',f'{classes}:{cp}','ScoreboardVectors',str(classes/'scoreboard.dat')])
    rows = [line for line in output.splitlines() if line.split('\t')[0] in {'membership','allied','collision','saved','codec','criterion','criterion-alias','ordering'}]
    (source/'vectors.tsv').write_text('\n'.join(rows)+'\n')
    print(f'{len(rows)} vanilla scoreboard cases')

    run([str(java/'java'),'-Xmx768m','-XX:+EnableDynamicAgentLoading','-Dnet.bytebuddy.experimental=true','-cp',f'{classes}:{cp}','CommandVectors',str(source/'commands.json'),str(source/'join.json'),str(source/'formatting.json'),str(source/'producers.json'),str(source/'readonly.json')])
    import json
    print(f"{len(json.loads((source/'commands.json').read_text()))} vanilla command cases")

    run([str(java/'java'),'-Xmx512m','-XX:+EnableDynamicAgentLoading','-Dnet.bytebuddy.experimental=true','-cp',f'{classes}:{cp}','TeamFeedbackVectors',str(source/'team-feedback.json')])
    print(f"{len(json.loads((source/'team-feedback.json').read_text()))} vanilla team feedback cases")

    run([str(java/'java'),'-Xmx512m','-XX:+EnableDynamicAgentLoading','-Dnet.bytebuddy.experimental=true','-cp',f'{classes}:{cp}','TeamSingleFeedbackVectors',str(source/'team-single-feedback.json')])
    print(f"{len(json.loads((source/'team-single-feedback.json').read_text()))} vanilla single-entity team feedback cases")

    run([str(java/'java'),'-Xmx512m','-XX:+EnableDynamicAgentLoading','-Dnet.bytebuddy.experimental=true','-cp',f'{classes}:{cp}','DynamicCommandVectors',str(source/'dynamic-commands.json')])
    print(f"{len(json.loads((source/'dynamic-commands.json').read_text()))} vanilla dynamic-command getter-order cases")
