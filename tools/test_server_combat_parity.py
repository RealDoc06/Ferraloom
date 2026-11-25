#!/usr/bin/env python3
"""Melee combat on a dedicated server, against vanilla's, seen by vanilla clients.

The setup of test_server_knockback_parity.py (a vanilla server and Horizon's on a copy of one flat world, Alice and
Bob made of vanilla's protocol code, waits on the server's game clock), with other cases:

- the attribute instances each player is sent (ClientboundUpdateAttributesPacket), for Alice herself and for Bob who
  tracks her: a sword, an axe and an empty hand selected in turn, armour worn then the hand changed, Sweeping Edge,
  Haste with a sword, and a sword that wears down on a hit. The attack indicator of a vanilla client and its attack
  cooldown come from the attack speed it is sent (`Player.getCurrentItemAttackStrengthDelay`), the server's
  `LivingEntity.collectEquipmentChanges` putting the held item's modifiers on it;
- the damage the attack strength scales (`Player.attack`): a zombie hit right after a swing at the air, then at full
  strength, then the sweep of a full-strength sword hit with and without Sweeping Edge, read back as Health;
- an axe on a raised shield: the cooldown the shield gets (ClientboundCooldownPacket).

Changes a case makes are a couple of ticks apart: vanilla puts equipment modifiers on in the player's tick
(`detectEquipmentUpdates`) and sends dirty instances in the level's, so two changes in one tick share a packet
differently than in Horizon, which puts them on at once; the client ends the same either way. It is midnight, so
that no zombie burns.

Each case is compared on its own. KNOWN lists the cases Horizon still gets wrong and why; one that matches again is
reported, and should leave the list.
"""
import difflib
import json
import os
from pathlib import Path
import platform
import re
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

NAME = 'server-combat-parity'
# Cases Horizon still gets wrong, and why.
KNOWN = {}

A = (0.5, -60.0, 0.5)  # Alice, facing east (yaw -90)
V = (2.5, -60.0, 0.5)  # the mob, or Bob behind his shield, two blocks east
B = (0.5, -60.0, 5.5)  # Bob watching, out of the sweep
MOB = '{NoAI:1b,Rotation:[90f,0f]}'


def reset():
    """No other entity, Alice with nothing on her, her first hotbar slot selected, Bob back to watching."""
    return ['cmd Alice kill @e[type=!player]', 'cmd Alice clear @s', 'ticks 2', 'cmd Alice effect clear @s',
            'slot Alice 0', 'cmd Bob clear @s',
            f'cmd Alice tp Bob {B[0]} {B[1]} {B[2]} 180 0', *stand('Alice', A, -90), *stand('Bob', B, 180), 'ticks 5']


def hold(item, slot=0):
    return f'cmd Alice item replace entity @s hotbar.{slot} with {item}'


def health(tag):
    return f'cmd Alice data get entity @e[tag={tag},limit=1] Health'


def zombie(tag, at):
    """A zombie standing still, tagged. The client's `attack Alice zombie` hits the last one summoned."""
    return f'cmd Alice summon minecraft:zombie {at[0]} {at[1]} {at[2]} {{NoAI:1b,Rotation:[90f,0f],Tags:["{tag}"]}}'


def sweep_case(sword):
    """A full-strength sword hit, grounded and not sprinting (Player.isSweepAttack), on a zombie with another a block
    beside it: the first takes the hit, the second the sweep."""
    return [*reset(), zombie('b', (V[0], V[1], V[2] + 1)), zombie('a', V), hold(sword), 'ticks 30',
            'attack Alice zombie', 'ticks 2', health('a'), health('b'), 'cmd Alice kill @e[type=minecraft:zombie]']


