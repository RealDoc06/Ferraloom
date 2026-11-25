#!/usr/bin/env python3
"""Ferraloom's player packet handlers on a dedicated server, against vanilla's, seen by vanilla clients.

The same two vanilla-codec players as tools/test_server_knockback_parity.py (KnockbackClient, Alice and Bob,
both operators, on a copy of one flat world vanilla generated) run each case on a vanilla dedicated server and on
Ferraloom's, and what each player was sent is compared, case by case:

- a spectator watching through an entity (ServerboundSpectatorActionPacket, `/spectate` for oneself and for
  another player): ClientboundSetCameraPacket, the teleport to the camera, and the view coming back on a sneak, a
  `/spectate` with no target, the camera's death and a game mode change (ServerPlayer.setCamera and tick);
- a riding jump (START_RIDING_JUMP) on a saddled horse and camel: the jump and dash sounds, the horse rearing
  (AbstractHorse.DATA_ID_FLAGS 32) and the camel's Camel.DASH;
- `/dialog show` of a registered and an inline dialog, and `/dialog clear`, for another player;
- block clicks out of reach, with a click location off the block, and at the build limits
  (handleUseItemOn, handleBlockBreakAction): what was placed or broken, the blocks sent back and the
  "Height limit for building" overlay;
- F3+I's tag queries of a block entity, a block with none, a mob and a player (only their top-level keys of
  KEYS are compared, so that Horizon's NBT coverage, which `/data` harnesses check, isn't this one's).

Each case has its setup (run under no case label, so not compared), its lines, and the kinds of lines it
compares; the two players' lines are each kept in order.
"""
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
    from .test_server_knockback_parity import A, V, run_side
except ImportError:
    import lab
    import test_rust_parity as base
    from test_locator_bar_parity import generate_world
    from test_server_knockback_parity import A, V, run_side

NAME = 'server-player-handlers-parity'
# The tag query keys compared: whether the metadata is left out, and that the data is there.
KEYS = {'id', 'x', 'y', 'z', 'Items', 'Pos', 'UUID', 'Health', 'Inventory'}
COW = '@e[type=minecraft:cow,limit=1]'
CLEAR = ['cmd Alice kill @e[type=!player]', 'cmd Alice gamemode survival Bob', 'cmd Alice gamemode creative Alice',
         f'cmd Alice tp Alice {A[0]} {A[1]} {A[2]} -90 0', f'cmd Alice tp Bob {V[0]} {V[1]} {V[2]} 90 0', 'ticks 10']
CAMERA = ('camera', 'teleport', 'chat')


def summon(kind, x, nbt='NoAI:1b'):
    return [f'cmd Alice summon minecraft:{kind} {x} -60 0.5 {{{nbt}}}', 'ticks 10']


def spectating(*lines):
    return [*CLEAR, 'cmd Alice gamemode spectator Bob', *summon('cow', 4.5)], [*lines, 'ticks 5'], CAMERA


def riding(kind):
    saddled = 'NoAI:1b,Tame:1b,equipment:{saddle:{id:"minecraft:saddle",count:1}}'
    return ([*CLEAR, *summon(kind, 6.5, saddled), f'cmd Alice ride Bob mount @e[type=minecraft:{kind},limit=1]',
             'ticks 5'], ['ridejump Bob 60', 'ticks 30'], ('sound', 'data'))


def holding(item):
    return [*CLEAR, f'cmd Alice item replace entity @s weapon.mainhand with minecraft:{item}', 'ticks 2']


def is_block(x, y, z, block):
    return f'cmd Alice execute if block {x} {y} {z} minecraft:{block}'


