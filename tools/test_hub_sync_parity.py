#!/usr/bin/env python3
"""Ferraloom's hub against vanilla's dedicated server: what each player is synced and checked for.

A vanilla dedicated server and Ferraloom's (`horizon-client --dedicated --nogui`) each serve a copy of one flat
world vanilla generated first, with the same server.properties (offline, survival, a code of conduct, the default
`allow-flight=false` and spam thresholds). Alice has been here before: her saved file puts a parrot on her left
shoulder, where it stays (she is never hurt, never spectates, never stands in powder snow: a parrot that leaves
lands again on vanilla within seconds, which Ferraloom's mobs, who see only the nearest player, don't do yet). HubSyncClient, players made of vanilla's protocol code (any byte vanilla would not read is an error),
then runs one script on each server and records, step by step, what each player's client would make of it:

- the code of conduct each player is shown and accepts (ServerCodeOfConductConfigurationTask);
- the entity data each holds of itself and of the others it sees (`snapshot`): the shoulder parrot
  (`Player.DATA_SHOULDER_PARROT_LEFT`), the score (`DATA_SCORE_ID`, after `xp add`), its own and others' effect
  swirls (`DATA_EFFECT_PARTICLES`), the ticks frozen (`DATA_TICKS_FROZEN`) standing in powder snow and after;
- the tab list's hat flag of a player who joins without the hat layer and then shows it (`UPDATE_HAT`);
- the scores of displayed objectives on the `health`, `level` and `xp` criteria (ServerPlayer.doTick);
- which players each sees while one is a spectator (ServerPlayer.broadcastToPlayer), before and after it moves;
- the status's player sample, with a player who doesn't allow listing (`Anonymous Player`);
- the pong to a play-state ping request;
- the advancements tab the server selects for the tabs a player opens (ServerboundSeenAdvancementsPacket);
- the recipe book's highlights after a seen recipe, across a rejoin;
- who is kicked: a player spamming chat or commands (`disconnect.spam`), one floating 10 blocks off the ground
  for 130 ticks (`multiplayer.disconnect.flying`), and not one standing on it or held 3 blocks up (the server's
  own gravity step lands it every few ticks).

Each player's lines in each step are compared exactly (sorted within the step, where the order of packets of one
tick is the servers' own).
"""
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import socket
import struct
import subprocess
import sys
import time
import uuid as uuidlib
import gzip

try:
    from . import lab, test_rust_parity as base
    from .test_dedicated_server_parity import Server, free_port, wait_ticks
    from .test_locator_bar_parity import Client
except ImportError:
    import lab
    import test_rust_parity as base
    from test_dedicated_server_parity import Server, free_port, wait_ticks
    from test_locator_bar_parity import Client

NAME = 'hub-sync-parity'
PROPERTIES = {
    'online-mode': 'false',
    'white-list': 'false',
    'level-seed': '12345',
    'level-type': 'minecraft\\:flat',
    'generate-structures': 'false',
    'gamemode': 'survival',
    # No slimes on the flat world to kill the players standing about.
    'difficulty': 'peaceful',
    'view-distance': '4',
    'simulation-distance': '4',
    'spawn-protection': '0',
    'max-players': '10',
    'pause-when-empty-seconds': '-1',
    'enable-code-of-conduct': 'true',
    # A slow disk on a shared machine stalls a save past vanilla's watchdog.
    'max-tick-time': '-1',
}
# The colour codes are taken out (StringUtil.stripColor), the line break kept.
CODE_OF_CONDUCT = '§aBe kind§r to everyone.\nNo griefing.\n'
# Operators may spam (detectRateSpam) and run `time query` (the client's sync); Dave, Erin and Gina, who are
# kicked, are not.
OPS = ['Alice', 'Bob', 'Carol', 'Frank']
STEP_TICKS = 40

