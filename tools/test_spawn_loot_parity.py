#!/usr/bin/env python3
"""A vanilla loot chest at spawn has vanilla's loot when Ferraloom opens the world.

Vanilla rolls a container's loot table when it is first opened or looked into
(`RandomizableContainer.unpackLootTable`). Horizon rolls a loaded container with a loot table
and no items as its chunk is read from disk (`save::roll_loaded_loot`); the loot tables have to
be loaded before the first chunks are read, or a chest read before them comes out empty and
its loot table is lost on the next save.

The vanilla 26.3 server writes a flat world with a seeded loot chest beside the world spawn
(force-loaded), then rolls the chest itself in a copy of that world (`execute if items`
unpacks it) and prints its items. Ferraloom's dedicated server (`horizon-client --dedicated
--nogui`) opens another copy, untouched, in a new process, so nothing was loaded before the
world; its console does the same, and the items must be vanilla's, in the same slots.

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
    """What a server rolls from the chest of a copy of `world` in `directory`: [(slot, id, count)]."""
    shutil.copytree(world, directory / 'world')
    (directory / 'world' / 'session.lock').unlink(missing_ok=True)
    server = Vanilla(directory, command=command)
    try:
        server.run('forceload add 0 0')
        time.sleep(2)
        server.run('execute if items block {} {} {} container.* *'.format(*CHEST))
        text = ' '.join(server.run('data get block {} {} {} Items'.format(*CHEST)))
    finally:
        server.stop()
    items = []
    for item in re.finditer(r'\{(?:[^{}]|\{(?:[^{}]|\{[^{}]*\})*\})*\}', text.split('data:', 1)[-1]):
        found = item.group(0)
        count = re.search(r'count: (\d+)', found)
        items.append((int(re.search(r'Slot: (\d+)b', found).group(1)), re.search(r'id: "([^"]+)"', found).group(1),
                      int(count.group(1)) if count else 1))
    return sorted(items)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--engine', type=Path, help='an already-built engine instead of compiling the fast profile')
    parser.add_argument('--keep', action='store_true', help='keep the run folder')
    args = parser.parse_args()
    shutil.rmtree(RUN, ignore_errors=True)
    RUN.mkdir(parents=True)
    _, _, engine, _ = servers(RUN, args.engine)
    world = vanilla_world()
    expected = rolled(RUN / 'vv', world)
    print('vanilla rolls:', expected)
    if not expected:
        sys.exit('vanilla rolled nothing from the chest')
    got = rolled(RUN / 'h', world, engine)
    ok = got == expected
    print(f'ferraloom: {len(got)} stacks', 'ok' if ok else f'FAIL: {got}')
    if not args.keep and ok:
        shutil.rmtree(RUN, ignore_errors=True)
    print('spawn loot:', 'PASS' if ok else 'FAILED')
    sys.exit(0 if ok else 1)


if __name__ == '__main__':
    main()