CLICKS = ('block', 'chat', 'overlay')
CASES = {
    'spectate-click': spectating('spectate Bob cow', 'ticks 5', 'sneak Bob on', 'ticks 3', 'sneak Bob off'),
    'spectate-click-far': spectating(f'cmd Alice tp {COW} 14.5 -60 0.5', 'ticks 5', 'spectate Bob cow'),
    'spectate-command': spectating(f'cmd Bob spectate {COW}', 'ticks 5', 'cmd Bob spectate'),
    'spectate-other-dies': spectating(f'cmd Alice spectate {COW} Bob', 'ticks 5', f'cmd Alice kill {COW}'),
    'spectate-mode-change': spectating(f'cmd Bob spectate {COW}', 'ticks 5', 'cmd Alice gamemode creative Bob'),
    'riding-jump-horse': riding('horse'),
    'riding-jump-camel': riding('camel'),
    'dialog': (CLEAR, ['cmd Alice dialog show Bob minecraft:server_links',
                       'cmd Alice dialog show Bob {type:"minecraft:notice",title:"Hi"}', 'cmd Alice dialog clear Bob',
                       'ticks 2'], ('dialog', 'chat')),
}
SETUP = ['cmd Alice gamerule spawn_mobs false', 'cmd Alice gamerule spawn_monsters false',
         'cmd Alice gamerule advance_time false', 'cmd Alice time set noon', 'ticks 20']


def compared(lines):
    """Per case, each player's lines of the kinds it compares, a tag query's keys cut down to KEYS."""
    cases = {}
    for line in lines:
        who, step, text = line.split(' ', 2)
        step = step.removeprefix('step=')
        if step not in CASES:
            continue
        kinds = CASES[step][2]
        kind = text.split(' ', 1)[0]
        if kind == 'tag':
            _, transaction, keys = text.split(' ', 2)
            keys = 'none' if keys == 'none' else ','.join(sorted(set(keys.split(',')) & KEYS))
            text = f'tag {transaction} {keys}'
        if kind in kinds or kind in ('error', 'disconnected'):
            cases.setdefault(step, []).append(f'{who} {text}')
    # Each player's own lines in order; the two players' interleaving is the network's.
    return {step: sorted(kept, key=lambda l: l.split(' ', 1)[0]) for step, kept in cases.items()}


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
    subprocess.run(['cargo', 'build', '--profile', 'fast', '-p', 'horizon-client', '--no-default-features', '--bin',
                    'horizon-client'],
                   cwd=lab.ROOT, check=True, capture_output=True)
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'],
                                                  cwd=lab.ROOT, text=True))
    binary = Path(metadata['target_directory']) / 'fast' / ('horizon-client.exe' if os.name == 'nt' else 'horizon-client')
    vanilla_server = ['java', '-Xmx2g', '-Duser.timezone=UTC', '-cp', dependencies, 'net.minecraft.server.Main', 'nogui']
    horizon_server = [binary, '--resources', lab.LOCAL / 'horizon-client/26.3', '--dedicated', '--nogui']
    client = ['java', '-Xmx1g', '-cp', str(classes) + os.pathsep + dependencies, 'dev.horizonlab.parity.KnockbackClient']
    # The setup runs under a label no case has, so only the case's own lines are compared.
    cases = {name: ['step setup', *setup, f'step {name}', *lines] for name, (setup, lines, _) in CASES.items()}

    world = generate_world(vanilla_server, build)
    vanilla = compared(run_side('vanilla', vanilla_server, client, build, world, cases, SETUP))
    horizon = compared(run_side('horizon', horizon_server, client, build, world, cases, SETUP))
    differ = []
    for case in CASES:
        want, got = vanilla.get(case, []), horizon.get(case, [])
        if any(l.split(' ', 2)[1] in ('error', 'disconnected') for l in want):
            raise AssertionError(f'{case}: vanilla failed: {want}')
        if want != got:
            import difflib
            differ.append(f'{case}:\n' + '\n'.join(difflib.unified_diff(want, got, 'vanilla', 'horizon', n=1,
                                                                        lineterm='')))
    if differ:
        raise AssertionError(f'{len(differ)} cases differ (see {build}/vanilla.txt and horizon.txt):\n'
                             + '\n'.join(differ)[:8000])
    lab.write_json(report, dict(
        schema=1, minecraft=lock['minecraft']['version'], platform=platform.platform(),
        java=subprocess.check_output(['java', '--version'], text=True).splitlines()[0],
        cases=len(CASES), output_sha256=lab.digest(build / 'horizon.txt', 'sha256'),
        scope='Player packet handlers on a dedicated server, as two vanilla-codec players get them: the spectator '
              'camera, riding jumps, /dialog for another player, block clicks out of reach or at the build limits, '
              'and tag queries.'))
    print(f'server player handlers: {len(CASES)} cases identical')


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        sys.exit(f'{NAME}: {error}')