# (label, [what the console does], [what the players do], ticks to wait after).
SCRIPT = [
    ('alice-joins', [], ['join Alice', 'snapshot Alice'], STEP_TICKS),
    ('bob-joins', [], ['join Bob nohat'], STEP_TICKS),
    ('bob-sees', [], ['snapshot Alice', 'snapshot Bob'], 1),
    ('bob-hat-on', [], ['hat Bob on'], STEP_TICKS),
    # Clients are sent the scores of displayed objectives only (ServerScoreboard.startTrackingObjective). The
    # changes come a step later: a player's command could run before the console's (vanilla runs them apart).
    ('criteria', ['scoreboard objectives add hp health', 'scoreboard objectives add lvl level',
                  'scoreboard objectives add xpo xp', 'scoreboard objectives setdisplay list hp',
                  'scoreboard objectives setdisplay sidebar lvl', 'scoreboard objectives setdisplay below_name xpo'],
     [], STEP_TICKS),
    # Absorption (its swirls hidden) raises the health criteria without a hurt, which would send Alice's parrot
    # off her shoulder.
    ('criteria-change', ['xp add Alice 30 points', 'effect give Alice minecraft:speed 1000 1',
                         'effect give Alice minecraft:absorption 1000 0 true'], [], STEP_TICKS),
    ('after-criteria', [], ['snapshot Alice', 'snapshot Bob'], 1),
    # Bob spectates (Alice keeps her parrot: a spectator's leaves its shoulder).
    ('spectator', ['gamemode spectator Bob'], ['join Carol unlisted'], STEP_TICKS),
    ('spectator-moves', ['tp Bob 20.5 -60 0.5'], [], STEP_TICKS),
    # Back, away from the others at the spawn.
    ('survival', ['gamemode survival Bob', 'tp Bob 5.5 -60 5.5'], [], STEP_TICKS),
    ('ping', [], ['ping Alice 42'], STEP_TICKS),
    ('tabs', [], ['tab Alice minecraft:story/root', 'tab Alice minecraft:story/mine_stone',
                  'tab Alice minecraft:story/mine_stone', 'tab Alice minecraft:nope',
                  'tab Alice minecraft:adventure/root'], STEP_TICKS),
    ('freeze', ['setblock 5 -60 5 minecraft:powder_snow'], ['hold Bob 170', 'snapshot Alice', 'snapshot Bob'], 1),
    ('thaw', ['setblock 5 -60 5 minecraft:air'], ['hold Bob 100', 'snapshot Alice', 'snapshot Bob'], 1),
    ('recipes', ['recipe give Alice minecraft:crafting_table', 'recipe give Alice minecraft:chest'], [], STEP_TICKS),
    ('recipe-seen', [], ['seen Alice'], STEP_TICKS),
    ('alice-leaves', [], ['quit Alice'], STEP_TICKS),
    ('alice-rejoins', [], ['join Alice'], STEP_TICKS),
    ('chat-spam', [], ['join Dave', 'chat Dave 12'], STEP_TICKS),
    ('command-spam', [], ['join Erin', 'commands Erin 12'], STEP_TICKS),
    # Held 3 blocks up the server's own fall reaches the ground within a tick before the 80 are up
    # (verticalCollisionBelow), and that counts as standing; 10 blocks up it never does.
    ('standing', [], ['join Frank', 'hold Frank 40', 'float Frank 130 3'], STEP_TICKS),
    ('floating', [], ['join Gina', 'float Gina 130 10'], STEP_TICKS),
]
# Where a line of a step counts: the scores only where the script changes the criteria (freezing hurts later).
SCORE_STEPS = {'criteria', 'criteria-change'}


def offline_uuid(name):
    digest = bytearray(hashlib.md5(f'OfflinePlayer:{name}'.encode()).digest())
    digest[6] = (digest[6] & 0x0F) | 0x30
    digest[8] = (digest[8] & 0x3F) | 0x80
    return uuidlib.UUID(bytes=bytes(digest))


