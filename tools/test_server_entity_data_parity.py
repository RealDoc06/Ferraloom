#!/usr/bin/env python3
"""Entity data and world changes the server owns, against vanilla's dedicated server, seen by vanilla clients.

The setup of test_server_combat_parity.py (a vanilla server and Horizon's on a copy of one flat world, Alice and Bob
made of vanilla's protocol code, waits on the server's game clock), with these cases:

- an arrow that hurts a zombie, and one that hurts Bob: the arrow count (LivingEntity.DATA_ARROW_COUNT_ID, 12) each
  player is sent for them (`AbstractArrow.onHitEntity` → `setArrowCount`);
- a bee the zombie hurt stings it: the stinger count (DATA_STINGER_COUNT_ID, 13, `Bee.doHurtTarget`);
- a zombie with NoAI at noon: it burns (`Mob.aiStep` → `burnUndead` runs without AI), seen as the on-fire bit;
- `/fillbiome` around the players: the ClientboundChunksBiomesPacket each is sent (`ChunkMap.resendBiomesForChunks`),
  read section by section as the client reads it;
- sand that lands: the ClientboundAddTransientBlockPacket its trackers get (`FallingBlockEntity.tick`);
- `/tp` of an item at rest (its trackers) and of a boat Bob rides (Bob, its controller): the
  ClientboundTeleportEntityPacket each player is sent (`Entity.sendTeleportTransitionToPlayers`). An item's yaw is
  random on vanilla's side and left out.

Each case is compared on its own. KNOWN lists the cases Horizon still gets wrong and why; one that matches again is
reported, and should leave the list.
"""
import difflib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

try:
    from . import lab, test_rust_parity as base
    from .test_locator_bar_parity import generate_world
    from .test_server_knockback_parity import run_side, stand
except ImportError:
    import lab
    import test_rust_parity as base
    from test_locator_bar_parity import generate_world
    from test_server_knockback_parity import run_side, stand

NAME = 'server-entity-data-parity'
# Cases Horizon still gets wrong, and why.
KNOWN = {
    'arrow-player': "a player's arrow count is kept (PlayerMotion.stuck) but not yet sent: the player's entity data is "
                    'lan/hub.rs player_state',
}

A = (0.5, -60.0, 0.5)  # Alice, facing east (yaw -90)
V = (2.5, -60.0, 0.5)  # the mob, two blocks east
B = (0.5, -60.0, 5.5)  # Bob
ZOMBIE = '{NoAI:1b,Rotation:[90f,0f]}'


def reset():
    """No other entity, Alice and Bob back in place with nothing on them."""
    return ['cmd Alice kill @e[type=!player]', 'cmd Alice clear @s', 'cmd Bob clear @s', 'ticks 2',
            f'cmd Alice tp Bob {B[0]} {B[1]} {B[2]} 180 0', *stand('Alice', A, -90), *stand('Bob', B, 180), 'ticks 5']


def arrow_at(at):
    """An arrow a block west of `at`, flying east into what stands there."""
    return f'cmd Alice summon minecraft:arrow {at[0] - 1.0} {at[1] + 1.0} {at[2]} {{Motion:[1.5d,0d,0d]}}'


CASES = {
    'arrow-mob': [*reset(), f'cmd Alice summon minecraft:zombie {V[0]} {V[1]} {V[2]} {ZOMBIE}', 'ticks 5',
                  arrow_at(V), 'ticks 10'],
    'arrow-player': [*reset(), arrow_at(B), 'ticks 10'],
    'sting-mob': [*reset(), f'cmd Alice summon minecraft:zombie {V[0]} {V[1]} {V[2]} {ZOMBIE}',
                  f'cmd Alice summon minecraft:bee {V[0]} {V[1] + 1} {V[2] + 3}', 'ticks 2',
                  # Hurt by the zombie, the bee turns on it (BeeHurtByOtherGoal).
                  'cmd Alice damage @e[type=minecraft:bee,limit=1] 1 minecraft:mob_attack by '
                  '@e[type=minecraft:zombie,limit=1]', 'ticks 200'],
    'noai-sunburn': [*reset(), 'cmd Alice weather clear', 'cmd Alice time set noon',
                     f'cmd Alice summon minecraft:zombie {V[0]} {V[1]} {V[2]} {ZOMBIE}', 'ticks 200',
                     'cmd Alice time set midnight'],
    'fillbiome': [*reset(), 'cmd Alice fillbiome -16 -64 -16 15 -49 15 minecraft:desert', 'ticks 5'],
    'falling-sand': [*reset(), f'cmd Alice summon minecraft:falling_block {V[0]} {V[1] + 5} {V[2] + 2} '
                     '{BlockState:{Name:"minecraft:sand"}}', 'ticks 40'],
    # An item at rest on the ground, moved along it.
    'tp-item': [*reset(), f'cmd Alice summon minecraft:item {V[0]} {V[1]} {V[2] + 2} '
                '{Item:{id:"minecraft:stone",count:1},PickupDelay:32767s}', 'ticks 40',
                f'cmd Alice tp @e[type=minecraft:item,limit=1] {V[0] + 2} {V[1]} {V[2] + 2}', 'ticks 5'],
    'tp-boat-rider': [*reset(), f'cmd Alice summon minecraft:oak_boat {V[0]} {V[1]} {V[2] + 3}', 'ticks 5',
                      'interact Bob oak_boat', 'ticks 5',
                      f'cmd Alice tp @e[type=minecraft:oak_boat,limit=1] {V[0] + 2} {V[1]} {V[2] + 3}', 'ticks 5',
                      'cmd Alice ride Bob dismount'],
}
# The flat world's slimes split when killed: three passes take the big ones' smallest children too.
SETUP = ['cmd Alice gamerule spawn_mobs false', 'cmd Alice gamerule spawn_monsters false',
         *['cmd Alice kill @e[type=!player]'] * 3, 'cmd Alice gamerule advance_time false', 'cmd Alice time set midnight',
         'cmd Alice gamerule advance_weather false', 'cmd Alice weather clear',
         f'cmd Alice tp Alice {A[0]} {A[1]} {A[2]} -90 0', f'cmd Alice tp Bob {B[0]} {B[1]} {B[2]} 180 0', 'ticks 40']

