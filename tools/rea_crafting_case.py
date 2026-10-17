#!/usr/bin/env python3
"""One real table_offset_right boot for a later REA process capture.

ROOT supplies a seeded observation file, fresh artifact directory and admitted
build/native inputs. This process neither invokes REA nor compares engines.
The existing Probe uses zero changed-slot predictions, including SWAP. It also
resets after retaining the extracted cursor/remainder; stopped continuation is
therefore RESET/empty plus sugar crafted-stat2 and world state. Unrelated main
and offhand slots are EMPTY; this case has no nonempty preservation sentinel.
Both engines require the explicit buffered-clean-stop reference I/O profile.
Only Java disables per-write sync; observations cover clean save/stop readback,
with no default-sync or crash-durability parity claim.
"""
from __future__ import annotations

import argparse
from copy import deepcopy
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import stat
import subprocess
import sys

sys.dont_write_bytecode = True
import check_crafting as craft
from check_jukeboxes import stopped_time
from parity import worlds

CASE = 'table_offset_right'
OWNED = ('tools/rea_crafting_case.py', 'tests/test_rea_crafting_case.py')
CLIENT_PATHS = ('tools/parity/crafting.ts', 'tests/protocol_client.test.ts')
NATIVE_HELPER = '.cache/reviews/r29-native-validation-prepare/run-review29-integrated-all-sixteen.py'
NATIVE_HELPER_SHA256 = '769cddec92e87f39a133ad9e1c55fa65ebd97b8c204269adc5dc559f4b1b2c1c'
NATIVE_STEPS = ('fmt-root', 'fmt-vendor', 'python', 'vendor-clippy', 'data', 'protocol',
                'command', 'world', 'inventory', 'core', 'gametest-compile', 'clippy',
                'wrapper', 'node-protocol', 'typescript', 'build')
REFERENCE_IO_SCOPE = ('Java only: sync-chunk-writes=false disables per-write sync. '
                      'Clean save/stop readback only; no default-sync or crash-durability parity. '
                      'Rust configuration is unchanged.')
NEUTRALIZATION = {
    'item_ids': 'Resolve received registry IDs through the pinned official item report; retain counts and reject components.',
    'menu_ids_revisions_sequences': 'Retain raw IDs; project menu kind, all slots, action order and validated later same-stream fences.',
    'world_time': 'Retain absolute times; project each frozen interval and stopped continuation delta.',
    'chunk_status': 'Remove the optional minecraft: prefix from the observed full status.',
    'process_identity': 'Retain engine, host, paths, argv, PIDs, timestamps, REA token and hashes in the per-engine manifest; project clean owned exits.',
    'other_saved_fields': 'Retain complete typed NBT/stats; project inventory, selected slot, UUID, position, rotation, dimension, game mode and crafted counters only.',
    'other_world_fields': 'Retain level/terrain bytes and typed NBT; project versions, chunk identity, table/floor states and frozen continuation only.',
    'other_statistics': 'Retain complete AwardStats and saved stats; only the three existing crafting counters are in this case contract.',
}


def canonical(value: object) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False) + '\n').encode()


def retain(path: Path, value: object) -> None:
    """Exclusive, read-only final artifact; never replace a prior result."""
    with path.open('xb') as output:
        output.write(value if isinstance(value, bytes) else canonical(value))
    path.chmod(0o444)


def fence_projection(fence: dict, after: int) -> dict:
    craft.validate_fence(fence, after)
    sent, response = fence['request'], fence['response']
    entries = response['entries']
    craft.require(isinstance(entries, list) and len(entries) <= 65536, 'Missing decoded AwardStats entries')
    values = [(row['category_id'], row['statistic_id'], row['value']) for row in entries]
    craft.require(all(type(value) is int and 0 <= value <= 2147483647 for row in values for value in row)
                  and len({row[:2] for row in values}) == len(values), 'Invalid/duplicate statistic observation')
    # Re-encode the existing client's decoded map to bind names/values to raw
    # bytes. This is a case-local check, not a replacement statistics decoder.
    encoded = b'\x03' + craft.protocol.varint(len(entries)) + b''.join(
        craft.protocol.varint(value) for row in values for value in row)
    craft.require(craft.payload(response) == encoded, 'Decoded statistics disagree with retained raw packet')
    return {'request_packet': sent['packet_id'], 'action_id': sent['action_id'],
            'response_packet': response['packet_id'], 'written': sent['written'],
            'request_after_action': sent['after_sequence'] >= after,
            'response_after_request': response['sequence'] > sent['after_sequence']}


