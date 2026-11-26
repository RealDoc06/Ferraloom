#!/usr/bin/env python3
"""Stopping a server and starting it again on its world keeps its entities as they were (#374).

Horizon once kept non-mob entities (minecarts, boats, armor stands...) in memory after a
world was closed; opening it again loaded them from disk too and the next save wrote both
copies, doubling them each time. A dedicated server reopens its world in a new process, so
what this checks now is that saving and loading entities neither drops nor copies them.

Vanilla's dedicated server and Ferraloom's (`horizon-client --dedicated --nogui`) each
generate a world from seed 7, force-load the chunks around the origin (seed 7 has
mineshaft chest minecarts there), and their consoles summon a minecart on a stone pad at
y 200. Once the minecarts stop changing, the console saves (`save-all flush`) and stops
the server; then the server starts again on its world three times. Each time the
minecarts in the force-loaded chunks (kind, position to 0.1 and UUID) must be the ones it
had before it first stopped, each once, and Ferraloom's must be where vanilla's are.

  test_world_reopen_parity.py [--engine PATH] [--keep]
"""
import argparse
import re
import shutil
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import lab  # noqa: E402
from test_command_exec_parity import server_dir, servers  # noqa: E402
from test_dedicated_server_parity import Server  # noqa: E402

RUN = lab.LOCAL / 'world-reopen'
CYCLES = 3
PROPERTIES = {
    'online-mode': 'false',
    'level-seed': '7',
    'view-distance': '4',
    'simulation-distance': '4',
    'spawn-protection': '0',
    'pause-when-empty-seconds': '-1',
}
KINDS = ['minecart', 'chest_minecart', 'furnace_minecart', 'hopper_minecart', 'tnt_minecart', 'spawner_minecart',
         'command_block_minecart']
# The force-loaded chunks (-3..6 on both axes, with three of seed 7's mineshaft chest minecarts); others, around
# the world spawn, are loaded only while the server starts.
AREA = 'x=-48,y=-64,z=-48,dx=160,dy=400,dz=160'
ENTITY_DATA = re.compile(r'has the following entity data: (.*)$')


def console(server, command, timeout=300):
    """Runs a console command; returns the log lines that came before the marker said after it (the console runs
    its queue in order)."""
    console.marker += 1
    marker = f'@@{console.marker}@@'
    start = len(server.lines)
    server.console(command)
    server.console(f'say {marker}')
    server.wait_for(re.escape(f'[Server] {marker}'), timeout=timeout)
    lines = server.lines[start:]
    return lines[:next(i for i, l in enumerate(lines) if l.endswith(f'[Server] {marker}'))]


console.marker = 0


def carts(server):
    """The loaded minecarts once their list stops changing: entities of the force-loaded chunks keep arriving after
    the server says it is done."""
    last = None
    for _ in range(30):
        found = []
        for kind in KINDS:
            for line in console(server, f'execute as @e[type=minecraft:{kind},{AREA}] run data get entity @s'):
                if m := ENTITY_DATA.search(line):
                    data = m.group(1)
                    pos = re.search(r'Pos: \[([-0-9.]+)d, ([-0-9.]+)d, ([-0-9.]+)d\]', data)
                    uuid = re.search(r'UUID: \[I; ([-0-9]+), ([-0-9]+), ([-0-9]+), ([-0-9]+)\]', data)
                    found.append((kind, *(round(float(v), 1) for v in pos.groups()),
                                  ','.join(uuid.groups()) if uuid else '?'))
        found.sort()
        if found == last:
            return found
        last = found
        time.sleep(2)
    return last


def run_side(label, command):
    """(the minecarts before the first stop, [the minecarts after each restart])."""
    directory = RUN / label
    server_dir(directory, PROPERTIES)
    server = Server(command, directory, f'{label}-0')
    try:
        server.wait_for(r'Done \(', timeout=600)
        console(server, 'forceload add -48 -48 111 111', timeout=600)
        deadline = time.time() + 300
        while not any('Test passed' in line for line in console(server, 'execute if loaded 1 200 1')):
            if time.time() > deadline:
                raise AssertionError(f'{label}: chunk 0 0 did not load')
            time.sleep(1)
        for line in ('fill 0 200 0 2 200 2 minecraft:stone', 'summon minecraft:minecart 1.5 201 1.5'):
            print(f'{label}: {line} ->', [reply.split(']: ', 1)[-1] for reply in console(server, line)])
        expected = carts(server)
        console(server, 'save-all flush', timeout=600)
    finally:
        server.stop()
    after = []
    for cycle in range(1, CYCLES + 1):
        server = Server(command, directory, f'{label}-{cycle}')
        try:
            server.wait_for(r'Done \(', timeout=600)
            after.append(carts(server))
        finally:
            server.stop()
    return expected, after


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--engine', type=Path, help='an already-built engine instead of compiling the fast profile')
    parser.add_argument('--keep', action='store_true', help='keep the run folder')
    args = parser.parse_args()
    RUN.mkdir(parents=True, exist_ok=True)  # each side's folder starts afresh (server_dir)
    _, vanilla, ferraloom, _ = servers(RUN, args.engine)
    failures = 0
    sides = {}
    for label, command in [('vanilla', vanilla), ('ferraloom', ferraloom)]:
        expected, after = sides[label] = run_side(label, command)
        print(f'{label}: {len(expected)} minecarts')
        if not any(kind == 'minecart' and y >= 200 for kind, x, y, z, _ in expected):
            print(f'{label}: the summoned minecart is not there: {expected}')
            failures += 1
        for cycle, got in enumerate(after, 1):
            ok = got == expected
            failures += not ok
            print(f'{label} restart {cycle}: {len(got)} minecarts', 'ok' if ok else f'FAIL (expected {expected}, got {got})')
    # UUIDs are random, but where the world and the console put minecarts is not.
    placed = {label: [cart[:4] for cart in expected] for label, (expected, _) in sides.items()}
    if placed['ferraloom'] != placed['vanilla']:
        failures += 1
        print(f'FAIL: Ferraloom\'s minecarts {placed["ferraloom"]} are not vanilla\'s {placed["vanilla"]}')
    if not args.keep and not failures:
        shutil.rmtree(RUN, ignore_errors=True)
    print('world reopen:', 'PASS' if not failures else f'{failures} FAILED')
    sys.exit(1 if failures else 0)


if __name__ == '__main__':
    main()