LIVING = ('self', 'Alice', 'Bob', 'zombie')


def compared(lines):
    """Per case, what each player was sent: the arrow and stinger counts of the players and the zombie, the zombie's
    on-fire bit, the biomes, the transient blocks and the entity teleports. Each receiver's stream about one subject keeps its order; how
    streams about different subjects interleave is the server's to choose, so they are compared apart."""
    cases = {}
    for line in lines:
        who, step, text = line.split(' ', 2)
        step = step.removeprefix('step=')
        kept = cases.setdefault(step, [])
        word, rest = (text.split(' ', 1) + [''])[:2]
        subject = rest.split(' ', 1)[0]
        # A player's arrow falls out 580 ticks on, in whichever case the servers' timings put it: players' counts
        # are compared in the case that sticks them only.
        if word in ('arrows', 'stingers') and subject in ('self', 'Alice', 'Bob') and step != 'arrow-player':
            continue
        if word in ('arrows', 'stingers') and subject in LIVING or word == 'fire' and subject == 'zombie':
            kept.append((who, subject, f'{who} {text}'))
        elif word == 'tpentity':
            fields = text.split(' ')
            if subject == 'item':
                fields[8:10] = ['~', '~']
            kept.append((who, subject, f'{who} {" ".join(fields)}'))
        elif word in ('biomes', 'transient', 'error', 'disconnected'):
            kept.append((who, '~' + word, f'{who} {text}'))
    return {step: [line for *_, line in sorted(kept, key=lambda k: k[:2])] for step, kept in cases.items()}


def main():
    lock, jars, libraries = base.verify_artifacts()
    build = lab.LOCAL / NAME
    build.mkdir(parents=True, exist_ok=True)
    report = build / 'report.json'
    report.unlink(missing_ok=True)
    oracle = lab.ROOT / 'mods/horizon/src/test/java/dev/horizonlab/parity/KnockbackClient.java'
    classes = build / 'classes'
    shutil.rmtree(classes, ignore_errors=True)
    classes.mkdir()
    dependencies = os.pathsep.join(map(str, [jars['server'], *libraries]))
    subprocess.run(['javac', '--release', str(lock['minecraft']['java_major']), '-proc:none', '-nowarn', '-sourcepath',
                    lab.ROOT / 'mods/horizon/src/test/java', '-cp', dependencies, '-d', classes, oracle],
                   cwd=lab.ROOT, check=True, capture_output=True)
    subprocess.run(['cargo', 'build', '--profile', 'fast', '-p', 'horizon-client', '--no-default-features',
                    '--bin', 'horizon-client'],
                   cwd=lab.ROOT, check=True, capture_output=True)
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'],
                                                  cwd=lab.ROOT, text=True))
    binary = Path(metadata['target_directory']) / 'fast' / ('horizon-client.exe' if os.name == 'nt' else 'horizon-client')
    vanilla_server = ['java', '-Xmx2g', '-Duser.timezone=UTC', '-cp', dependencies, 'net.minecraft.server.Main', 'nogui']
    horizon_server = [binary, '--resources', lab.LOCAL / 'horizon-client/26.3', '--dedicated', '--nogui']
    client = ['java', '-Xmx1g', '-cp', str(classes) + os.pathsep + dependencies, 'dev.horizonlab.parity.KnockbackClient']

    world = generate_world(vanilla_server, build)
    vanilla = compared(run_side('vanilla', vanilla_server, client, build, world, CASES, SETUP))
    horizon = compared(run_side('horizon', horizon_server, client, build, world, CASES, SETUP))
    differ, fixed = [], []
    for case in CASES:
        want, got = vanilla.get(case, []), horizon.get(case, [])
        if any(' error' in f' {l}' or 'disconnected' in l for l in want):
            raise AssertionError(f'{case}: vanilla failed: {want}')
        if not want:
            raise AssertionError(f'{case}: vanilla sent nothing the case looks for (see {build}/vanilla.txt)')
        if want == got:
            if case in KNOWN:
                fixed.append(case)
            continue
        if case in KNOWN:
            continue
        differ.append(f'{case}:\n' + '\n'.join(difflib.unified_diff(want, got, 'vanilla', 'horizon', n=1, lineterm='')))
    if fixed:
        print('matching vanilla now, take them out of KNOWN: ' + ', '.join(fixed))
    if differ:
        raise AssertionError(f'{len(differ)} cases differ (see {build}/vanilla.txt and horizon.txt):\n'
                             + '\n'.join(differ)[:12000])
    lab.write_json(report, dict(
        schema=1, minecraft=lock['minecraft']['version'], platform=platform.platform(),
        java=subprocess.check_output(['java', '--version'], text=True).splitlines()[0],
        cases=len(CASES), known=sorted(KNOWN), output_sha256=lab.digest(build / 'horizon.txt', 'sha256'),
        scope='Entity data and world changes a dedicated server owns, as two vanilla-codec players get them: arrow '
              'and stinger counts on a mob and a player, the sun burning a NoAI zombie, /fillbiome resending the '
              "chunks' biomes, a landing falling block's transient block and /tp of an item and a ridden boat."))
    print(f'server entity data: {len(CASES) - len(KNOWN)} of {len(CASES)} cases identical')


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        sys.exit(f'{NAME}: {error}')