def saved_projection(saved: dict) -> dict:
    typed, statistics = saved['player']['typed'], saved['statistics']['data']
    main, offhand = craft.player_items(typed)
    fields = ('DataVersion', 'SelectedItemSlot', 'UUID', 'Pos', 'Rotation', 'Dimension', 'playerGameType')
    craft.require(all(key in typed for key in fields), 'Missing authoritative player continuation fields')
    craft.require(typed['DataVersion'] == craft.atom('Int', 5023) and statistics['DataVersion'] == 5023,
                  'Wrong saved player/statistics DataVersion')
    return {'main': main, 'offhand': offhand, 'player': {key: typed[key] for key in fields},
            'statistics_data_version': statistics['DataVersion'], 'crafted': craft.crafted_counts(saved['statistics'])}


def snapshot_projection(snapshot: dict, items: dict) -> dict:
    """Extract actual received/saved facts, refusing omissions and opaque stacks."""
    received = snapshot['fence']['received']
    actual = craft.view(received, items)
    saved = saved_projection(snapshot['saved'])
    totals, identities = craft.validate_drop_entities(snapshot['drops'])
    craft.require(actual == snapshot['actual'] and saved['main'] == actual['main']
                  and saved['offhand'] == actual['offhand'], 'Changed received/saved checkpoint')
    craft.require(saved['crafted'] == snapshot['crafted_counts'] == snapshot['wire_counts'],
                  'Missing/disagreeing saved and observed crafted counters')
    craft.require(totals == snapshot['drop_totals'] and not identities and not totals,
                  'This case requires observed zero drops')
    predicate = snapshot['live_count']['predicate']
    craft.require(predicate == f'if score CraftDrops pdprobe matches {len(identities)}'
                  and snapshot['live_count']['commands_log'] and 'presence' in snapshot
                  and 'live_items' in snapshot and 'live_count_command' in snapshot,
                  'Missing actual live drop census')
    craft.require(not any('CRAFT_DROPS_' in line and '[Server]' in line for line in snapshot['presence']),
                  'Unexpected live drop presence')
    craft.require(snapshot['time_before'] == snapshot['time_after'], 'Frozen checkpoint advanced')
    menus = {name: [craft.wire_stack(value, items) for value in received[name]['slots']]
             if received.get(name) is not None else None for name in ('player', 'table')}
    return {'label': snapshot['label'], 'active': received['active'], 'menus': menus, 'view': actual,
            'saved': saved, 'drops': totals, 'live_drop_count': len(identities), 'drop_predicate': predicate,
            'frozen_elapsed': snapshot['time_after'] - snapshot['time_before'],
            'fence': fence_projection(snapshot['fence'], snapshot['fence']['request']['after_sequence'])}


def lifecycle_projection(entry: dict) -> dict:
    stop, clients = entry['teardown'], entry['clients']
    craft.require(stop.get('boot') == entry['role'] and type(stop.get('pid')) is int and stop['pid'] > 0
                  and stop.get('argv') and stop.get('started') and stop.get('stopped')
                  and stop['started'] < stop['stopped'] and stop.get('exit_code') == 0
                  and stop.get('forced_stop') is False and not stop.get('unexpected_exit')
                  and entry.get('cleanup_errors') == [] and entry.get('watchdogs') == [], 'Unclean owned server exit')
    closed = stop.get('clients', [])
    craft.require(len(clients) == len(closed) == 1 and clients[0].get('name') == craft.NAME
                  and '--crafting' in clients[0].get('argv', []) and '--raw-packets' in clients[0]['argv']
                  and type(closed[0].get('pid')) is int and closed[0]['pid'] == clients[0].get('pid')
                  and closed[0].get('exit_code') == 0 and closed[0].get('forced_close') is False
                  and closed[0].get('failure') is None and closed[0].get('expected_disconnect') is None,
                  'Unclean owned client exit')
    return {'server_exit': stop['exit_code'], 'server_forced': stop['forced_stop'],
            'client_exit': closed[0]['exit_code'], 'client_forced': closed[0]['forced_close']}


