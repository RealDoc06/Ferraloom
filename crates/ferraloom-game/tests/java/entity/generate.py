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

run([str(java / 'javac'), '-cp', cp, '-d', str(classes), str(source / 'EntityVectors.java'), str(source / 'TrackingVectors.java'), str(source / 'PersistenceVectors.java'), str(source / 'SpawnVectors.java'), str(source / 'BorderVectors.java'), str(source / 'RidingVectors.java'), str(source / 'SynchedDataVectors.java'), str(source / 'AttributeVectors.java'), str(source / 'LivingVectors.java'), str(source / 'TravelVectors.java'), str(source / 'AbilityVectors.java'), str(source / 'AirTravelVectors.java'), str(source / 'FluidTravelVectors.java'), str(source / 'EffectVectors.java'), str(source / 'EffectReadVectors.java'), str(source / 'DamageVectors.java'), str(source / 'DifficultyVectors.java'), str(source / 'ExperienceVectors.java'), str(source / 'EntityFactsVectors.java'), str(source / 'NbtVectors.java'), str(source / 'NameVectors.java'), str(source / 'FreezingVectors.java'), str(source / 'ClimbVectors.java'), str(source / 'PushingVectors.java'), str(source / 'CrammingVectors.java'), str(source / 'DimensionVectors.java'), str(source / 'PlayerTrackingVectors.java'), str(source / 'WorldgenVectors.java'), str(source / 'CollisionVectors.java'), str(source / 'SoundVectors.java'), str(source / 'TeleportVectors.java'), str(source / 'PushVectors.java'), str(source / 'InteractionVectors.java'), str(source / 'ContactVectors.java'), str(source / 'ToolVectors.java'), str(source / 'UseItemVectors.java'), str(source.parent / 'level/NeighborVectors.java')])
run([str(java / 'java'), '-Xmx512m', '-cp', f'{classes}:{cp}', 'EntityVectors', 'generate', str(classes / 'EntityFixture.java')])
run([str(java / 'javac'), '-cp', f'{classes}:{cp}', '-d', str(classes), str(classes / 'EntityFixture.java')])
output = run([str(java / 'java'), '-Xmx768m', '-cp', f'{classes}:{cp}', 'EntityVectors'])
rows = [line for line in output.splitlines() if line.split('\t', 1)[0] in {'travel-fluid', 'fluid-jump-threshold', 'travel-air', 'player-flying-speed', 'player-ability-speeds', 'mob-effect-load', 'travel-climb', 'mob-effect-instance', 'mob-effect-update', 'mob-effect-chain', 'mob-effect-copy', 'mob-effect-scale', 'mob-effect-duration', 'mob-effect-negative', 'entity-equipment-fact', 'entity-equipment-order', 'entity-position-fact', 'entity-flags-fact', 'xp-curve', 'xp-award', 'xp-nbt', 'living-push-rate', 'living-cramming', 'difficulty-read', 'difficulty-initial', 'freeze-counter', 'freeze-armor', 'freeze-frost', 'entity-legacy', 'item-name', 'entity-name', 'entity-nbt', 'entity-visibility', 'damage-source', 'damage-type', 'damage-codec', 'entity-push', 'entity-impulse', 'push-team-capture', 'living-climb', 'living-climb-cache', 'travel-input', 'travel-friction', 'travel-fluid-drag', 'worldgen-shared', 'passenger-lists', 'entity-creation-order', 'player-spawn-rotation', 'stop-use', 'tool-drops', 'tool-tag', 'tool-missing-tag', 'player-instabuild', 'player-profile-uuid', 'player-contact', 'cooldown', 'cooldown-group', 'player-range', 'player-interaction', 'player-may-build', 'teleport-confirm-snap', 'teleport-pending-rotation', 'teleport-passengers', 'push-entities', 'teleport-transform', 'teleport-set', 'entity-tag', 'entity-sound', 'lava-base-tick', 'living-load-rotation', 'no-collision-order', 'worldgen-entities', 'living-movement', 'player-pairing', 'player-tracking', 'size-refresh', 'dimensions', 'attachment', 'living-data', 'living-health', 'attribute', 'attribute-map', 'attribute-nbt', 'synched-prototype', 'synched-float', 'synched-stack', 'riding', 'hurt-sync', 'border-lerp', 'border-geometry', 'bubble', 'entity-reference', 'tnt-launch', 'nbt-rotation', 'nbt-motion', 'server-entity', 'server-teleport', 'delta-codec', 'delta-steps', 'tracked-steps', 'orb-tick', 'orb-removed-follow', 'falling-tick', 'drop', 'pick', 'item-tick', 'tnt', 'inventory', 'trace', 'move', 'edge', 'flow', 'interaction', 'block-tag', 'fluid-tag'}]
(source / 'vectors.tsv').write_text('\n'.join(rows) + '\n')
print(f'{len(rows)} vanilla entity vectors')
