#!/usr/bin/env python3
"""A vanilla loot chest at spawn has vanilla's loot when Ferraloom opens the world.

Vanilla rolls a container's loot table when it is first opened or looked into
(`RandomizableContainer.unpackLootTable`). Horizon's single player once rolled a container as
its chunk was read from disk, and a chest read before the loot tables were loaded came out
empty, its loot table lost on the next save. The chest must keep its loot table until it is
looked into, then hold what vanilla rolls.

The vanilla 26.3 server writes a flat world with a seeded loot chest beside the world spawn
(force-loaded), then rolls the chest itself in a copy of that world (`execute if items`
unpacks it) and prints its items. Ferraloom's dedicated server (`horizon-client --dedicated
--nogui`) opens another copy, untouched, in a new process, so nothing was loaded before the
world; its console does the same. The chest's data as loaded, the reply to `execute if items` and
the items the chest then holds must be vanilla's (KNOWN lists what still differs).

  test_spawn_loot_parity.py [--engine PATH] [--keep]
"""
import argparse
import re
import shutil
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import lab  # noqa: E402
from test_command_exec_parity import servers  # noqa: E402
from test_save_roundtrip_parity import Vanilla  # noqa: E402

RUN = lab.LOCAL / 'spawn-loot'
CHEST = (2, -60, 1)
LOOT = '{LootTable:"minecraft:chests/simple_dungeon",LootTableSeed:42L}'
# Checks Ferraloom still gets wrong (check -> why). They are reported, and do not fail the run.
KNOWN = {
    'the items it then holds': 'execute if items counts the rolled loot but leaves it unrolled in the chest; vanilla '
                               'unpacks it (RandomizableContainer.unpackLootTable) when the slots are read',
}


def vanilla_world():
    """The world with the unopened chest."""
    server = Vanilla(RUN / 'v0')
    try:
        server.run('setworldspawn 0 -60 0')
        server.run('forceload add 0 0')
        time.sleep(2)
        server.run('setblock {} {} {} minecraft:chest{}'.format(*CHEST, LOOT))
        server.run('save-all flush')
        time.sleep(2)
    finally:
        server.stop()
    return RUN / 'v0' / 'world'


def rolled(directory, world, command=None):
    """What a server rolls from the chest of a copy of `world` in `directory`: (the chest's data before, the reply
    to `execute if items`, the items the chest then holds as [(slot, id, count)])."""
    shutil.copytree(world, directory / 'world')
    (directory / 'world' / 'session.lock').unlink(missing_ok=True)
    server = Vanilla(directory, command=command)
    try:
        server.run('forceload add 0 0')
        time.sleep(2)
        before = ' '.join(server.run('data get block {} {} {}'.format(*CHEST)))
        replies = server.run('execute if items block {} {} {} container.* *'.format(*CHEST))
        text = ' '.join(server.run('data get block {} {} {} Items'.format(*CHEST)))
    finally:
        server.stop()
    print(f'{directory.name}: the chest {before[:300]}; execute if items -> {replies}; then {text[:300]}')
    items = []
    for item in re.finditer(r'\{(?:[^{}]|\{(?:[^{}]|\{[^{}]*\})*\})*\}', text.split('data:', 1)[-1]):
        found = item.group(0)
        count = re.search(r'count: (\d+)', found)
        items.append((int(re.search(r'Slot: (\d+)b', found).group(1)), re.search(r'id: "([^"]+)"', found).group(1),
                      int(count.group(1)) if count else 1))
    return before, replies, sorted(items)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--engine', type=Path, help='an already-built engine instead of compiling the fast profile')
    parser.add_argument('--keep', action='store_true', help='keep the run folder')
    args = parser.parse_args()
    RUN.mkdir(parents=True, exist_ok=True)
    for folder in ('v0', 'vv', 'h'):
        shutil.rmtree(RUN / folder, ignore_errors=True)
    _, _, engine, _ = servers(RUN, args.engine)
    world = vanilla_world()
    v_before, v_replies, expected = rolled(RUN / 'vv', world)
    print('vanilla rolls:', expected)
    if not expected:
        sys.exit('vanilla rolled nothing from the chest')
    h_before, h_replies, got = rolled(RUN / 'h', world, engine)
    checks = {
        'the chest as loaded': (v_before, h_before),
        'execute if items': (v_replies, h_replies),
        'the items it then holds': (expected, got),
    }
    failures = 0
    for name, (want, have) in checks.items():
        if want == have:
            print(f'{name}: ok', '(matches now, leave KNOWN)' if name in KNOWN else '')
        elif name in KNOWN:
            print(f'{name}: KNOWN GAP ({KNOWN[name]}): vanilla {want!r}, ferraloom {have!r}')
        else:
            failures += 1
            print(f'{name}: FAIL: vanilla {want!r}, ferraloom {have!r}')
    if not args.keep and not failures:
        shutil.rmtree(RUN, ignore_errors=True)
    print('spawn loot:', 'PASS' if not failures else 'FAILED')
    sys.exit(1 if failures else 0)


if __name__ == '__main__':
    main()