def project(entry: dict, contract: dict) -> dict:
    """Validate this one case, never the full campaign or a passed:true proxy."""
    case = entry['cases'][CASE]
    craft.require(list(entry['cases']) == [CASE] and case['name'] == CASE, 'Exactly one declared case required')
    craft.validate_case(case)
    craft.require(case['timeline'][-1] == 'cleanup_complete', 'Missing existing case reset')
    snapshots = case['snapshots']
    craft.require([value['label'] for value in snapshots] == ['baseline', 'prepared', 'extracted', 'evacuated', 'before_stop'],
                  'Missing/reordered retained checkpoint')
    checkpoints = [snapshot_projection(value, contract['items']) for value in snapshots]
    for check, checkpoint in zip(case['checks'], checkpoints[1:3], strict=True):
        craft.require(check['actual'] == checkpoint['view'] and check['drops'] == checkpoint['drops']
                      and check['after_stats'] == checkpoint['saved']['crafted']
                      and check['before_stats'] == checkpoints[0]['saved']['crafted'], 'Check differs from actual snapshot')
    baseline = checkpoints[0]['saved']['crafted']
    for checkpoint in checkpoints:
        checkpoint['crafted_delta'] = {name: checkpoint['saved']['crafted'][name] - baseline[name] for name in craft.CRAFTED}
    clicks = [phase for phase in case['phases'] if phase['kind'] == 'click']
    craft.require({row['pid'] for phase in case['phases'] for row in phase.get('wire', [])}
                  == {entry['clients'][0]['pid']}, 'Raw phases are not from the owned client')
    actions = [{'purpose': phase['purpose'], 'slot': phase['request']['slot'], 'input': phase['request']['input'],
                'button': phase['request']['button'], 'changed_slots': phase['request']['changed_slots'],
                'before': craft.view(phase['request']['before'], contract['items']),
                'after': craft.view(phase['received'], contract['items']),
                'fence': fence_projection(phase['fence'], phase['request']['after_sequence'])} for phase in clicks]
    craft.require(actions[-1]['after'] == checkpoints[2]['view'] and actions[0]['after'] == checkpoints[1]['view'],
                  'Fenced immediate click result differs from retained checkpoint')
    fences = [phase['fence'] for phase in case['phases'] if 'fence' in phase] + [value['fence'] for value in snapshots]
    named = {identifier: name for name, identifier in contract['items'].items() if name in craft.CRAFTED}
    observed = set()
    for fence in fences:
        fence_projection(fence, fence['request']['after_sequence'])
        observed.update((named[row['statistic_id']], row['value']) for row in fence['response']['entries']
                        if row['category_id'] == contract['crafted_category'] and row['statistic_id'] in named)
    craft.require(('sugar', checkpoints[2]['saved']['crafted']['sugar']) in observed,
                  'Missing actual named sugar AwardStats observation')
    opened = next(phase for phase in case['phases'] if phase['kind'] == 'table_open')
    uses = [row for row in opened['wire'] if row['direction'] == 'serverbound' and row['packet_id'] == 66]
    craft.require(len(uses) == 1 and opened['request'].get('sent_action') == 'use_item_on'
                  and opened['request'].get('written') is True, 'Missing actual table use')
    stopped = saved_projection(entry['saved'])
    craft.require(stopped == checkpoints[-1]['saved'], 'Stopped player/statistics continuation differs')
    world = entry['world']
    craft.require(world['level_data_version'] == craft.atom('Int', 5023) and world['chunk_data_version'] == 5023
                  and world['chunk'] == [256, 0] and world['status'].removeprefix('minecraft:') == 'full'
                  and world['table'] == {'name': 'minecraft:crafting_table', 'properties': {}}
                  and world['floor'] == {'name': 'minecraft:stone', 'properties': {}}
                  and world['world_time'] == snapshots[-1]['time_after'], 'Wrong stopped world continuation')
    craft.require(entry['table_predicate']['predicate'] == 'if block 4104 200 6 minecraft:crafting_table',
                  'Missing authoritative live table predicate')
    return {'schema': 'rea-crafting-case/1', 'case': case['name'], 'protocol': entry['status']['protocol'],
            'neutralization': NEUTRALIZATION, 'checkpoints': checkpoints, 'actions': actions,
            'observed_crafted_values': [{'item': name, 'absolute': value} for name, value in sorted(observed)],
            'table_open': {'request': opened['request']['sent_action'], 'payload_hex': craft.payload(uses[0]).hex(),
                           'menu_type': next(row['menu_type'] for row in opened['packets'] if row['kind'] == 'open'),
                           'fence': fence_projection(opened['fence'], opened['after_sequence'])},
            'live_table_predicate': entry['table_predicate']['predicate'], 'stopped': stopped,
            'world': {**{key: value for key, value in world.items() if key not in ('world_time', 'status')},
                      'status': world['status'].removeprefix('minecraft:'),
                      'continuation_elapsed': world['world_time'] - snapshots[-1]['time_after']},
            'lifecycle': lifecycle_projection(entry)}


