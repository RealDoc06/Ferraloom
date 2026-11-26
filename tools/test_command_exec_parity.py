#!/usr/bin/env python3
"""Compare Ferraloom's command execution with vanilla's, both as dedicated servers.

A vanilla dedicated server (Commands.CommandSelection.DEDICATED) and Ferraloom's
(`horizon-client --dedicated --nogui`) each generate a world from seed 1 with the
same server.properties. CommandExecClient, a player made of vanilla's protocol
code (any byte vanilla would not read is an error), joins each one as an
operator (level 4, ops.json), freezes the game and runs the script of
tools/command_exec_corpus.py through ServerboundChatCommandPacket. Each recorded
command's messages are compared: every system message that is not an overlay
and every player or disguised chat message, decorated by its chat type, as
text and resolved style (colour, bold, italic, underlined, strikethrough,
obfuscated). So is the result its CommandResultCallback was given last, read
back through `execute store ... storage` (see CommandExecClient). World, entity
and item changes are observed through probe commands whose feedback reports
them (execute if, clear with a zero count, data get, scoreboard queries...).
Messages that depend on time are compared after masking.

KNOWN lists the commands Ferraloom still gets wrong and why; one that matches
again is reported, and should leave the list.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import uuid as uuidlib

try:
    from . import lab, test_rust_parity as base
    from .test_dedicated_server_parity import Server
    from . import command_exec_corpus as corpus_module
except ImportError:
    import lab
    import test_rust_parity as base
    from test_dedicated_server_parity import Server
    import command_exec_corpus as corpus_module

NAME = 'command-exec-parity'
SEED = 1
PLAYER = 'Player'
PROPERTIES = {
    'online-mode': 'false',
    'white-list': 'false',
    'level-seed': str(SEED),
    'view-distance': '4',
    'simulation-distance': '4',
    'spawn-protection': '0',
    'max-players': '8',
    'allow-flight': 'true',
    'pause-when-empty-seconds': '-1',
}
# Commands Ferraloom still gets wrong (the command as in the corpus -> why). Their mismatches are reported, and do
# not fail the run.
KNOWN = {}


def offline_uuid(name):
    """The UUID an offline-mode server gives a player (UUIDUtil.createOfflinePlayerUUID)."""
    digest = bytearray(hashlib.md5(f'OfflinePlayer:{name}'.encode()).digest())
    digest[6] = (digest[6] & 0x0F) | 0x30
    digest[8] = (digest[8] & 0x3F) | 0x80
    return str(uuidlib.UUID(bytes=bytes(digest)))


def server_dir(directory, properties, operators=(), world=None):
    """A fresh server folder: server.properties (and `server-port`, a free port, which this returns), eula.txt,
    ops.json naming `operators` at level 4, and a copy of `world` as its world."""
    if directory.exists():
        shutil.rmtree(directory)
    directory.mkdir(parents=True)
    port = lab.free_port()
    props = dict(properties, **{'server-port': str(port)})
    (directory / 'server.properties').write_text(''.join(f'{k}={v}\n' for k, v in props.items()), encoding='iso-8859-1')
    (directory / 'eula.txt').write_text('eula=true\n')
    ops = [{'uuid': offline_uuid(n), 'name': n, 'level': 4, 'bypassesPlayerLimit': False} for n in operators]
    (directory / 'ops.json').write_text(json.dumps(ops, indent=2))
    if world is not None:
        shutil.copytree(world, directory / 'world')
        (directory / 'world' / 'session.lock').unlink(missing_ok=True)
    return port


def servers(build, engine=None):
    """The commands that start vanilla's dedicated server and Ferraloom's (built with the fast profile, without
    audio or controller libraries, unless `engine` names a built one), and the classpath of the vanilla server
    with lock: (lock, vanilla command, Ferraloom command, classpath)."""
    lock, jars, libraries = base.verify_artifacts()
    if engine is None:
        with (build / 'compile-rust.log').open('w') as log:
            subprocess.run(['cargo', 'build', '--locked', '--profile', 'fast', '-p', 'horizon-client',
                            '--no-default-features', '--bin', 'horizon-client'], cwd=lab.ROOT, check=True,
                           stdout=log, stderr=subprocess.STDOUT)
        metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'],
                                                      cwd=lab.ROOT, text=True))
        engine = Path(metadata['target_directory']) / 'fast' / ('horizon-client.exe' if os.name == 'nt' else 'horizon-client')
    classpath = os.pathsep.join(map(str, [jars['server'], *libraries]))
    vanilla = ['java', '-Xmx2g', '-Duser.timezone=UTC', '-cp', classpath, 'net.minecraft.server.Main', 'nogui']
    ferraloom = [Path(engine).resolve(), '--resources', lab.RESOURCES, '--dedicated', '--nogui']
    return lock, vanilla, ferraloom, classpath


def compile_java(build, lock, classpath, source):
    """Compiles one of mods/horizon/src/test/java's parity classes (and what it uses) into build/classes."""
    classes = build / 'classes'
    shutil.rmtree(classes, ignore_errors=True)
    classes.mkdir()
    with (build / 'compile-java.log').open('w') as log:
        subprocess.run(['javac', '--release', str(lock['minecraft']['java_major']), '-proc:none', '-nowarn',
                        '-sourcepath', lab.ROOT / 'mods/horizon/src/test/java', '-cp', classpath, '-d', classes,
                        lab.ROOT / 'mods/horizon/src/test/java/dev/horizonlab/parity' / f'{source}.java'],
                       cwd=lab.ROOT, check=True, stdout=log, stderr=subprocess.STDOUT)
    return classes


