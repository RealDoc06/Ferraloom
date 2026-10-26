#!/usr/bin/env python3
"""Check every block description against native bootstrap and capture falling names."""
from pathlib import Path
import json
import subprocess
import sys

source = Path(__file__).resolve().parent
root = source.parents[5]
cache = Path('/home/doc/code/Ferraloom/.cache')
java = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/entity-falling-name-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [classes, vanilla / 'versions/26.3/server-26.3.jar',
    *sorted((vanilla / 'libraries').rglob('*.jar')),
    *sorted((vanilla / 'tree-harness').glob('*.jar'))]))

def run(args):
    result = subprocess.run(args, cwd=classes, text=True, capture_output=True)
    if result.returncode:
        print(result.stdout[-2000:])
        print(result.stderr)
        raise SystemExit(result.returncode)
    return result.stdout

run([str(java / 'javac'), '-cp', cp, '-d', str(classes), str(source / 'FallingNameVectors.java')])
output = run([str(java / 'java'), '-Xmx512m', '-XX:+EnableDynamicAgentLoading',
    '-Dnet.bytebuddy.experimental=true', '-cp', cp, 'FallingNameVectors'])
actual = [line.split('\t')[1:] for line in output.splitlines() if line.startswith('block-description\t')]
facts = json.loads((root / 'data/generated/block_properties.json').read_text())['blocks']
expected = [[str(row['id']), row['name'], row['description_id']] for row in facts]
assert actual == expected, 'Native bootstrap block descriptions differ from extracted facts'
rows = [line for line in output.splitlines() if line.startswith('falling-name\t')]
assert len(rows) == 14, len(rows)
vectors = '\n'.join(rows) + '\n'
if sys.argv[1:] == ['--check']:
    if (source / 'vectors.tsv').read_text() != vectors:
        raise SystemExit('Native falling-block name observations differ; regenerate and inspect.')
elif sys.argv[1:]:
    raise SystemExit('usage: generate.py [--check]')
else:
    (source / 'vectors.tsv').write_text(vectors)
print(f'{len(actual)} exact native block descriptions and {len(rows)} falling name/hover observations')