def buffered_reference_configuration(engine: str, raw: bytes, *, started: bool = False) -> bytes:
    """Admit plain generated Java properties; leave Rust configuration untouched."""
    craft.require(engine in ('java', 'rust'), 'Unknown reference profile engine')
    if engine == 'rust':
        return raw
    values = []
    for line in re.split(r'\r\n|\r|\n', raw.decode('ascii')):
        line = line.lstrip(' \t\f')
        if not line or line.startswith(('#', '!')):
            continue
        # Refuse escaped keys, continuation lines and alternate separators.
        match = re.fullmatch(r'([a-z0-9.-]+)=(.*)', line)
        craft.require(match is not None and (len(line) - len(line.rstrip('\\'))) % 2 == 0,
                      'Ambiguous Java reference property')
        if match[1] == 'sync-chunk-writes':
            values.append(match[2])
    if started:
        craft.require(values == ['false'], 'Java reference requires uniquely false sync-chunk-writes')
        return raw
    craft.require(not values and raw.endswith(b'\n'), 'Fresh Java sync-chunk-writes must be absent and unambiguous')
    return raw + b'sync-chunk-writes=false\n'


def run_case(args: argparse.Namespace, report: dict) -> dict:
    kind = 'vanilla' if args.engine == 'java' else 'rust'
    role = args.engine + '_fresh'
    report.update(active_boot=role, boots={}, processes={})
    fixture = craft.Fixture(kind, args.artifacts / role, args, report)
    server = fixture.server
    entry = report['boots'][role] = {'role': role, 'engine': args.engine, 'cases': {}, 'clients': []}
    probe = None
    try:
        server.configure()
        configuration = server.directory / ('server.properties' if args.engine == 'java' else 'pumpkin.toml')
        initial = buffered_reference_configuration(args.engine, craft.read_bytes(configuration))
        if args.engine == 'java':
            configuration.write_bytes(initial)
        retain(configuration.with_name(configuration.name + '.initial'), initial)
        fixture.start(role)
        after_start = craft.read_bytes(configuration)
        retain(configuration.with_name(configuration.name + '.after-start'), after_start)
        entry['reference_configuration'] = {'file': configuration.name,
            'initial_sha256': hashlib.sha256(initial).hexdigest(),
            'after_start_sha256': hashlib.sha256(after_start).hexdigest()}
        craft.require(fixture.processes[-1]['configuration'][configuration.name]
                      == {'sha256': entry['reference_configuration']['initial_sha256'], 'text': initial.decode('ascii')},
                      'Reference profile differs from Fixture captured initial configuration')
        buffered_reference_configuration(args.engine, after_start, started=True)
        entry['status'] = craft.protocol.status_fields(server.port)
        craft.write_json(args.artifacts / 'progress.json', report)
        craft.require(entry['status']['protocol'] == 777, 'Wrong live protocol')
        with craft.interaction_deadline(server, 'rea:fixture'):
            server.command('tick freeze')
            craft.require_frozen(server)
            server.command('forceload add 4096 0')
        craft.suite.wait_loaded(server, x=4096, z=0)
        with craft.interaction_deadline(server, 'rea:fixture_blocks'):
            for command in ('fill 4096 199 0 4111 199 15 stone', 'difficulty peaceful',
                            'scoreboard objectives add pdprobe dummy', 'setblock 4104 200 6 crafting_table'):
                server.command(command)
        client = craft.Client(server, log_name='crafting-wire', crafting=True, raw_packets=True)
        fixture.clients.append(client)
        entry['clients'].append({'name': client.name, 'pid': client.process.pid, 'argv': client.process.args})
        client.ready()
        with craft.interaction_deadline(server, 'rea:admission'):
            client.wait_state(lambda state: state['crafting']['player'] is not None)
            server.command(f'gamemode survival {craft.NAME}')
            entry['teleport'] = craft.teleport_exact(server, client, craft.POSITION)
            entry['identity'] = craft.typed_predicate(server,
                f'if entity @a[name={craft.NAME},nbt={{UUID:[I;{",".join(map(str, craft.UUID_INTS))}],playerGameType:0,SelectedItemSlot:0}}]')
            client.wait_state(lambda state: any(row['x'] == 256 and row['z'] == 0 for row in state['chunk_packets']))
        probe = craft.Probe(fixture, client, report['contract'])
        probe.exercise(CASE)
        entry['before_stop'] = probe.capture('before_stop')
        with craft.interaction_deadline(server, 'rea:table_continuation'):
            entry['table_predicate'] = craft.typed_predicate(server, 'if block 4104 200 6 minecraft:crafting_table')
        fixture.close(client)
        server.save_flush()
        fixture.stop()
        world, stem = server.directory / 'world', server.directory / 'stopped'
        entry['saved'] = craft.retained_player(world, stem)
        entry['level'] = stopped_time(world, stem)
        entry['terrain'] = craft.read_fixture(world, stem.with_suffix('.terrain.nbt'))
        # Decode the already-retained chunk with the existing terrain codec.
        chunk = worlds.decode_nbt(Path(entry['terrain']['raw_nbt']).read_bytes())
        states = worlds.sample_states(chunk, (12,))
        table, floor = states[(8 * 16 + 6) * 16 + 8], states[(7 * 16 + 6) * 16 + 8]
        level = json.loads(stem.with_suffix('.level.typed.json').read_text())['typed']['Data']
        entry['world'] = {'world_time': entry['level']['world_time'], 'level_data_version': level['DataVersion'],
                          'chunk_data_version': chunk['DataVersion'], 'chunk': [chunk['xPos'], chunk['zPos']],
                          'status': chunk['Status'], 'table': {'name': table[0], 'properties': dict(table[1])},
                          'floor': {'name': floor[0], 'properties': dict(floor[1])}}
    except Exception as error:
        entry['error'] = f'{type(error).__name__}: {error}'
        if probe is not None and probe.case and not server.broken and not probe.client.failure:
            try:
                entry['failure_snapshot'] = probe.capture('failure')
            except Exception as capture_error:
                entry['failure_capture_error'] = f'{type(capture_error).__name__}: {capture_error}'
        raise
    finally:
        entry['cleanup_errors'] = fixture.cleanup()
        entry['watchdogs'] = deepcopy(server.abort_events)
        if fixture.processes:
            entry['teardown'] = deepcopy(fixture.processes[-1])
        craft.write_json(args.artifacts / 'progress.json', report)
    return entry