def run_side(label, server_command, client_command, build, script_path):
    """One server running the script: its records."""
    directory = build / label
    port = server_dir(directory, PROPERTIES, [PLAYER])
    out = build / f'{label}.jsonl'
    out.unlink(missing_ok=True)
    server = Server(server_command, directory, label)
    try:
        server.wait_for(r'Done \(', timeout=600)
        with (build / f'{label}-client.log').open('w') as log:
            result = subprocess.run([*map(str, client_command), '127.0.0.1', str(port), PLAYER, script_path, out],
                                    cwd=lab.ROOT, stdout=log, stderr=subprocess.STDOUT, timeout=3600)
        if result.returncode:
            tail = (build / f'{label}-client.log').read_text(errors='replace').splitlines()[-30:]
            server_tail = server.lines[-15:]
            raise AssertionError(f'{label}: the client failed ({result.returncode}):\n' + '\n'.join(tail)
                                 + f'\n{label} server:\n' + '\n'.join(server_tail))
    finally:
        server.stop()
    return [json.loads(line) for line in out.open(encoding='utf-8')]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prepare', action='store_true')
    parser.add_argument('--limit', type=int, default=40, help='print at most this many mismatches')
    parser.add_argument('--allow-gaps', action='store_true', help='report mismatches without failing')
    parser.add_argument('--only', help='run only corpus groups whose name contains this')
    parser.add_argument('--engine', type=Path, help='an already-built engine instead of compiling the fast profile')
    args = parser.parse_args()
    build = lab.LOCAL / NAME
    build.mkdir(parents=True, exist_ok=True)
    report_path = build / 'report.json'
    report_path.unlink(missing_ok=True)
    if args.prepare:
        lab.prepare()
    lock, vanilla_server, ferraloom_server, classpath = servers(build, args.engine)
    classes = compile_java(build, lock, classpath, 'CommandExecClient')
    client = ['java', '-Xmx1g', '-Duser.timezone=UTC', '-cp', str(classes) + os.pathsep + classpath,
              'dev.horizonlab.parity.CommandExecClient']

    groups = corpus_module.groups()
    if args.only:
        groups = [g for g in groups if args.only in g[0] or g[0] == 'setup']
    script = [op for _, lines in groups for op in lines]
    script_path = build / 'script.jsonl'
    script_path.write_text(''.join(json.dumps(op, ensure_ascii=False) + '\n' for op in script), encoding='utf-8')

    vanilla = run_side('vanilla', vanilla_server, client, build, script_path)
    rust = run_side('ferraloom', ferraloom_server, client, build, script_path)
    if len(vanilla) != len(rust):
        sys.exit(f'record count differs: vanilla {len(vanilla)} ferraloom {len(rust)}')
    commands_by_group = []
    index = 0
    for name, lines in groups:
        n = sum(1 for op in lines if (op['op'] == 'cmd' and op.get('record', True)) or (op['op'] == 'tick' and op.get('record')))
        commands_by_group.append((name, index, index + n))
        index += n
    mismatches, known, fixed = [], [], set(KNOWN)
    for v, r in zip(vanilla, rust):
        assert v['cmd'] == r['cmd'], (v['cmd'], r['cmd'])
        kind = compare(v, r)
        if kind:
            (known if v['cmd'] in KNOWN else mismatches).append({'kind': kind, 'cmd': v['cmd'], 'vanilla': v, 'rust': r})
            fixed.discard(v['cmd'])
    (build / 'mismatches.jsonl').write_text(''.join(json.dumps(m, ensure_ascii=False) + '\n' for m in mismatches + known),
                                            encoding='utf-8')
    summary = []
    bad = {m['cmd'] for m in mismatches + known}
    for name, lo, hi in commands_by_group:
        failed = sum(1 for rec in vanilla[lo:hi] if rec['cmd'] in bad)
        summary.append({'group': name, 'commands': hi - lo, 'mismatches': failed})
        print(f'{name}: {hi - lo - failed}/{hi - lo} commands match', flush=True)
    for m in mismatches[:args.limit]:
        print(m['kind'], repr(m['cmd']))
        print('  vanilla  ', render(m['vanilla'])[:400])
        print('  ferraloom', render(m['rust'])[:400])
    for m in known:
        print(f'known: {m["cmd"]!r} ({KNOWN[m["cmd"]]})')
    for command in sorted(fixed & {rec['cmd'] for rec in vanilla}):
        print(f'matches now, leave KNOWN: {command!r}')
    total = len(vanilla)
    print(f'total: {total - len(mismatches) - len(known)}/{total} commands match ({len(known)} known)', flush=True)
    report = {'schema': 2, 'minecraft': lock['minecraft']['version'], 'platform': platform.platform(), 'seed': SEED,
              'commands': total, 'mismatches': len(mismatches), 'known': len(known), 'groups': summary,
              'scope': 'Feedback and failure messages (text and resolved style), chat lines, and the last '
                       'CommandResultCallback result of commands an operator runs on a dedicated server; world, '
                       'entity and item effects through probe commands.'}
    lab.write_json(report_path, report)
    if mismatches and not args.allow_gaps:
        sys.exit(f'{len(mismatches)} command execution mismatches; see {build}')
    print(f'Command execution parity report: {report_path}')


