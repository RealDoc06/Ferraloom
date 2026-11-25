#!/usr/bin/env python3
"""Item components on a dedicated server, against vanilla's, seen by a vanilla client.

The setup of test_server_knockback_parity.py (a vanilla server and Horizon's on a copy of one flat world, players
made of vanilla's protocol code). Each case puts an item in Alice's first hotbar slot with `/item replace`; what is
compared is the ClientboundContainerSetSlotPacket she receives, decoded by vanilla's codecs: the item, its count and
its component patch. Vanilla sends every networked component, so lore, custom data, a coloured name and the rest that
commands, loot tables, data packs and plugins put on items must reach the client as they are (lan/item_extras.rs).

KNOWN lists the cases Horizon still gets wrong and why; one that matches again is reported, and should leave the list.
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
    from .test_server_knockback_parity import run_side
except ImportError:
    import lab
    import test_rust_parity as base
    from test_locator_bar_parity import generate_world
    from test_server_knockback_parity import run_side

NAME = 'server-items-parity'
# Cases Horizon still gets wrong, and why.
KNOWN = {}


def give(item):
    return ['cmd Alice clear @s', 'ticks 2', f'cmd Alice item replace entity @s hotbar.0 with {item}', 'ticks 5']


CASES = {
    'plain-name': give('minecraft:stick[custom_name="Plain"]'),
    'coloured-name': give('minecraft:chest[custom_name={text:"Shop",color:"gold",italic:false}]'),
    'name-with-extra': give('minecraft:chest[custom_name={text:"",extra:[{text:"Sh",color:"red"},{text:"op",bold:true}]}]'),
    'lore': give('minecraft:diamond_sword[lore=["Sharp",{text:"Gold",color:"gold",italic:false}]]'),
    'custom-data': give('minecraft:emerald[custom_data={shop:{price:250L,id:"ruby"}}]'),
    'custom-model-data': give('minecraft:paper[custom_model_data={floats:[7.0f],strings:["gui"]}]'),
    'unbreakable': give('minecraft:iron_pickaxe[unbreakable={}]'),
    'hidden-tooltip-parts': give('minecraft:diamond_sword[enchantments={"minecraft:sharpness":3},'
                                 'tooltip_display={hidden_components:["minecraft:enchantments",'
                                 '"minecraft:attribute_modifiers"]}]'),
    'hide-tooltip': give('minecraft:stone[tooltip_display={hide_tooltip:true}]'),
    'rarity-glint-stack': give('minecraft:stick[rarity="epic",enchantment_glint_override=true,max_stack_size=16]'),
    'item-model': give('minecraft:stick[item_model="minecraft:diamond"]'),
    'player-head': give('minecraft:player_head[profile={name:"Steve"}]'),
    # A plugin's GUI item: everything at once.
    'gui-item': give('minecraft:lime_stained_glass_pane[custom_name={text:"Buy",color:"green",italic:false},'
                     'lore=[{text:"Price: 5",color:"gray",italic:false}],custom_data={PublicBukkitValues:'
                     '{"shop:slot":3}},enchantment_glint_override=true]'),
}
SETUP = ['cmd Alice gamerule spawn_mobs false', 'cmd Alice gamerule spawn_monsters false',
         *['cmd Alice kill @e[type=!player]'] * 3, 'ticks 20']


def compared(lines):
    """Per case, the inventory slot packets Alice got (and any error or disconnection)."""
    cases = {}
    for line in lines:
        who, step, text = line.split(' ', 2)
        step = step.removeprefix('step=')
        if who == 'Alice' and text.startswith(('slot ', 'error', 'disconnected')):
            cases.setdefault(step, []).append(text)
    return cases


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
        if not want or any(l.startswith(('error', 'disconnected')) for l in want):
            raise AssertionError(f'{case}: vanilla sent no slot or failed: {want}')
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
        scope='Item stacks a dedicated server sends a vanilla client: the component patch of an item put in the '
              'inventory by /item replace, with names (plain and styled), lore, custom data, custom model data, '
              'unbreakable, tooltip display, rarity, glint, max stack size, item model and a player head profile.'))
    print(f'server items: {len(CASES) - len(KNOWN)} of {len(CASES)} cases identical')


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        sys.exit(f'{NAME}: {error}')