def git(*arguments: str) -> str:
    return subprocess.check_output(['git', '-C', str(craft.oracle.ROOT), *arguments], text=True).strip()


def source_boundaries(engine: str, client: str, harness: str) -> dict:
    """Observe the direct E -> F -> H chain, with exactly two paths per boundary."""
    craft.require(all(re.fullmatch('[0-9a-f]{40}', value) for value in (engine, client, harness)),
                  'Exact engine/client/harness source commits required')
    bridge_paths = git('diff', '--no-renames', '--name-only', client, harness).splitlines()
    bridge_status = git('diff', '--no-renames', '--name-status', client, harness).splitlines()
    craft.require(bridge_paths == sorted(OWNED)
                  and bridge_status == ['A\t' + path for path in sorted(OWNED)],
                  'F -> H must add exactly the two bridge paths')
    observed_harness = git('rev-list', '--parents', '-n', '1', harness).split()
    observed_client = git('rev-list', '--parents', '-n', '1', client).split()
    craft.require(observed_harness == [harness, client] and observed_client == [client, engine],
                  'Requires observed direct single-parent E -> F -> H ancestry')
    git('merge-base', '--is-ancestor', engine, harness)
    client_status = git('diff', '--no-renames', '--name-status', engine, client).splitlines()
    craft.require(client_status == ['M\t' + path for path in sorted(CLIENT_PATHS)],
                  'E -> F must modify exactly the two crafting client paths')
    full_status = git('diff', '--no-renames', '--name-status', engine, harness).splitlines()
    expected = dict.fromkeys(CLIENT_PATHS, 'M') | dict.fromkeys(OWNED, 'A')
    craft.require(full_status == [status + '\t' + path for path, status in sorted(expected.items())],
                  'E -> H must contain exactly the four declared paths and statuses')
    boundaries = {}
    for name, before, after, paths, statuses in (
            ('engine_to_client', engine, observed_client[0], CLIENT_PATHS, client_status),
            ('client_to_harness', observed_client[0], observed_harness[0], OWNED, bridge_status),
            ('engine_to_harness', engine, observed_harness[0], tuple(expected), full_status)):
        outside = git('diff', '--no-renames', '--name-status', before, after, '--', '.',
                      *(':(exclude)' + path for path in paths)).splitlines()
        craft.require(outside == [], 'Engine/build-input diff outside exact declared boundary paths must be empty')
        boundaries[name] = {'from_commit': before, 'to_commit': after,
                            'name_status': statuses, 'outside_declared_paths': outside}
    return boundaries