def nbt(value, name=None):
    """A minimal NBT writer: dicts are compounds, ('int', n) / ('double', x) tagged, str strings, lists of
    doubles."""
    def payload(v):
        if isinstance(v, dict):
            return 10, b''.join(nbt(x, k) for k, x in v.items()) + b'\0'
        if isinstance(v, str):
            data = v.encode('utf-8')
            return 8, struct.pack('>H', len(data)) + data
        if isinstance(v, tuple) and v[0] == 'int':
            return 3, struct.pack('>i', v[1])
        if isinstance(v, list):
            return 9, bytes([6]) + struct.pack('>i', len(v)) + b''.join(struct.pack('>d', x) for x in v)
        raise TypeError(v)
    kind, data = payload(value)
    head = bytes([kind]) + (struct.pack('>H', len(name.encode())) + name.encode() if name is not None else b'')
    return head + data


def alice_file(world):
    """Alice's saved player: at the spawn, a green parrot (Variant 2) on her left shoulder."""
    tag = {
        'DataVersion': ('int', 5023),
        'Pos': [0.5, -60.0, 0.5],
        'Dimension': 'minecraft:overworld',
        'playerGameType': ('int', 0),
        'ShoulderEntityLeft': {'id': 'minecraft:parrot', 'Variant': ('int', 2)},
    }
    directory = world / 'players' / 'data'
    directory.mkdir(parents=True, exist_ok=True)
    (directory / f'{offline_uuid("Alice")}.dat').write_bytes(gzip.compress(nbt(tag, '')))


def generate_world(server_command, build):
    """The flat world both servers load, made once by vanilla: players appear at its spawn (respawn_radius 0)
    on the ground at 0 -60 0, and the chunks around it are saved."""
    directory = build / 'generate'
    if directory.exists():
        shutil.rmtree(directory)
    directory.mkdir(parents=True)
    props = dict(PROPERTIES, **{'server-port': str(free_port()), 'enable-code-of-conduct': 'false'})
    (directory / 'server.properties').write_text(''.join(f'{k}={v}\n' for k, v in props.items()),
                                                 encoding='iso-8859-1')
    (directory / 'eula.txt').write_text('eula=true\n')
    server = Server(server_command, directory, 'generate')
    try:
        server.wait_for(r'Done \(')
        for command, reply in [('gamerule respawn_radius 0', 'respawn_radius'),
                               ('setworldspawn 0 -60 0', 'Set the world spawn'),
                               ('forceload add -64 -64 64 64', 'force loaded'),
                               ('save-all flush', 'Saved the game')]:
            server.console(command)
            server.wait_for(reply, timeout=300)
        server.console('forceload remove all')
    finally:
        server.stop()
    return directory / 'world'


def server_dir(directory, world):
    if directory.exists():
        shutil.rmtree(directory)
    directory.mkdir(parents=True)
    port = free_port()
    props = dict(PROPERTIES, **{'server-port': str(port)})
    (directory / 'server.properties').write_text(''.join(f'{k}={v}\n' for k, v in props.items()),
                                                 encoding='iso-8859-1')
    (directory / 'eula.txt').write_text('eula=true\n')
    (directory / 'ops.json').write_text(json.dumps([
        {'uuid': str(offline_uuid(n)), 'name': n, 'level': 4, 'bypassesPlayerLimit': False} for n in OPS]))
    (directory / 'codeofconduct').mkdir()
    (directory / 'codeofconduct' / 'en_us.txt').write_text(CODE_OF_CONDUCT, encoding='utf-8')
    shutil.copytree(world, directory / 'world')
    (directory / 'world' / 'session.lock').unlink(missing_ok=True)
    alice_file(directory / 'world')
    return port


def varint(n):
    out = b''
    n &= 0xFFFFFFFF
    while True:
        b = n & 0x7F
        n >>= 7
        out += bytes([b | (0x80 if n else 0)])
        if not n:
            return out