CASES = {
    'sword-hand-sword': [*reset(), hold('minecraft:diamond_sword'), 'ticks 5', 'slot Alice 1', 'ticks 5',
                         'slot Alice 0', 'ticks 5'],
    'sword-axe': [*reset(), hold('minecraft:diamond_sword'), hold('minecraft:netherite_axe', 1), 'ticks 5',
                  'slot Alice 1', 'ticks 5', 'slot Alice 0', 'ticks 5'],
    'armour-then-hand': [*reset(), 'cmd Alice item replace entity @s armor.chest with minecraft:diamond_chestplate',
                         'ticks 2', 'cmd Alice item replace entity @s armor.head with minecraft:netherite_helmet',
                         'ticks 5',
                         hold('minecraft:iron_sword'), 'ticks 5', 'slot Alice 1', 'ticks 5'],
    'sweeping-edge': [*reset(), hold('minecraft:diamond_sword[enchantments={"minecraft:sweeping_edge":3}]'),
                      'ticks 5', 'slot Alice 1', 'ticks 5'],
    'haste-sword': [*reset(), 'cmd Alice effect give @s minecraft:haste 1000 1 true', 'ticks 5',
                    hold('minecraft:golden_sword'), 'ticks 5'],
    # The hit wears the sword: a changed stack in the hand (ItemStack.matches), its modifiers put on again.
    'sword-wears': [*reset(), f'cmd Alice summon minecraft:cow {V[0]} {V[1]} {V[2]} {MOB}',
                    hold('minecraft:iron_sword'), 'ticks 30', 'attack Alice cow', 'ticks 5',
                    'cmd Alice kill @e[type=minecraft:cow]'],
    # A swing at the air resets the attack strength: the hit in the same tick does a fifth of the damage and a bit.
    'weak-then-full': [*reset(), zombie('a', V), hold('minecraft:diamond_axe'), 'ticks 30', 'punch Alice',
                       'attack Alice zombie', 'ticks 2', health('a'), 'ticks 30', 'attack Alice zombie', 'ticks 2',
                       health('a'), 'cmd Alice kill @e[type=minecraft:zombie]'],
    'sweep': sweep_case('minecraft:diamond_sword'),
    'sweep-edge-3': sweep_case('minecraft:diamond_sword[enchantments={"minecraft:sweeping_edge":3}]'),
    # Bob raises a shield; an axe at full strength disables it (BlocksAttacks.disable: 100 ticks).
    'axe-disables-shield': [*reset(), f'cmd Alice tp Bob {V[0]} {V[1]} {V[2]} 90 0', *stand('Bob', V, 90),
                            'cmd Bob item replace entity @s weapon.mainhand with minecraft:shield',
                            'cmd Bob effect give @s minecraft:resistance 1000 4 true', hold('minecraft:iron_axe'),
                            'use Bob', 'ticks 30', *stand('Bob', V, 90, ticks=1), 'attack Alice Bob', 'ticks 5'],
}
# The flat world's slimes split when killed: three passes take the big ones' smallest children too.
SETUP = ['cmd Alice gamerule spawn_mobs false', 'cmd Alice gamerule spawn_monsters false',
         *['cmd Alice kill @e[type=!player]'] * 3, 'cmd Alice gamerule advance_time false', 'cmd Alice time set midnight',
         f'cmd Alice tp Alice {A[0]} {A[1]} {A[2]} -90 0', f'cmd Alice tp Bob {B[0]} {B[1]} {B[2]} 180 0', 'ticks 40']

DATA = re.compile(r'^chat \S+ has the following entity data: (.*)$')


def compared(lines):
    """Per case, what is compared: the players' attribute packets each player got (their own, `self`, and the
    other's), the cooldown packets, and the Health read back. The join (step `start`) is compared too. Each
    receiver's stream about one subject keeps its order; how streams about different entities interleave is the
    server's to choose (vanilla's own varies with its tracker's timing), so they are compared apart."""
    cases = {}
    for line in lines:
        who, step, text = line.split(' ', 2)
        step = step.removeprefix('step=')
        kept = cases.setdefault(step, [])
        data = DATA.match(text)
        if who == 'Alice' and data:
            kept.append((who, '~health', 'health ' + data.group(1)))
        elif text.startswith('attributes '):
            subject = text.split(' ', 2)[1]
            if subject in ('self', 'Alice', 'Bob'):
                kept.append((who, subject, f'{who} {text}'))
        elif text.startswith(('cooldown ', 'error', 'disconnected')):
            kept.append((who, '~' + text.split(' ', 1)[0], f'{who} {text}'))
    def stream(kept):
        out = []
        for who, subject, line in sorted(kept, key=lambda k: k[:2]):
            # A tracker re-sent what it already holds changes nothing on its client (vanilla re-sends every dirty
            # instance, as when a hit wears the sword; Horizon's tracker sends changes): collapsed. The player's
            # own stream stays exact.
            if subject in ('Alice', 'Bob') and out and out[-1] == (who, subject, line):
                continue
            out.append((who, subject, line))
        return [line for *_, line in out]
    return {step: stream(kept) for step, kept in cases.items()}


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
    for case in ['start', *CASES]:
        want, got = vanilla.get(case, []), horizon.get(case, [])
        if any(' error' in f' {l}' or 'disconnected' in l for l in want):
            raise AssertionError(f'{case}: vanilla failed: {want}')
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
        cases=len(CASES) + 1, known=sorted(KNOWN), output_sha256=lab.digest(build / 'horizon.txt', 'sha256'),
        scope='Melee combat on a dedicated server, as two vanilla-codec players get it: the attribute instances '
              'synced to a player and to its tracker at join and as held items, armour, enchantments and effects '
              'change (the attack speed the attack indicator reads), damage scaled by the attack strength, sweep '
              'damage, and the cooldown an axe gives a raised shield.'))
    print(f'server combat: {len(CASES) + 1 - len(KNOWN)} of {len(CASES) + 1} cases identical')


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        sys.exit(f'{NAME}: {error}')