def validate_build_association(build_raw: bytes, native_raw: bytes, *, build_name: str,
                               native_name: str, source: str, binary_sha256: str) -> None:
    """Associate retained ROOT-admitted files, without revalidating raw test logs/counts.

    ROOT separately validates the full native pass. These are file-based links,
    not embedded or cryptographic binary/source attestation. Never reread live
    receipt paths here: the parsed bytes are the ones retained and hash-pinned.
    """
    build = json.loads(build_raw, object_pairs_hook=craft.unique_object)
    native = json.loads(native_raw, object_pairs_hook=craft.unique_object)
    craft.require(isinstance(build, dict) and isinstance(native, dict), 'Missing native/build receipt object')
    craft.require(re.fullmatch(r'\d{8}T\d{6}Z-[0-9a-f]{8}\.json', build_name) is not None,
                  'Requires canonical builder job filename')
    match = re.fullmatch(r'(review29-all16-builder-[a-z0-9-]+)-native\.json', native_name)
    craft.require(match is not None, 'Requires original all-sixteen native receipt filename')
    prefix = match[1]
    command = ['timeout', '--signal=TERM', '--kill-after=30s', '7200', 'python3', '-B',
               NATIVE_HELPER, source, prefix]
    root = Path('/home/doc/code/Ferraloom')
    craft.require(build.get('host') == 'ferraloom-builder' and build.get('source_commit') == source
                  and build.get('cwd') == str(root) and build.get('command') == command,
                  'Builder host/source/cwd/exact native helper command differs')
    craft.require(type(build.get('exit_code')) is int and build['exit_code'] == 0
                  and build.get('source_status_after') == ''
                  and build.get('resource_limits') == {'memory': '6G', 'swap': '0', 'cpu': '200%'},
                  'Builder helper job must finish cleanly under the admitted limits')
    craft.require(native.get('host') == build['host'] == 'ferraloom-builder'
                  and native.get('helper_sha256') == NATIVE_HELPER_SHA256,
                  'Native receipt host/helper hash differs from the exact admitted helper job')
    craft.require(native.get('source_commit') == source and native.get('binary_sha256') == binary_sha256
                  and re.fullmatch('[0-9a-f]{64}', binary_sha256) is not None,
                  'Native source/completed-build hash differs from the supplied binary')
    craft.require(native.get('full_source_clean') is True
                  and native.get('build_environment') == {'CARGO_INCREMENTAL': '0', 'CARGO_BUILD_JOBS': '1'}
                  and native.get('resource_limits') == {'memory.max': str(6 * 1024**3),
                                                       'memory.swap.max': '0', 'cpu.max': '200000 100000'},
                  'Native helper must finish with clean source and admitted environment/limits')
    group = native.get('cgroup')
    craft.require(isinstance(group, str) and group.startswith('/') and '..' not in Path(group).parts
                  and Path(group).name == f'ferraloom-build-{Path(build_name).stem}.scope',
                  'Native cgroup belongs to another builder job')
    steps = native.get('steps')
    craft.require(isinstance(steps, list) and all(isinstance(row, dict) for row in steps)
                  and [row.get('step') for row in steps] == list(NATIVE_STEPS),
                  'Native helper must retain all sixteen steps in order')
    craft.require(all(type(row.get('exit_code')) is int and row['exit_code'] == 0
                      and row.get('execution_gate_passed') is True for row in steps),
                  'Native helper has incomplete/failed execution gates')
    craft.require(steps[-1].get('command') == ['cargo', 'build', '--locked', '--offline', '--jobs', '1'],
                  'Native helper must end with the actual locked/offline binary build')
    craft.require(all(row.get('log') == str(root / '.cache' / f'{prefix}-{row["step"]}.log') for row in steps),
                  'Native step logs belong to another helper attempt')