def status_sample(port):
    """The status response's player sample (names, sorted: vanilla shuffles them)."""
    def read_varint(s):
        value = 0
        for shift in range(0, 35, 7):
            b = s.recv(1)[0]
            value |= (b & 0x7F) << shift
            if not b & 0x80:
                return value
        raise ValueError('varint')
    with socket.create_connection(('127.0.0.1', port), timeout=10) as s:
        host = b'127.0.0.1'
        hello = varint(0) + varint(777) + varint(len(host)) + host + struct.pack('>H', port) + varint(1)
        s.sendall(varint(len(hello)) + hello + varint(1) + varint(0))
        length = read_varint(s)
        data = b''
        while len(data) < length:
            data += s.recv(length - len(data))
    body = data[1:]
    n, shift, i = 0, 0, 0
    while True:
        b = body[i]
        n |= (b & 0x7F) << shift
        i += 1
        shift += 7
        if not b & 0x80:
            break
    status = json.loads(body[i:i + n])
    return sorted(f'{p["name"]} {p["id"]}' for p in status['players'].get('sample', []))


def joined_count(server, name):
    return sum(f'{name} joined the game' in l for l in server.lines)


def run_side(label, server_command, client_command, build, world):
    directory = build / label
    port = server_dir(directory, world)
    out = build / f'{label}.txt'
    out.unlink(missing_ok=True)
    server = Server(server_command, directory, label)
    status = []
    try:
        server.wait_for(r'Done \(')
        with (build / f'{label}-client.log').open('w') as log:
            client = Client(client_command, port, out, log)
            synced = []
            try:
                for step, console, actions, ticks in SCRIPT:
                    client.tell(f'step {step}')
                    for command in console:
                        server.console(command)
                    for action in actions:
                        verb, *rest = action.split()
                        before = joined_count(server, rest[0]) if verb == 'join' else 0
                        client.tell(action)
                        if verb == 'join':
                            # Joining takes the world's first chunks: wait for the server to place the player.
                            deadline = time.time() + 120
                            while joined_count(server, rest[0]) == before:
                                if time.time() > deadline:
                                    raise AssertionError(f'{label}: {rest[0]} did not join')
                                time.sleep(0.2)
                            if rest[0] in OPS:
                                synced.append(rest[0])
                            client.wait(server, synced, STEP_TICKS)
                        elif verb == 'quit':
                            synced.remove(rest[0])
                        elif verb in ('hold', 'float'):
                            # The client runs its lines in order: this has ended once a player is synced.
                            client.sync(synced)
                    if step == 'spectator':
                        client.wait(server, synced, STEP_TICKS)
                        status = status_sample(port)
                    client.wait(server, synced, ticks)
                client.end()
            finally:
                client.kill()
    finally:
        server.stop()
    if not out.exists():
        raise AssertionError(f'{label}: the client wrote no transcript (see {build}/{label}-client.log)')
    lines = out.read_text(encoding='utf-8').splitlines()
    return lines + [f'status sample {s}' for s in status]


def shown(lines):
    """Each player's lines by step, sorted within the step; scores only where the script changes them."""
    steps = {}
    for line in lines:
        m = re.match(r'(\S+) step=(\S+) (.*)', line)
        who, step, rest = m.groups() if m else ('server', 'status', line)
        if rest.startswith('score ') and step not in SCORE_STEPS:
            continue
        steps.setdefault((step, who), []).append(rest)
    step_rank = {s[0]: i for i, s in enumerate(SCRIPT)}
    out = []
    for step, who in sorted(steps, key=lambda k: (step_rank.get(k[0], len(SCRIPT)), k[1])):
        out.extend(f'{who} step={step} {rest}' for rest in sorted(steps[(step, who)]))
    return out