MASKS = [
    (re.compile(r'debug-trace-\d{4}-\d\d-\d\d_\d\d\.\d\d\.\d\d\.txt'), 'debug-trace-<time>.txt'),
    (re.compile(r'after [0-9.]+ second\(s\) and \d+ tick\(s\) \([0-9.,NaInfity]+ tick\(s\) per second\)'), 'after <time>'),
    (re.compile(r'Average time per tick: [0-9.]+ms'), 'Average time per tick: <ms>'),
    (re.compile(r'P50: [0-9.]+ms P95: [0-9.]+ms P99: [0-9.]+ms'), 'P50: <ms> P95: <ms> P99: <ms>'),
    (re.compile(r'debug/(client|server)-[0-9_.-]+\.(jfr|json)'), r'debug/\1-<time>'),
]


GAMERULES = re.compile(r'/gamerule \(([^)]*)\)')


def masked(record):
    text = json.dumps(record['messages'], ensure_ascii=False)
    # GameRuleCommand registers the rules in identity-hash order, different
    # in every JVM: compare /help's gamerule alternatives as a set.
    text = GAMERULES.sub(lambda m: '/gamerule (' + '|'.join(sorted(m.group(1).split('|'))) + ')', text)
    for pattern, replacement in MASKS:
        text = pattern.sub(replacement, text)
    return text


def compare(v, r):
    if masked(v) != masked(r):
        return 'messages'
    if v['results'] != r['results']:
        # The ticks the profiler ran, which depend on how long it ran.
        if v['cmd'].startswith('/debug stop'):
            return None
        return 'results'
    return None


def render(record):
    lines = [''.join(span[0] for span in m) for m in record['messages']]
    return json.dumps({'messages': lines, 'styles': [[span[1] for span in m] for m in record['messages']], 'results': record['results']}, ensure_ascii=False)


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        sys.exit(f'{NAME}: {error}')