def pin_inputs(args: argparse.Namespace, report: dict) -> dict[str, str]:
    """Hash actual inputs and associate ROOT's admitted receipts with this binary."""
    report['source'] = craft.suite.git_state()
    report['engine_source_commit'] = args.rust_source_commit
    report['manifest'] = {**craft.manifest(),
                          **{path: craft.digest(craft.oracle.ROOT / path) for path in (*CLIENT_PATHS, *OWNED)}}
    paths = {'python': Path(sys.executable).resolve(), 'client_node': Path(shutil.which('node') or 'node').resolve(),
             'java': Path(shutil.which(args.java) or args.java).resolve(), 'rust_binary_original': args.rust_binary.resolve(),
             'reference_outer': args.reference_cache / 'server.jar', 'reference_inner': args.reference_cache / 'minecraft.jar',
             'build_receipt': args.build_receipt, 'native_receipt': args.native_receipt,
             'native_helper': craft.oracle.ROOT / NATIVE_HELPER}
    pins = {name: craft.digest(path) for name, path in paths.items()}
    report['inputs'] = {name: {'path': str(path), 'sha256': pins[name]} for name, path in paths.items()}
    receipts = {name: craft.read_bytes(paths[name]) for name in ('build_receipt', 'native_receipt')}
    for name, raw in receipts.items():
        retain(args.artifacts / (name + '.original'), raw)
    report['native_receipt_scope'] = 'ROOT-admitted hash pins plus file-based native/build/binary association; ROOT validates full counts/raw logs separately. No tests re-executed here.'
    craft.write_json(args.artifacts / 'progress.json', report)
    craft.require(report['source'] == {'commit': args.current_clean_commit, 'dirty': False}, 'Exact clean harness source required')
    report['source_boundaries'] = source_boundaries(args.rust_source_commit, args.client_source_commit,
                                                   args.current_clean_commit)
    report['client_source_commit'] = report['source_boundaries']['engine_to_client']['to_commit']
    report['engine_to_harness_changed_paths'] = [row.split('\t')[1]
        for row in report['source_boundaries']['engine_to_harness']['name_status']]
    craft.write_json(args.artifacts / 'progress.json', report)
    craft.require(pins['build_receipt'] == args.build_receipt_sha256 and pins['native_receipt'] == args.native_receipt_sha256,
                  'Changed ROOT-admitted receipts')
    craft.require(all(hashlib.sha256(raw).hexdigest() == pins[name]
                      and craft.digest(args.artifacts / (name + '.original')) == pins[name]
                      for name, raw in receipts.items()), 'Receipt changed during retention')
    craft.require(pins['native_helper'] == NATIVE_HELPER_SHA256, 'Changed ROOT-reviewed native helper')
    validate_build_association(receipts['build_receipt'], receipts['native_receipt'],
                               build_name=paths['build_receipt'].name, native_name=paths['native_receipt'].name,
                               source=args.rust_source_commit, binary_sha256=pins['rust_binary_original'])
    craft.require(pins['rust_binary_original'] == args.rust_binary_sha256
                  and os.access(paths['rust_binary_original'], os.X_OK), 'Changed/nonexecutable admitted Rust binary')
    craft.require(pins['java'] == craft.JDK_SHA256 and pins['reference_inner'] == craft.INNER_JAR_SHA256,
                  'Changed pinned reference/JDK')
    args.java = str(paths['java'])
    craft.oracle.CACHE = args.reference_cache
    # Both existing server acquisition entry points become cached-only.
    craft.oracle.acquire = craft.suite.acquire = craft.cached_reference
    craft.cached_reference()
    report['contract'] = craft.primary_contract(args)
    args.rust_binary = craft.suite.pin_binary(paths['rust_binary_original'], args.artifacts)
    craft.require(craft.digest(args.rust_binary) == pins['rust_binary_original'], 'Changed pinned executable copy')
    report['inputs']['rust_binary_pinned'] = {'path': str(args.rust_binary), 'sha256': pins['rust_binary_original']}
    report['binary_source_association'] = 'ROOT-admitted binary/native/build/helper pins with matched E, native completed-build hash and exact successful builder helper job; no embedded or cryptographic source attestation.'
    return {str(path): pins[name] for name, path in paths.items()} | {str(args.rust_binary): pins['rust_binary_original']}