def main():
    lock, jars, libraries = base.verify_artifacts()
    build = lab.LOCAL / NAME
    build.mkdir(parents=True, exist_ok=True)
    report = build / 'report.json'
    report.unlink(missing_ok=True)
    oracle = lab.ROOT / 'mods/horizon/src/test/java/dev/horizonlab/parity/HubSyncClient.java'
    classes = build / 'classes'
    shutil.rmtree(classes, ignore_errors=True)
    classes.mkdir()
    dependencies = os.pathsep.join(map(str, [jars['server'], *libraries]))
    subprocess.run(['javac', '--release', str(lock['minecraft']['java_major']), '-proc:none', '-nowarn', '-sourcepath',
                    lab.ROOT / 'mods/horizon/src/test/java', '-cp', dependencies, '-d', classes, oracle],
                   cwd=lab.ROOT, check=True)
    subprocess.run(['cargo', 'build', '--profile', 'fast', '-p', 'horizon-client', '--no-default-features', '--bin',
                    'horizon-client'], cwd=lab.ROOT, check=True)
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'],
                                                  cwd=lab.ROOT, text=True))
    binary = Path(metadata['target_directory']) / 'fast' / ('horizon-client.exe' if os.name == 'nt' else 'horizon-client')
    vanilla_server = ['java', '-Xmx2g', '-Duser.timezone=UTC', '-cp', dependencies, 'net.minecraft.server.Main', 'nogui']
    horizon_server = [binary, '--resources', lab.LOCAL / 'horizon-client/26.3', '--dedicated', '--nogui']
    client = ['java', '-Xmx1g', '-cp', str(classes) + os.pathsep + dependencies, 'dev.horizonlab.parity.HubSyncClient']
    world = generate_world(vanilla_server, build)
    vanilla = run_side('vanilla', vanilla_server, client, build, world)
    horizon = run_side('horizon', horizon_server, client, build, world)
    errors = [l for l in vanilla + horizon if ' error ' in f' {l} ']
    vanilla, horizon = shown(vanilla), shown(horizon)
    (build / 'vanilla-shown.txt').write_text('\n'.join(vanilla) + '\n', encoding='utf-8')
    (build / 'horizon-shown.txt').write_text('\n'.join(horizon) + '\n', encoding='utf-8')
    if errors or vanilla != horizon:
        import difflib
        diff = '\n'.join(difflib.unified_diff(vanilla, horizon, 'vanilla', 'horizon', n=1, lineterm=''))
        raise AssertionError(f'{len(errors)} errors, transcripts differ (see {build}/vanilla-shown.txt and '
                             f'horizon-shown.txt):\n' + '\n'.join(errors[:10]) + '\n' + diff[:8000])
    # The script ran: vanilla kicked the spammers and the floater and showed the parrot.
    for wanted in ['disconnected disconnect.spam', 'disconnected multiplayer.disconnect.flying', 'OptionalInt[2]',
                   'code of conduct Be kind to everyone.\\nNo griefing.', 'pong 42', 'Anonymous Player']:
        if not any(wanted in l for l in vanilla):
            raise AssertionError(f'vanilla never showed {wanted!r}: the script did not run (see {build})')
    lab.write_json(report, dict(
        schema=1, minecraft=lock['minecraft']['version'], platform=platform.platform(),
        java=subprocess.check_output(['java', '--version'], text=True).splitlines()[0],
        lines=len(vanilla), output_sha256=lab.digest(build / 'horizon-shown.txt', 'sha256'),
        scope='The hub\'s per-player sync and checks seen by vanilla-codec players: code of conduct, shoulder '
              'parrots, score, own and others\' effect swirls, ticks frozen, the hat in the tab list, health/level/'
              'xp criteria, spectators hidden from non-spectators, the status sample\'s anonymous players, play '
              'pings, the selected advancements tab, recipe highlights across a rejoin, chat and command spam '
              'kicks, the floating kick.'))
    print(f'hub sync: {len(vanilla)} lines identical')


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        sys.exit(f'{NAME}: {error}')
