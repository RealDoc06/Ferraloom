#!/usr/bin/env python3
"""The level data pack functions are compiled at on a dedicated server, against vanilla's.

ServerFunctionLibrary.reload compiles every function with a source at `function-permission-level`
(DedicatedServer.getFunctionCompilationPermissions, GAMEMASTER by default): a command that level cannot use is an
unknown command, the function fails to load ("Failed to load function", "Whilst parsing command on line N"), and
every tag needing it fails too (TagLoader.build, "Couldn't load tag ... missing following references"). A macro
line compiles at that level when the function is instantiated (MacroFunction.MacroEntry). Whoever calls a function,
its commands keep the level they were compiled at.

A vanilla dedicated server and Horizon's (`horizon-client --dedicated --nogui`) each create a flat world holding one
data pack: `op Bob` (level 3) alone, after a comment and a continued line, and as a macro; a level-2 scoreboard
command; load and tick tags, a tag needing the tick tag, and a tag naming the `op` function as optional. The console
then calls the functions directly, through tags, `/schedule` and `/execute`, and reads the scores. Once at the
default level and once with `function-permission-level=4`. The load errors the server logs, the console's replies
and ops.json afterwards are compared exactly.
"""
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
    from .test_dedicated_server_parity import Server, free_port, game_time, wait_ticks
    from .test_locator_bar_parity import PROPERTIES
except ImportError:
    import lab
    import test_rust_parity as base
    from test_dedicated_server_parity import Server, free_port, game_time, wait_ticks
    from test_locator_bar_parity import PROPERTIES

NAME = 'server-function-permission-parity'
CASES = {'default': {}, 'level-4': {'function-permission-level': '4'}}
# data/<path> of the pack (26.3's data pack format is 121).
PACK = {
    'perm/function/op_bob.mcfunction': 'op Bob\n',
    # Line 6 is where the continued `op` starts.
    'perm/function/multi.mcfunction': 'scoreboard players set first perm 1\nscoreboard players set \\\n  x perm 1\n\n# comment\nop \\\n Bob\n',
    'perm/function/op_macro.mcfunction': '$op $(name)\n',
    'perm/function/mark.mcfunction': 'scoreboard players set mark perm 2\n',
    'perm/function/load.mcfunction': 'scoreboard objectives add perm dummy\nscoreboard players set loaded perm 1\n',
    'perm/function/tick.mcfunction': 'scoreboard players add ticked perm 1\n',
    'minecraft/tags/function/load.json': {'values': ['perm:load']},
    'minecraft/tags/function/tick.json': {'values': ['perm:tick', 'perm:op_bob']},
    'perm/tags/function/nested.json': {'values': ['#minecraft:tick']},
    'perm/tags/function/optional.json': {'values': [{'id': 'perm:op_bob', 'required': False}, 'perm:mark']},
}
CONSOLE = [
    'function perm:op_bob',
    'function perm:mark',
    'schedule function perm:op_bob 1t',
    'execute run function perm:op_bob',
    'function perm:op_macro {name:"Carol"}',
    'function perm:multi',
    'function #perm:optional',
    'function #perm:nested',
    'function #minecraft:tick',
    'scoreboard players get mark perm',
    'scoreboard players get loaded perm',
    'execute if score ticked perm matches 1..',
]
CONSOLE_LINE = re.compile(r'^\[\d\d:\d\d:\d\d\] \[[^\]/]+/(\w+)\]: (.*)$')
# What log4j prints as the first line of the exception vanilla logs with "Failed to load function".
JAVA_CAUSE = re.compile(r'^java\.util\.concurrent\.CompletionException: java\.lang\.IllegalArgumentException: ')


def write_pack(world):
    pack = world / 'datapacks' / 'perm'
    for path, content in PACK.items():
        file = pack / 'data' / path
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(content if isinstance(content, str) else json.dumps(content), encoding='utf-8')
    (pack / 'pack.mcmeta').write_text(json.dumps({'pack': {'description': 'perm', 'min_format': 121,
                                                           'max_format': 121}}), encoding='utf-8')


def load_errors(lines):
    """The data pack load errors, each with its cause, in a stable order (vanilla loads functions in parallel)."""
    out = []
    for i, line in enumerate(lines):
        m = CONSOLE_LINE.match(line)
        if not m or not re.match(r"Failed to load function|Couldn't load tag", m.group(2)):
            continue
        text = f'{m.group(1)} {m.group(2)}'
        if m.group(2).startswith('Failed to load function') and i + 1 < len(lines):
            text += ' | ' + JAVA_CAUSE.sub('', lines[i + 1])
        out.append(text)
    functions = sorted(l for l in out if 'Failed to load function' in l)
    return functions + [l for l in out if "Couldn't load tag" in l]