def raw_manifest(directory: Path) -> None:
    files = {}
    total = 0
    for path in sorted(directory.rglob('*')):
        craft.require(not path.is_symlink(), 'Linked artifact refused')
        if path.is_file():
            metadata = path.stat()
            total += metadata.st_size
            craft.require(len(files) < 8192 and total <= 1024**3, 'Artifact manifest budget exceeded')
            files[str(path.relative_to(directory))] = {'size': metadata.st_size, 'mode': stat.S_IMODE(metadata.st_mode),
                                                     'sha256': craft.digest(path)}
    retain(directory / 'manifest.json', {'schema': 'rea-crafting-raw/1', 'files': files})


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--engine', choices=('java', 'rust'), required=True)
    parser.add_argument('--reference-io-profile', choices=('buffered-clean-stop',), required=True,
                        help=REFERENCE_IO_SCOPE)
    for name in ('root', 'rust-binary', 'reference-cache', 'primary-root', 'build-receipt', 'native-receipt', 'artifacts', 'outcome'):
        parser.add_argument('--' + name, type=Path, required=True)
    for name in ('current-clean-commit', 'rust-source-commit', 'client-source-commit', 'rust-binary-sha256', 'build-receipt-sha256', 'native-receipt-sha256', 'java'):
        parser.add_argument('--' + name, required=True)
    parser.add_argument('--accept-eula', action='store_true')
    parser.add_argument('--workers', type=int, choices=(2,), default=2)
    parser.add_argument('--startup-timeout', type=float, choices=(180,), default=180)
    args = parser.parse_args()
    craft.require(args.accept_eula and args.root.resolve() == craft.oracle.ROOT.resolve(), 'Exact checkout and explicit EULA acceptance required')
    for name in ('current_clean_commit', 'rust_source_commit', 'client_source_commit'):
        craft.require(re.fullmatch('[0-9a-f]{40}', getattr(args, name)), 'Exact source commit required')
    for name in ('rust_binary_sha256', 'build_receipt_sha256', 'native_receipt_sha256'):
        craft.require(re.fullmatch('[0-9a-f]{64}', getattr(args, name)), 'Exact ROOT-admitted SHA256 required')
    cache = (args.root / '.cache').resolve()
    for name in ('artifacts', 'outcome'):
        path = getattr(args, name).absolute()
        craft.require(path.resolve().is_relative_to(cache) and not any(p.is_symlink() for p in (path, *path.parents)),
                      'Use unlinked ignored .cache paths')
        setattr(args, name, path.resolve())
    craft.require(not args.outcome.is_relative_to(args.artifacts)
                  and not args.artifacts.is_relative_to(args.outcome.parent),
                  'Fresh artifacts must be outside the shared scratch observation directory')
    args.reference_cache = args.reference_cache.resolve()
    args.artifacts.mkdir(parents=True, exist_ok=False)
    report = {'schema': 'rea-crafting-raw/1', 'result': 'failed', 'created': craft.now(), 'engine': args.engine,
              'reference_io_profile': {'name': args.reference_io_profile, 'scope': REFERENCE_IO_SCOPE},
              'host': socket.gethostname(), 'checkout': str(craft.oracle.ROOT), 'argv': sys.argv,
              'environment': {key: os.environ.get(key) for key in ('REA_PROCESS_RUN_ID', 'PATH', 'JAVA_HOME', 'CARGO_INCREMENTAL', 'CARGO_BUILD_JOBS')},
              'bounds': {'startup_seconds': 180, 'interaction_seconds': 20, 'workers': 2},
              'scope': __doc__, 'neutralization': NEUTRALIZATION}
    projection = None
    try:
        metadata = args.outcome.stat()
        craft.require(stat.S_ISREG(metadata.st_mode), 'ROOT must seed a regular observation file')
        before = craft.read_bytes(args.outcome)
        before_mode = stat.S_IMODE(metadata.st_mode)
        retain(args.artifacts / 'outcome-before.bin', before)
        report['scratch_before'] = {'path': str(args.outcome), 'mode': before_mode, 'size': len(before),
                                    'sha256': hashlib.sha256(before).hexdigest()}
        craft.write_json(args.artifacts / 'progress.json', report)
        pins = pin_inputs(args, report)
        entry = run_case(args, report)
        # Retain all raw/typed facts and child receipts before bridge assertions.
        craft.write_json(args.artifacts / 'progress.json', report)
        projection = canonical(project(entry, report['contract']))
        craft.require(craft.suite.git_state() == report['source'] and
                      {**craft.manifest(), **{path: craft.digest(craft.oracle.ROOT / path)
                                             for path in (*CLIENT_PATHS, *OWNED)}} == report['manifest'],
                      'Harness source changed during boot')
        craft.require(all(craft.digest(Path(path)) == expected for path, expected in pins.items())
                      and craft.primary_contract(args) == report['contract'], 'Pinned inputs changed during boot')
        craft.require(craft.read_bytes(args.outcome) == before and
                      stat.S_IMODE(args.outcome.stat().st_mode) == before_mode, 'Scratch observation changed during boot')
        retain(args.artifacts / 'outcome.json', projection)
        report['result'] = 'completed'
        report['outcome_sha256'] = hashlib.sha256(projection).hexdigest()
    except Exception as error:
        projection = None
        report['error'] = f'{type(error).__name__}: {error}'
    report['finished'] = craft.now()
    retain(args.artifacts / 'report.json', report)
    try:
        raw_manifest(args.artifacts)
        if projection is None:
            print(f"rea-crafting-case failed; retained {args.artifacts}", file=sys.stderr, flush=True)
            return 1
        # ROOT owns the seeded file contract. Preserve its mode and refuse a race.
        with os.fdopen(os.open(args.outcome, os.O_RDWR | os.O_NOFOLLOW), 'r+b') as output:
            craft.require(stat.S_IMODE(os.fstat(output.fileno()).st_mode) == before_mode and output.read() == before,
                          'Scratch observation changed during boot')
            output.seek(0)
            output.write(projection)
            output.truncate()
            output.flush()
            os.fsync(output.fileno())
    except Exception as error:
        retain(args.artifacts / 'finalization-failure.json', {'error': f'{type(error).__name__}: {error}'})
        print(f"rea-crafting-case finalization failed; retained {args.artifacts}", file=sys.stderr, flush=True)
        return 1
    print(f"rea-crafting-case completed; retained {args.artifacts}", flush=True)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
