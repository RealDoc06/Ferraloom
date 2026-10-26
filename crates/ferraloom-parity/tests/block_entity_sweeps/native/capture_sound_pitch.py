#!/usr/bin/env python3
"""Prove native storage pitch differs in two independent JVMs on identical input."""
from pathlib import Path
import json
import subprocess

source = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/be-storage-sound-pitch'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [classes, vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar')), *sorted((vanilla / 'tree-harness').rglob('*.jar'))]))
compile = subprocess.run([str(jdk/'javac'), '-cp', cp, '-d', str(classes), str(source/'StorageSoundPitch.java')], text=True, capture_output=True)
if compile.returncode:
    raise SystemExit(compile.stderr)
runs = []
for _ in range(2):
    run = subprocess.run([str(jdk/'java'), '-Xmx384m', '-XX:+EnableDynamicAgentLoading', '-Dnet.bytebuddy.experimental=true', '-cp', cp, 'StorageSoundPitch'], cwd=classes, text=True, capture_output=True)
    if run.returncode:
        raise SystemExit(run.stdout + run.stderr)
    row = next(line for line in run.stdout.splitlines() if line.startswith('native-storage-sounds\t'))
    runs.append(json.loads(row.split('\t', 1)[1]))
assert all(len(run) == 4 for run in runs)
assert all(0.9 <= sound['pitch'] <= 1.0 for run in runs for sound in run)
assert runs[0] != runs[1], 'independent native pitches unexpectedly identical'
without_pitch = lambda run: [{key: value for key, value in sound.items() if key != 'pitch'} for sound in run]
assert without_pitch(runs[0]) == without_pitch(runs[1]), 'another sound field/order/count differed'
(source/'sound-pitch-two-jvms.json').write_text(json.dumps(runs, indent=2) + '\n')
print('Two native JVMs differ in pitch; key/position/source/volume/order/count agree exactly')
