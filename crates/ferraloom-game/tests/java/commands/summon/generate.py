#!/usr/bin/env python3
"""Actual SummonCommand vectors and two independent JVM implicit-spawn UUID captures."""
from pathlib import Path
import json
import subprocess

cache = Path('/home/doc/code/Ferraloom/.cache')
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/summon-vectors'
classes.mkdir(parents=True, exist_ok=True)
source = Path(__file__).resolve().parent
cp = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar',
    *sorted((vanilla / 'libraries').rglob('*.jar')),
    *sorted((vanilla / 'tree-harness').rglob('*.jar'))]))
subprocess.run([str(jdk / 'javac'), '-cp', cp, '-d', str(classes), str(source / 'SummonVectors.java'), str(source.parents[1] / 'loot/LootVectors.java')], check=True)
captures = []
for index in range(2):
    output = classes / f'run-{index}.json'
    subprocess.run([str(jdk / 'java'), '-Xmx1g', '-Dnet.bytebuddy.experimental=true',
        '-XX:+EnableDynamicAgentLoading', '-cp', f'{classes}:{cp}', 'SummonVectors', str(output)],
        cwd=vanilla, check=True)
    captures.append(json.loads(output.read_text()))
assert captures[0]['vectors'] == captures[1]['vectors']
for first, second in zip(captures[0]['uuid_proof'], captures[1]['uuid_proof']):
    assert first['command'] == second['command'] and first['uuid'] != second['uuid']
(source / 'vectors.json').write_text(json.dumps(captures[0]['vectors'], indent=2, ensure_ascii=False) + '\n')
(source / 'uuid-proof.json').write_text(json.dumps([capture['uuid_proof'] for capture in captures], indent=2, ensure_ascii=False) + '\n')
print('Captured', len(captures[0]['vectors']), 'native summon cases and two-JVM UUID proof')