def replies(lines):
    """The console's replies (the game time queries that pace the steps aside)."""
    out = []
    for line in lines:
        m = CONSOLE_LINE.match(line)
        if m and m.group(2).startswith('System chat: ') and not re.search(r'The (game )?time is', m.group(2)):
            out.append(re.sub(r'at gametime \d+', 'at gametime _', m.group(2)))
    return out


def run_side(label, case, command, build):
    directory = build / label / case
    if directory.exists():
        shutil.rmtree(directory)
    (directory / 'world').mkdir(parents=True)
    props = dict(PROPERTIES, **{'server-port': str(free_port())}, **CASES[case])
    (directory / 'server.properties').write_text(''.join(f'{k}={v}\n' for k, v in props.items()),
                                                 encoding='iso-8859-1')
    (directory / 'eula.txt').write_text('eula=true\n')
    write_pack(directory / 'world')
    server = Server(command, directory, f'{label}-{case}')
    try:
        server.wait_for(r'Done \(')
        game_time(server)
        for line in CONSOLE:
            start = len(server.lines)
            server.console(line)
            game_time(server)
            if line.startswith('schedule'):
                wait_ticks(server, 2)  # the scheduled call runs (or not) before the next step
            if not replies(server.lines[start:]):
                raise AssertionError(f'{label} {case}: no reply to {line!r} (see {server.log_path})')
    finally:
        server.stop()
    ops_file = directory / 'ops.json'
    # Sorted: vanilla writes ops.json in its map's hash order.
    ops = sorted(f'{case} op {o["name"]} {o["uuid"]} {o["level"]} {o["bypassesPlayerLimit"]}'
           for o in json.loads(ops_file.read_text(encoding='utf-8'))) if ops_file.exists() else [f'{case} no ops.json']
    return [f'{case} load {l}' for l in load_errors(server.lines)] + [f'{case} reply {l}' for l in replies(server.lines)] + ops


def main():
    lock, jars, libraries = base.verify_artifacts()
    build = lab.LOCAL / NAME
    build.mkdir(parents=True, exist_ok=True)
    report = build / 'report.json'
    report.unlink(missing_ok=True)
    dependencies = os.pathsep.join(map(str, [jars['server'], *libraries]))
    # The server build: no audio or controller libraries.
    subprocess.run(['cargo', 'build', '--locked', '--profile', 'fast', '-p', 'horizon-client', '--no-default-features',
                    '--bin', 'horizon-client'], cwd=lab.ROOT, check=True, capture_output=True)
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'],
                                                  cwd=lab.ROOT, text=True))
    binary = Path(metadata['target_directory']) / 'fast' / ('horizon-client.exe' if os.name == 'nt' else 'horizon-client')
    vanilla_server = ['java', '-Xmx2g', '-Duser.timezone=UTC', '-cp', dependencies, 'net.minecraft.server.Main', 'nogui']
    horizon_server = [binary, '--resources', lab.LOCAL / 'horizon-client/26.3', '--dedicated', '--nogui']

    vanilla, horizon = [], []
    for case in CASES:
        vanilla += run_side('vanilla', case, vanilla_server, build)
        horizon += run_side('horizon', case, horizon_server, build)
    (build / 'vanilla.txt').write_text('\n'.join(vanilla) + '\n', encoding='utf-8')
    (build / 'horizon.txt').write_text('\n'.join(horizon) + '\n', encoding='utf-8')
    if vanilla != horizon:
        import difflib
        diff = '\n'.join(difflib.unified_diff(vanilla, horizon, 'vanilla', 'horizon', n=1, lineterm=''))
        raise AssertionError(f'transcripts differ (see {build}/vanilla.txt and horizon.txt):\n' + diff[:6000])
    # The script ran: the default level refused `op` at load, level 4 ran it from the tick tag and the macro.
    if not any(l.startswith('default load ERROR Failed to load function perm:op_bob') for l in vanilla) \
            or not any(l.startswith('level-4 op Carol') for l in vanilla):
        raise AssertionError(f'vanilla did not do what the script expects (see {build}/vanilla.txt)')
    lab.write_json(report, dict(
        schema=1, minecraft=lock['minecraft']['version'], platform=platform.platform(),
        java=subprocess.check_output(['java', '--version'], text=True).splitlines()[0],
        lines=len(vanilla), output_sha256=lab.digest(build / 'horizon.txt', 'sha256'),
        scope='Data pack functions on a dedicated server at function-permission-level 2 and 4: the load errors '
              'of functions using a level-3 command (plain, after a continued line, as a macro) and of the tags '
              'needing them, the console calling them directly, through tags, /schedule and /execute, and the '
              'operators they made.'))
    print(f'server function permission: {len(vanilla)} lines identical')


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        sys.exit(f'{NAME}: {error}')
