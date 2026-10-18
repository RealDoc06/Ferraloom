#!/usr/bin/env python3
"""Regenerate fixtures by executing 26.3's real Entity.move on controlled Levels."""
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[5]
cache = Path('/home/doc/code/Ferraloom/.cache')
java = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classes = cache / 'tools/entity-vectors'
classes.mkdir(parents=True, exist_ok=True)
cp = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar', *sorted((vanilla / 'libraries').rglob('*.jar'))]))
source = Path(__file__).resolve().parent

def run(args, **kwargs):
    result = subprocess.run(args, cwd=classes, text=True, capture_output=True, **kwargs)
    if result.returncode:
        print(result.stdout[-1500:])
        print(result.stderr)
        raise SystemExit(result.returncode)
    return result.stdout

run([str(java / 'javac'), '-cp', cp, '-d', str(classes), str(source / 'EntityVectors.java'), str(source / 'TrackingVectors.java'), str(source / 'PersistenceVectors.java'), str(source / 'BorderVectors.java'), str(source / 'RidingVectors.java'), str(source / 'SynchedDataVectors.java'), str(source / 'AttributeVectors.java'), str(source / 'LivingVectors.java'), str(source / 'DimensionVectors.java'), str(source / 'PlayerTrackingVectors.java'), str(source.parent / 'level/NeighborVectors.java')])
run([str(java / 'java'), '-Xmx512m', '-cp', f'{classes}:{cp}', 'EntityVectors', 'generate', str(classes / 'EntityFixture.java')])
run([str(java / 'javac'), '-cp', f'{classes}:{cp}', '-d', str(classes), str(classes / 'EntityFixture.java')])
output = run([str(java / 'java'), '-Xmx768m', '-cp', f'{classes}:{cp}', 'EntityVectors'])
rows = [line for line in output.splitlines() if line.split('\t', 1)[0] in {'player-pairing', 'player-tracking', 'size-refresh', 'dimensions', 'attachment', 'living-data', 'living-health', 'attribute', 'attribute-map', 'attribute-nbt', 'synched-prototype', 'synched-float', 'synched-stack', 'riding', 'hurt-sync', 'border-lerp', 'border-geometry', 'bubble', 'entity-reference', 'tnt-launch', 'nbt-rotation', 'nbt-motion', 'server-entity', 'server-teleport', 'delta-codec', 'delta-steps', 'tracked-steps', 'orb-tick', 'orb-removed-follow', 'falling-tick', 'drop', 'pick', 'item-tick', 'tnt', 'inventory', 'trace', 'move', 'edge', 'flow', 'interaction', 'block-tag', 'fluid-tag'}]
(source / 'vectors.tsv').write_text('\n'.join(rows) + '\n')
print(f'{len(rows)} vanilla entity vectors')
