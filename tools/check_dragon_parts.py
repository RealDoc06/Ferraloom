#!/usr/bin/env python3
"""Four fresh sequential exact26.3 process/world/client boots: Java24, Java1, Rust24, Rust1.

Normal difficulty, frozen NoAI origin parts: actual Attack, committed health, canonical NBT state.
The 24 frozen residents never step or park; cleanup waits for their final full-fact
sweep and clean owned shutdown. Only a fresh one-subject world steps the actual
lethal head hit and shared 200-tick death owner. Source-derived
expectations are not runtime gold. No fight/flight/Scanning AI or rendering claim.
Importing this module starts no processes. ROOT alone executes this campaign.
"""
from __future__ import annotations

import argparse
import base64
from copy import deepcopy
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import socket
import struct
import subprocess
import tempfile
import time
import uuid

import oracle
from check_block_data import interaction_deadline
from check_container_metadata import Fixture, now, typed_predicate
from check_crafting import payload, validate_fence
from check_item_interchange import INNER_JAR_SHA256
from check_player_data import (JDK_SHA256, NAME, PLAYER_ID, UUID_INTS, cached_reference,
    command_result, read_bytes, teleport_exact, unique_object, write_json)
from parity import protocol, suite
from parity.bot import Client
from parity.furnaces import digest, require_frozen, step_exact
from parity.protocol import require

ENGINES = ('java', 'rust')
BOOTS = ('java_frozen24', 'java_death1', 'rust_frozen24', 'rust_death1')
PARTS = ('head', 'neck', 'body', 'tail1', 'tail2', 'tail3', 'wing1', 'wing2')
STATE_CASES = ('default_hover', *(f'phase_{phase}' for phase in range(11)),
               'invalid_negative', 'invalid_eleven', 'nbt_fields')
CASES = (*STATE_CASES, *(f'part_{part}' for part in PARTS), 'unknown_then_positive', 'death_200')
POSITION = {'x': 0.0, 'y': 0.0, 'z': 2.0, 'yaw': 180.0, 'pitch': 0.0}
OWNED = {'tools/parity/client.ts', 'tools/parity/bot.py', 'tools/parity/dragon_parts.ts',
         'tools/check_dragon_parts.py', 'tests/protocol_client.test.ts', 'tests/test_dragon_parts.py',
         'tests/test_statistics_observation.py', 'tools/rea_crafting_case.py', 'tests/test_rea_crafting_case.py'}
GAPS = [
    'Source/helper units do not establish runtime acceptance; only actual observations in this execution count.',
    'Never-ticked parts at literal origin; frozen NoAI fixture is not multipart placement/rendering or AI proof.',
    'Normal difficulty fixture only; observed Peaceful Dragon summon admission discrepancy remains OPEN, not fixed or verified.',
    'Positive committed health loss only; uncontrolled player recovery excludes exact damage multipliers.',
    'Unknown signed ID assumes a fresh allocator has not allocated two billion IDs; tracking absence alone is insufficient.',
    'Parent isPickable=false is not an attack-immunity negative: direct parent hurt maps BODY.',
    'Phase6 Scanning AI is UNPORTED. Phase/NBT rows are state reads, not persistence/restart results.',
    'Natural creation initial Holding0 deferred: Java fight ticks before entities, Rust after; an after-step read cannot prove initialization.',
    'No full fight, natural Holding runtime, XP totals, crystals, gateways, rendering or advancement claim; no verified-family promotion.',
    'AddEntity is HEADER ONLY: LpVec3 body absent; raw tail retained, full packet consumption never claimed.',
]
PRIMARY = {
    'r28-dragon-paired-feasibility/assessment.txt': '675d52c20bc8bef317b936106ef7330d65611378a2a5c34394f85935997521b1',
    'r28-dragon-paired-feasibility/packet-primary-97/run/DragonPairedPackets.javap.txt': '279429a1278517854a02dfebdbb92b818f9debdc4fe0ecb883038c17c0403b86',
    'r28-dragon-paired-feasibility/packet-primary-97/run/receipt.json': 'b62afcf8f8b085badb5ecb438db0058411ba35815dfead5a1bd888abd6fc584a',
    'r28-dragon-paired-feasibility/transitive-primary-421/run/DragonPacketLeaves.javap.txt': 'd03bd067ffd02d39b1dca21a927e2d34f0f58ed9cacfde8d433ee8ba42671c08',
    'r28-dragon-paired-feasibility/transitive-primary-421/run/receipt.json': 'e5502337b3ef557456cc6c8ec3188be738807139f2c9a3d21c29a1f8a9888181',
    'r25-instrument-primary/Instrument-Items-Holder-Codecs.javap.txt': '6752df8ed74912db0a51b7b3a32c99f2113d9dc7a5c2fb17324b90ac1c8736bb',
    'r26-jukebox-hit-primary/JukeboxHit.javap.txt': '7d3da4caf09a68f9f60d90801fed6b6f24f0397b5ba3855e66f9d02cfa677a18',
    'r28-dragon-primary-f405/run/DragonMultipart.javap.txt': '2cdbf7be8f7996a28c2b66ba464162624a369cfa12201cdab62c541c282125a0',
    'r28-dragon-primary-f405/run/receipt.json': '08705faaa78bb09f9beb7961bcb33b6ee1c22d85108aec106f5960ac95595cdc',
    'r28-dragon-part-assessment/phase-primary-run/DragonPhaseDamage.javap.txt': '98808c7d3cb8ab895eeac606afb6d6458aa3cdb41eb5ef112821c7f255baf83d',
    'r28-dragon-part-assessment/phase-primary-run/receipt.json': '02006f4233d929369d49710dd445f69ed7a294f1ab9f90cee90a971b2872b6c2',
    'r28-dragon-part-assessment/death-primary-97/run/DragonDeathAndFight.javap.txt': 'def2239ef919110e939a4444774163cca7f210f5bbb0fd7394dc6a73221f98a7',
    'r28-dragon-part-assessment/death-primary-97/run/receipt.json': '25f1f1823c43d7c5d30a45c22c133dbfc462a1b4ea8fecf0a9bbbec90dbbc7dc',
    'r28-dragon-part-assessment/dr1-wire-dr2-correction-review/report.txt': 'bf49161cc3f1aa4c8d97e81bce54cc7e7ef1448186f14fb2f088111e548fcda5',
}
REPORT_PINS = {'packets.json': '2f0486311f18b7d3ec232028f3bff82c657d2f0240d2d65a3df40408db926886',
                'registries.json': '51f62d56f8bd0e9134ee88dd9226b64e9e56cf6a32377540d44291bf47ecdac6'}


def boot_cases(role: str) -> tuple[str, ...]:
    require(role in BOOTS, 'Unknown boot role')
    return CASES[:-1] if role.endswith('_frozen24') else ('death_200',)


def case_role(engine: str, name: str) -> str:
    require(engine in ENGINES and name in CASES, 'Unknown engine/case')
    return engine + ('_death1' if name == 'death_200' else '_frozen24')


def supplied_identity(label: str) -> dict:
    ordinal = CASES.index(label) + 1
    return {'uuid': f'13572468-2468-1357-0000-{ordinal:012x}',
            'uuid_ints': [324478056, 610800471, 0, ordinal]}


def origin_predicate(subject: dict) -> str:
    return f'if data entity {subject["uuid"]} {{UUID:[I;{",".join(map(str, subject["uuid_ints"]))}],Pos:[0.0d,0.0d,0.0d],NoAI:1b}}'


def harness_path(path: str) -> bool:
    """An exhaustive allowlist keeps every unlisted production/build input equal."""
    return path in OWNED or path == 'README.md' or path.startswith(('docs/', 'compatibility/results/'))


def git(*args: str) -> str:
    result = subprocess.run(['git', *args], cwd=oracle.ROOT, capture_output=True, text=True, check=True)
    return result.stdout


def engine_equivalence(engine: str, harness: str) -> dict:
    require(git('rev-parse', '--verify', engine + '^{commit}').strip() == engine, 'Unknown exact engine B')
    require(git('rev-parse', '--verify', harness + '^{commit}').strip() == harness, 'Unknown exact harness H')
    # --no-renames includes both sides of a rename, preventing an allowlisted
    # destination from hiding a removed production input. NUL preserves paths.
    changed = [path for path in git('diff', '--no-renames', '--name-only', '-z', engine, harness, '--').split('\0') if path]
    require(all(harness_path(path) for path in changed), {'non_harness_B_to_H_changes': changed})
    production_diff = git('diff', '--no-ext-diff', '--no-textconv', '--no-renames', engine, harness, '--', '.',
        ':(exclude,literal)README.md', ':(exclude,glob)docs/**', ':(exclude,glob)compatibility/results/**',
        *(':(exclude,literal)' + path for path in sorted(OWNED)))
    require(production_diff == '', 'Actual production/build-input B..H diff must be empty')
    return {'engine_B': engine, 'harness_H': harness, 'changed_paths': changed,
            'production_diff': production_diff, 'production_diff_sha256': hashlib.sha256(production_diff.encode()).hexdigest(), 'build_label': engine,
            'binary_source_association': 'caller supplied; not embedded provenance',
            'validation_scope': 'ROOT full native B separately; light helpers H separately; no full native H claim'}


def primary_contract(args: argparse.Namespace) -> dict:
    for name, expected in PRIMARY.items():
        require(digest(args.primary_root / name) == expected, 'Missing/changed primary: ' + name)
    reports = {}
    for name, expected in REPORT_PINS.items():
        path = args.reference_cache / 'generated/reports' / name
        require(digest(path) == expected, 'Changed official report: ' + name)
        reports[name] = json.loads(read_bytes(path), object_pairs_hook=unique_object)
    packets = reports['packets.json']['play']
    for direction, name, identifier in (('serverbound', 'attack', 1), ('serverbound', 'client_command', 12),
            ('clientbound', 'add_entity', 1), ('clientbound', 'remove_entities', 78), ('clientbound', 'award_stats', 3)):
        require(packets[direction]['minecraft:' + name]['protocol_id'] == identifier, 'Changed packet ID: ' + name)
    require(reports['registries.json']['minecraft:entity_type']['entries']['minecraft:ender_dragon']['protocol_id'] == 44,
            'Changed dragon entity type')
    return {'primary': PRIMARY, 'reports': REPORT_PINS, 'spawn_decode': 'header only, raw LpVec3/angles/data tail retained'}


def manifest() -> dict:
    paths = [*sorted((oracle.ROOT / 'tools').glob('check_*.py')),
             oracle.ROOT / 'tests/protocol_client.test.ts', oracle.ROOT / 'tests/test_dragon_parts.py']
    return {**suite.harness_manifest(), **{str(path.relative_to(oracle.ROOT)): digest(path) for path in paths}}


def parse_numeric(output: list[str], kind: str) -> dict:
    """Read actual data-get feedback, refusing errors, ambiguity and wrong NBT types."""
    literals = [match.group(1) for line in output
                if (match := re.search(r' has the following entity data: (.+)$', line))]
    require(len(literals) == 1, {'missing_or_ambiguous_data_read': output})
    literal = literals[0]
    pattern = (r'[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?' + ('[fF]' if kind == 'Float' else '[dD]')
               if kind in ('Float', 'Double') else r'-?\d+')
    require(kind in ('Float', 'Int', 'Double') and re.fullmatch(pattern, literal), {'wrong_typed_numeric_read': literal, 'expected': kind})
    value = (struct.unpack('>f', struct.pack('>f', float(literal[:-1])))[0] if kind == 'Float'
             else float(literal[:-1]) if kind == 'Double' else int(literal))
    require(math.isfinite(value), 'Nonfinite authoritative value')
    return {'nbt_type': kind, 'value': value, 'literal': literal}


def validate_packets(observations: list[dict]) -> None:
    require(len(observations) <= 4096, 'Dragon observation overflow')
    last = -1
    for packet in observations:
        raw = payload(packet)
        require(len(raw) <= 65536 and type(packet['sequence']) is int and packet['sequence'] > last, 'Invalid packet order/size')
        last = packet['sequence']
        if packet['kind'] == 'spawn_header':
            require(packet['packet_id'] == 1 and packet['complete'] is False and packet['entity_type'] >= 0, 'Bad spawn header')
            header = (b'\x01' + protocol.varint(packet['entity_id']) + uuid.UUID(packet['uuid']).bytes
                      + protocol.varint(packet['entity_type']) + struct.pack('>3d', *(packet[axis] for axis in ('x', 'y', 'z'))))
            remainder = base64.b64decode(packet['undecoded_remainder_base64'], validate=True)
            require(bool(remainder) and raw == header + remainder and packet['decoded_bytes'] == len(header)
                    and packet['payload_bytes'] == len(raw), 'Changed/truncated header or opaque remainder')
        else:
            require(packet['kind'] == 'remove' and packet['packet_id'] == 78 and packet['complete'] is True, 'Unknown Dragon packet')
            ids = packet['entity_ids']
            require(len(ids) <= 4096 and all(type(value) is int and -2**31 <= value < 2**31 for value in ids), 'Bad removal IDs')
            require(raw == b'\x4e' + protocol.varint(len(ids)) + b''.join(protocol.varint(value) for value in ids)
                    and packet['payload_bytes'] == packet['decoded_bytes'] == len(raw), 'Partial/changed removal packet')


def validate_parent(subject: dict, observations: list[dict], *, removed: bool = False) -> None:
    validate_packets(observations)
    identifier = subject['entity_id']
    require(type(identifier) is int and 0 < identifier < 2**31 - 9, 'Fresh bounded allocator precondition failed')
    spawns = [packet for packet in observations if packet['kind'] == 'spawn_header' and packet['uuid'] == subject['uuid']]
    require(len(spawns) == 1 and spawns[0] == subject['spawn'] and spawns[0]['entity_type'] == 44
            and spawns[0]['entity_id'] == identifier and all(spawns[0][axis] == 0 for axis in ('x', 'y', 'z')), 'Missing unique origin parent spawn')
    reserved = set(range(identifier + 1, identifier + 9))
    require(not any(packet['entity_id'] in reserved for packet in observations if packet['kind'] == 'spawn_header'), 'Part ordinary spawn')
    removals = [value for packet in observations if packet['kind'] == 'remove' for value in packet['entity_ids']]
    require(not reserved.intersection(removals), 'Part ordinary removal')
    require(removals.count(identifier) == (1 if removed else 0), 'Wrong parent removal count')


def validate_position(evidence: dict, identity: str, expected: tuple[float, float, float]) -> None:
    require(evidence['uuid'] == identity and evidence['expected'] == list(expected)
            and evidence.get('verified') is True and len(evidence['reads']) == 3, 'Missing exact position witness')
    for index, (read, value) in enumerate(zip(evidence['reads'], expected)):
        require(read['command'] == f'data get entity {identity} Pos[{index}]' and read['field'] == f'Pos[{index}]'
                and read['expected'] == value and read['actual'] == parse_numeric(read['output'], 'Double')
                and read['actual']['value'] == value, 'Wrong ordered Double position witness')


def validate_number(actual: dict, kind: str) -> None:
    require(actual == parse_numeric(['Dragon has the following entity data: ' + actual['literal']], kind),
            'Changed typed numeric literal/value')


def validate_health_read(row: dict, identity: str) -> None:
    actual = {key: row[key] for key in ('nbt_type', 'value', 'literal')}
    validate_number(actual, 'Float')
    read = row['full_read']
    command = f'data get entity {identity} Health'
    scaled = read['scaled_read']
    require(read['uuid'] == identity and read['field'] == 'Health' and read['command'] == command
            and read['actual'] == actual == parse_numeric(read['output'], 'Float')
            and read['typed_predicate']['predicate'] == f'if data entity {identity} {{Health:{actual["literal"]}}}'
            and scaled['command'] == command + ' 1000000' and scaled['mode'] == 'result'
            and scaled['expected'] == math.floor(actual['value'] * 1000000)
            and scaled['predicate'] == f'if score Observation pdprobe matches {scaled["expected"]}',
            'Missing actual typed/scaled target health')


def validate_resident(witness: dict, subject: dict, t0: int) -> None:
    require(witness['uuid'] == subject['uuid'] and witness['t0'] == t0
            and witness['gametime_before'] == witness['gametime_after'] == t0
            and witness['frozen_after'] is True
            and witness['identity_predicate']['predicate'] == origin_predicate(subject), 'Wrong resident origin/NoAI/T0 witness')
    validate_position(witness['position'], subject['uuid'], (0.0, 0.0, 0.0))


def validate_attack(attack: dict, *, offset: int | None, lethal: bool = False) -> None:
    subject, request, fence = attack['subject'], attack['request'], attack['fence']
    initial = subject['initial']
    require(initial['DragonPhase']['nbt_type'] == 'Int' and initial['DragonPhase']['value'] == 10
            and initial['DragonDeathTime']['nbt_type'] == 'Int' and initial['DragonDeathTime']['value'] == 0
            and initial['Health']['nbt_type'] == 'Float' and (0 < initial['Health']['value'] < 0.1 if lethal else initial['Health']['value'] == 200),
            'Attack needs an actually observed fresh hovered victim')
    target = attack['target_id']
    require(type(target) is int and -2**31 <= target < 2**31, 'Invalid signed attack ID')
    if offset is not None:
        require(1 <= offset <= 8 and target == subject['entity_id'] + offset, 'Wrong derived part ID')
    else:
        require(target == -2147483000 and attack['allocation_assumption'], 'Missing fresh allocator negative boundary')
        require(all(packet.get('entity_id') != target for packet in fence['observations']), 'Negative ID already tracked')
    expected = b'\x01' + protocol.varint(target)
    require(request['written'] is True and request['packet_id'] == 1 and request['entity_id'] == target
            and payload(request) == expected, 'Wrong latest Attack encoding')
    validate_fence(fence, request['after_sequence'])
    wire = attack['wire']
    sends = [index for index, packet in enumerate(wire) if packet['direction'] == 'serverbound' and packet['packet_id'] == 1]
    requests = [index for index, packet in enumerate(wire) if packet['direction'] == 'serverbound' and packet['packet_id'] == 12]
    replies = [index for index, packet in enumerate(wire) if packet['direction'] == 'clientbound'
               and packet.get('sequence') == fence['response']['sequence']]
    require(len(sends) == len(requests) == len(replies) == 1 and sends[0] < requests[0] < replies[0], 'Missing ordered Attack/stats wire fence')
    require(len({packet['pid'] for packet in wire}) == 1 and payload(wire[sends[0]]) == expected
            and payload(wire[requests[0]]) == b'\x0c\x01' and payload(wire[replies[0]]) == payload(fence['response']), 'Fence spans clients or wrong bytes')
    require(attack['gametime_before'] == attack['gametime_after'] == attack['t0'] and attack['frozen_after'] is True, 'Attack advanced game ticks')
    before, after = attack['before'], attack['after']
    identity = subject['uuid']
    retained = attack['retained_health']
    require(set(before) == set(after) == set(retained) and identity in before, 'Missing current victim health')
    for values in (before, after):
        validate_health_read(values[identity], identity)
        for key in values.keys() - {identity}:
            row = values[key]
            validate_number(retained[key], 'Float')
            command = f'if data entity {key} {{Health:{retained[key]["literal"]}}}'
            require(row['uuid'] == key and row['nbt_type'] == 'Float' and row['literal'] == retained[key]['literal']
                    and row['command'] == command and row['predicate']['predicate'] == command,
                    'Missing exact live non-target health witness; another current victim damaged')
    require({key: before[identity][key] for key in ('nbt_type', 'value', 'literal')} == retained[identity], 'Target lost last committed health')
    old, new = before[identity]['value'], after[identity]['value']
    require(old > 0 and (new == old if offset is None else new == 0 if lethal else 0 < new < old), 'No required committed health outcome')
    require(attack['healthy_connection']['phase'] == 'play' and attack['healthy_connection']['login'] is True, 'Connection lost after attack')
    require(attack['inventory']['slots'] == [None] * 46, 'Missing actual empty inventory')
    require(attack['player']['predicate'] == f'if entity @a[name={NAME},nbt={{playerGameType:0,Pos:[0.0d,0.0d,2.0d]}}]', 'Missing survival player witness')
    validate_position(attack['player_position'], str(PLAYER_ID), (0.0, 0.0, 2.0))
    validate_parent(subject, fence['observations'])


def validate_evidence(case: dict) -> None:
    require(case.get('name') in CASES, 'Unknown Dragon case')
    name = case['name']
    if name in STATE_CASES:
        values = case['values']
        expected = 10 if name == 'default_hover' else int(name[6:]) if name.startswith('phase_') else 0 if name.startswith('invalid_') else 9
        require(values['DragonPhase']['nbt_type'] == 'Int' and values['DragonPhase']['value'] == expected, 'Wrong canonical phase')
        require(values['DragonDeathTime']['nbt_type'] == 'Int' and values['DragonDeathTime']['value'] == (37 if name == 'nbt_fields' else 0), 'Wrong shared NBT counter')
        require(values['sitting_damage_received']['nbt_type'] == 'Float'
                and values['sitting_damage_received']['value'] == (2.5 if name == 'nbt_fields' else 0), 'Wrong sitting-damage NBT field')
        require(values['Health']['nbt_type'] == 'Float' and values['Health']['value'] == 200, 'Unhealthy state fixture')
        require(case['gametime_before'] == case['gametime_after'], 'State row ticked phase AI')
        validate_parent(case['subject'], case['observations'])
    elif name.startswith('part_'):
        validate_attack(case['attacks'][0], offset=PARTS.index(name[5:]) + 1)
        require(len(case['attacks']) == 1 and case['attacks'][0]['before'][case['subject']['uuid']]['value'] == 200,
                'Part route did not use one fresh healthy victim')
    elif name == 'unknown_then_positive':
        require(len(case['attacks']) == 2, 'Missing following positive control')
        validate_attack(case['attacks'][0], offset=None)
        validate_attack(case['attacks'][1], offset=1)
        negative, positive = case['attacks']
        identity = case['subject']['uuid']
        require(negative['subject']['uuid'] == positive['subject']['uuid'] == identity
                and negative['subject']['entity_id'] == positive['subject']['entity_id'] == case['subject']['entity_id']
                and all(negative['after'][identity][key] == positive['before'][identity][key] for key in ('nbt_type', 'value', 'literal'))
                and {packet['pid'] for packet in negative['wire']} == {packet['pid'] for packet in positive['wire']},
                'Following positive must use the same healthy victim and connection')
    else:
        validate_attack(case['attacks'][0], offset=1, lethal=True)
        require(len(case['attacks']) == 1 and case['prelethal']['DragonPhase']['value'] == 10
                and 0 < case['prelethal']['Health']['value'] < 0.1 and case['prelethal']['DragonDeathTime']['value'] == 0,
                'Death needs a fresh hovered positive-health victim, not a data-health-zero fixture')
        require(case['before_step']['DragonDeathTime']['nbt_type'] == 'Int' and case['before_step']['DragonDeathTime']['value'] == 0
                and case['before_step']['Health']['nbt_type'] == 'Float' and case['before_step']['Health']['value'] == 0, 'Counter advanced before real ticks')
        require([row['elapsed'] for row in case['steps']] == [1, 20, 155, 199, 200], 'Missing death milestones')
        previous_time = case['attacks'][0]['gametime_after']
        elapsed = 0
        for row in case['steps']:
            for step in row['chunks']:
                require(step['before'] == previous_time and step['frozen_after'] is True
                        and 0 < step['delta'] <= 100 and step['after'] == step['before'] + step['delta'], 'Invalid real step fence')
                previous_time = step['after']
                elapsed += step['delta']
            require(elapsed == row['elapsed'], 'Wrong cumulative death ticks')
            if elapsed < 200:
                require(row['values']['DragonDeathTime']['nbt_type'] == 'Int'
                        and row['values']['DragonDeathTime']['value'] == elapsed
                        and row['values']['Health']['nbt_type'] == 'Float' and row['values']['Health']['value'] == 0, 'Wrong shared owner count/health or early removal')
                validate_parent(case['subject'], row['observations'])
            else:
                require(row['absent']['predicate'] == 'unless entity ' + case['subject']['uuid']
                        and row['natural_lookup_failure']['success_zero']
                        and row['natural_lookup_failure']['command'] == f'data get entity {case["subject"]["uuid"]} DragonDeathTime'
                        and row['natural_lookup_failure']['mode'] == 'success'
                        and row['natural_lookup_failure']['expected'] == 0
                        and row['natural_lookup_failure']['predicate'] == 'if score Observation pdprobe matches 0', 'Missing real terminal lookup negative')
                validate_parent(case['subject'], row['observations'], removed=True)


def validate_case(case: dict) -> None:
    require(case.get('name') in CASES and case.get('result') == 'passed' and 'error' not in case, 'Failed/missing Dragon case')
    require(case.get('timeline') == ['started', 'evidence_retained', 'assertions_passed', 'cleanup_started',
            'cleanup_pending', 'cleanup_complete'], 'Missing stage or cleanup after failure')
    require(case.get('evidence_retained') is True, 'Missing retained actual evidence')
    validate_evidence(case)
    cleanup = case['cleanup']
    require(cleanup.get('verified') is True and cleanup.get('status') == 'complete'
            and cleanup['owned_boot'] == case['boot'] and cleanup['boundary'] == case['boot'], 'Unverified owned cleanup')
    if case['name'] != 'death_200':
        validate_resident(cleanup['resident'], case['subject'], case['t0'])
    else:
        require(cleanup['already_removed_by_actual_tick200'] == case['steps'][-1]['absent'], 'Missing actual death cleanup')


def campaign_rows(report: dict) -> list[dict]:
    accepted = False
    if report.get('result') == 'passed':
        try:
            validate_campaign(report)
            accepted = True
        except (AssertionError, KeyError, TypeError, ValueError):
            pass  # Partial/malformed captures remain raw evidence, never accepted rows.
    return [{'id': 'dragon_parts.' + name, 'result': 'passed' if all(
            report.get('boots', {}).get(case_role(engine, name), {}).get('cases', {}).get(name, {}).get('result') == 'passed'
            for engine in ENGINES) and accepted else 'failed',
            'boots': {case_role(engine, name): report.get('boots', {}).get(case_role(engine, name), {}).get('cases', {}).get(name, {})
                      for engine in ENGINES}} for name in CASES]


def validate_teardown(boot: dict, role: str) -> None:
    teardown = boot['teardown']
    require(teardown.get('boot') == role and teardown.get('exit_code') == 0 and not teardown.get('forced_stop')
            and not teardown.get('unexpected_exit') and bool(teardown.get('started')) and bool(teardown.get('stopped'))
            and type(teardown.get('pid')) is int and teardown['pid'] > 0, 'Unclean server stop/ownership')
    clients = teardown.get('clients', [])
    require(len(boot['clients']) == len(clients) == 1 and clients[0]['pid'] == boot['clients'][0]['pid']
            and clients[0]['exit_code'] == 0 and not clients[0]['forced_close'] and not clients[0]['failure'], 'Unclean client stop/ownership')


def validate_frozen_times(value: object, t0: int) -> None:
    if isinstance(value, dict):
        for key, child in value.items():
            if key in ('gametime', 'gametime_before', 'gametime_after', 'after_client_close_gametime', 't0'):
                require(type(child) is int and child == t0, 'Recorded frozen time differs from actual T0')
            elif key == 'frozen_times':
                require(all(type(actual) is int and actual == t0 for actual in child), 'Frozen read differs from actual T0')
            elif key in ('steps', 'chunks'):
                require(not child, 'Frozen boot contains real tick steps')
            validate_frozen_times(child, t0)
    elif isinstance(value, list):
        for child in value:
            validate_frozen_times(child, t0)


def validate_boot(boot: dict, role: str) -> None:
    names = boot_cases(role)
    frozen = role.endswith('_frozen24')
    require(boot['role'] == role and boot['engine'] == ('vanilla' if role.startswith('java_') else 'rust')
            and boot['result'] == 'passed' and set(boot['cases']) == set(names)
            and boot['input_pins_checked_before_boot'] is True and boot['status']['protocol'] == 777,
            'Missing real boot/client/required cases/pins')
    validate_teardown(boot, role)
    fresh = boot['fresh_arena']
    require(fresh['predicate'] == 'unless entity @e[type=minecraft:ender_dragon]' and fresh['roster'] == []
            and fresh['gametime'] == boot['t0'] and boot['world_absent_before_configure'] is True, 'Missing fresh world/arena roster witness')
    require([subject['label'] for subject in boot['subjects']] == list(names), 'Missing/distinct fresh subject roster')
    identities = {supplied_identity(name)['uuid'] for name in names}
    require(len(boot['subjects']) == len(names) and {subject['uuid'] for subject in boot['subjects']} == identities,
            'Required rows reused a victim or omitted a real subject')
    reserved = set()
    for subject in boot['subjects']:
        require({key: subject[key] for key in ('uuid', 'uuid_ints')} == supplied_identity(subject['label']), 'Wrong stable supplied UUID')
        require(subject['gametime_before'] == subject['gametime_after'] == boot['t0']
                and subject['identity_predicate']['predicate'] == origin_predicate(subject), 'Initial subject differs from origin/NoAI/T0')
        validate_position(subject['position'], subject['uuid'], (0.0, 0.0, 0.0))
        require(set(subject['initial']) == {'Health', 'DragonPhase', 'DragonDeathTime', 'sitting_damage_received'}, 'Missing baseline facts')
        for field, actual in subject['initial'].items():
            read = subject['initial_reads'][field]
            require(read['command'] == f'data get entity {subject["uuid"]} {field}' and read['uuid'] == subject['uuid']
                    and read['field'] == field and read['actual'] == actual == parse_numeric(read['output'], actual['nbt_type'])
                    and read['typed_predicate']['predicate'] == f'if data entity {subject["uuid"]} {{{field}:{actual["literal"]}}}', 'Missing initial full facts')
        validate_health_read({**subject['initial']['Health'], 'full_read': subject['initial_reads']['Health']}, subject['uuid'])
        current = set(range(subject['entity_id'], subject['entity_id'] + 9))
        require(not reserved.intersection(current), 'Overlapping per-boot entity ID ranges')
        reserved.update(current)
        validate_parent(subject, boot['observations'], removed=not frozen)
        require(boot['cases'][subject['label']]['subject'] == subject, 'Case/boot subject mismatch')
    health = {subject['uuid']: subject['initial']['Health'] for subject in boot['subjects']}
    for name in names:
        case = boot['cases'][name]
        require(case['name'] == name and case['subject']['label'] == name and case['boot'] == role
                and case['t0'] == boot['t0'], 'Case name/subject/boot ownership mismatch')
        require(('attacks' in case) == (name not in STATE_CASES), 'Wrong attack evidence presence for case')
        validate_case(case)
        if name in STATE_CASES:
            require(case['values'] == case['subject']['initial'] and case['gametime_before'] == case['gametime_after'] == boot['t0'],
                    'State row differs from retained initial facts/T0')
        for attack in case.get('attacks', []):
            require(attack['subject'] == case['subject'] and all(packet['pid'] == boot['clients'][0]['pid'] for packet in attack['wire']),
                    'Attack evidence belongs to another subject/client')
            cohort = identities - ({supplied_identity('unknown_then_positive')['uuid']} if name.startswith('part_') else set())
            require(set(attack['before']) == cohort and attack['retained_health'] == {key: health[key] for key in cohort}
                    and {packet['uuid'] for packet in attack['fence']['observations']
                         if packet['kind'] == 'spawn_header' and packet['entity_type'] == 44} == cohort
                    and attack['t0'] == boot['t0'], 'Missing fixed resident health cohort/last full-read literal/T0')
            health[case['subject']['uuid']] = {key: attack['after'][case['subject']['uuid']][key]
                                             for key in ('nbt_type', 'value', 'literal')}
    if frozen:
        validate_frozen_times(boot, boot['t0'])
        require(set(boot['final_sweep']) == identities and boot['after_client_close_gametime'] == boot['t0'], 'Missing final frozen sweep/T0')
        for subject in boot['subjects']:
            sweep = boot['final_sweep'][subject['uuid']]
            validate_resident(sweep, subject, boot['t0'])
            expected = {**subject['initial'], 'Health': health[subject['uuid']]}
            require(sweep['facts'] == expected, 'Final facts differ from origin/NoAI/last committed Health')
            for field, kind in (('Health', 'Float'), ('DragonPhase', 'Int'), ('DragonDeathTime', 'Int'), ('sitting_damage_received', 'Float')):
                validate_number(sweep['facts'][field], kind)
                read = sweep['reads_by_field'][field]
                require(read['command'] == f'data get entity {subject["uuid"]} {field}' and read['uuid'] == subject['uuid']
                        and read['field'] == field and read['actual'] == sweep['facts'][field] == parse_numeric(read['output'], kind)
                        and read['typed_predicate']['predicate'] == f'if data entity {subject["uuid"]} {{{field}:{read["actual"]["literal"]}}}',
                        'Missing final full fact read')
            validate_health_read({**sweep['facts']['Health'], 'full_read': sweep['health_read']}, subject['uuid'])
    else:
        require(boot['sole_subject']['roster'] == [supplied_identity('death_200')['uuid']]
                and boot['sole_subject']['predicate'] == 'unless entity @e[type=minecraft:ender_dragon,nbt=!{UUID:[I;324478056,610800471,0,25]}]',
                'Death world contains an old/unowned subject')
    validate_packets(boot['observations'])
    require({packet['uuid'] for packet in boot['observations'] if packet['kind'] == 'spawn_header' and packet['entity_type'] == 44} == identities,
            'Unexpected Dragon subject in fresh boot')
    received = [packet for packet in boot['wire'] if packet['direction'] == 'clientbound' and packet['packet_id'] in (1, 78)]
    require([packet['sequence'] for packet in received] == [packet['sequence'] for packet in boot['observations']]
            and all(payload(raw) == payload(decoded) for raw, decoded in zip(received, boot['observations'])),
            'Lost/evicted Dragon raw or decoded observations')
    require(all(packet['pid'] == boot['clients'][0]['pid'] for packet in boot['wire']), 'Boot evidence spans clients')


def finalize_cases(boot: dict, role: str) -> None:
    """Only real clean owned client/server stop may complete deferred row cleanup."""
    validate_teardown(boot, role)
    require(all(case.get('result') == 'pending' and case.get('cleanup', {}).get('status') == 'pending'
                and case['timeline'][-1:] == ['cleanup_pending'] and 'error' not in case for case in boot['cases'].values()),
            'Failed/unreached/early cleanup cannot be promoted')
    candidate = deepcopy(boot)
    for case in candidate['cases'].values():
        case['cleanup'].update(status='complete', verified=True, boundary=role)
        case['timeline'].append('cleanup_complete')
        case['result'] = 'passed'
    candidate['result'] = 'passed'
    validate_boot(candidate, role)
    boot.update(candidate)


def validate_campaign(report: dict) -> None:
    require(report.get('completed_boots') == list(BOOTS) and set(report.get('boots', {})) == set(BOOTS)
            and report.get('cleanup_complete') is True and not report.get('runtime_errors')
            and report.get('provenance_checked_before') is True and report.get('provenance_checked_after') is True,
            'Incomplete runtime/provenance/cleanup')
    directories = set()
    for role in BOOTS:
        boot = report['boots'][role]
        validate_boot(boot, role)
        require(boot['directory'] not in directories, 'Reused process/world boundary')
        directories.add(boot['directory'])
        expected_processes = report['processes'][boot['engine']]
        require([entry['boot'] for entry in expected_processes] == [owned for owned in BOOTS if report['boots'][owned]['engine'] == boot['engine']]
                and boot['teardown'] == next(entry for entry in expected_processes if entry['boot'] == role), 'Unowned process boundary')
    for previous, following in zip(BOOTS, BOOTS[1:]):
        require(report['boots'][previous]['teardown']['stopped'] <= report['boots'][following]['teardown']['started'], 'Overlapping boot process boundaries')
    for engine in ENGINES:
        cases = [case for role in BOOTS if role.startswith(engine + '_') for case in report['boots'][role]['cases'].values()]
        require(len(cases) == 25 and {case['name'] for case in cases} == set(CASES)
                and len({case['subject']['uuid'] for case in cases}) == 25, 'Missing exact disjoint 24+1 engine coverage')
    require(report.get('comparison') == 'equal_bounded_contracts'
            and comparable(report['boots'], 'java') == comparable(report['boots'], 'rust'), 'Missing paired semantic comparison')


class Probe:
    """Retain each actual observation before interpreting or asserting it."""
    def __init__(self, fixture: Fixture, client: Client, entry: dict):
        self.fixture, self.server, self.client, self.entry = fixture, fixture.server, client, entry
        self.case = entry['setup']
        self.subjects: list[dict] = []
        self.health: dict[str, dict] = {}
        self.last_reads: dict[tuple[str, str], dict] = {}

    def frozen_time(self) -> int:
        require_frozen(self.server)
        actual = suite.game_time(self.server)
        self.case.setdefault('frozen_times', []).append(actual)
        self.retain()
        require(actual == self.entry['t0'], 'Frozen fixture differs from actual T0')
        return actual

    def retain(self) -> None:
        (self.server.directory / 'boot-evidence.json').write_text(
            json.dumps(self.entry, allow_nan=False, separators=(',', ':')) + '\n')

    def field(self, subject: dict, field: str, kind: str) -> dict:
        command = f'data get entity {subject["uuid"]} {field}'
        evidence = {'command': command, 'field': field, 'uuid': subject['uuid']}
        self.case.setdefault('reads', []).append(evidence)
        self.retain()
        evidence['output'] = self.server.command(command)
        self.retain()
        evidence['actual'] = parse_numeric(evidence['output'], kind)
        self.retain()
        evidence['typed_predicate'] = typed_predicate(self.server,
            f'if data entity {subject["uuid"]} {{{field}:{evidence["actual"]["literal"]}}}')
        if field == 'Health':
            expected = math.floor(evidence['actual']['value'] * 1000000)
            evidence['scaled_read'] = command_result(self.server, command + ' 1000000', expected)
        self.retain()
        self.last_reads[(subject['uuid'], field)] = deepcopy(evidence)
        return deepcopy(evidence['actual'])

    def facts(self, subject: dict) -> dict:
        return {field: self.field(subject, field, kind) for field, kind in
                (('Health', 'Float'), ('DragonPhase', 'Int'), ('DragonDeathTime', 'Int'), ('sitting_damage_received', 'Float'))}

    def position(self, identity: str, expected: tuple[float, float, float]) -> dict:
        """Read ordered finite Double coordinates; compound list patterns only prove containment."""
        evidence = {'uuid': identity, 'expected': list(expected), 'reads': []}
        self.case.setdefault('positions', []).append(evidence)
        self.retain()
        for index, value in enumerate(expected):
            field = f'Pos[{index}]'
            read = {'command': f'data get entity {identity} {field}', 'field': field, 'expected': value}
            evidence['reads'].append(read)
            self.retain()
            read['output'] = self.server.command(read['command'])
            self.retain()
            read['actual'] = parse_numeric(read['output'], 'Double')
            self.retain()
            require(read['actual']['value'] == value, {'wrong_exact_position': evidence})
        evidence['verified'] = True
        self.retain()
        return deepcopy(evidence)

    def observations(self) -> list[dict]:
        observations = self.client.state()['dragon_packets']
        self.entry['observations'] = observations
        self.retain()
        validate_packets(observations)
        return observations

    def summon(self, label: str, *, phase: int | None = 10, health: str = '200.0f', death: int = 0,
               sitting: str = '0.0f') -> dict:
        require(label in boot_cases(self.entry['role']) and not any(other['label'] == label for other in self.subjects), 'Wrong/reused boot subject label')
        ordinal = CASES.index(label) + 1
        # A supplied command UUID is checked unchanged in real NBT and spawn;
        # no saved identity is migrated or rewritten between fresh boots.
        identity = supplied_identity(label)['uuid']
        subject = {'label': label, **supplied_identity(label), 'gametime_before': self.frozen_time()}
        self.subjects.append(subject)
        self.entry['subjects'] = self.subjects
        fields = [f'UUID:[I;324478056,610800471,0,{ordinal}]', f'Health:{health}', 'NoAI:1b', 'Silent:0b',
                  f'DragonDeathTime:{death}', f'sitting_damage_received:{sitting}']
        if phase is not None:
            fields.append(f'DragonPhase:{phase}')
        subject['command'] = 'summon minecraft:ender_dragon 0.0 0.0 0.0 {' + ','.join(fields) + '}'
        subject['after_sequence'] = self.client.state()['packets']
        self.retain()
        subject['output'] = self.server.command(subject['command'])
        self.retain()
        subject['identity_predicate'] = typed_predicate(self.server, origin_predicate(subject))
        subject['position'] = self.position(identity, (0.0, 0.0, 0.0))
        state = self.client.wait_state(lambda state: any(packet['kind'] == 'spawn_header'
            and packet['uuid'] == identity and packet['sequence'] > subject['after_sequence'] for packet in state['dragon_packets']))
        spawns = [packet for packet in state['dragon_packets'] if packet['kind'] == 'spawn_header' and packet['uuid'] == identity]
        subject['spawn'] = spawns[0]
        subject['entity_id'] = spawns[0]['entity_id']
        self.retain()
        validate_parent(subject, state['dragon_packets'])
        subject['initial'] = self.facts(subject)
        subject['initial_reads'] = {field: deepcopy(self.last_reads[(identity, field)]) for field in subject['initial']}
        subject['gametime_after'] = self.frozen_time()
        self.health[identity] = deepcopy(subject['initial']['Health'])
        self.retain()
        return subject

    def resident(self, subject: dict) -> dict:
        """Actual origin/NoAI/UUID/Double-position witness, pending owned shutdown."""
        evidence = {'uuid': subject['uuid'], 't0': self.entry['t0'], 'gametime_before': self.frozen_time()}
        self.case['resident'] = evidence
        self.retain()
        evidence['identity_predicate'] = typed_predicate(self.server, origin_predicate(subject))
        evidence['position'] = self.position(subject['uuid'], (0.0, 0.0, 0.0))
        evidence['gametime_after'] = self.frozen_time()
        evidence['frozen_after'] = True
        self.retain()
        validate_resident(evidence, subject, self.entry['t0'])
        return evidence

    def resident_cleanup(self, subject: dict) -> None:
        self.case['cleanup'] = {'status': 'pending', 'verified': False, 'owned_boot': self.entry['role'],
                                'resident': self.resident(subject)}
        self.retain()

    def health_map(self, target: dict) -> dict:
        result = {}
        for other in self.subjects:
            identity = other['uuid']
            if identity == target['uuid']:
                actual = self.field(other, 'Health', 'Float')
                result[identity] = {**actual, 'full_read': deepcopy(self.last_reads[(identity, 'Health')])}
            else:
                literal = self.health[identity]['literal']
                command = f'if data entity {identity} {{Health:{literal}}}'
                witness = result[identity] = {'uuid': identity, 'nbt_type': 'Float', 'literal': literal, 'command': command}
                self.case.setdefault('non_target_witnesses', []).append(witness)
                witness['predicate'] = typed_predicate(self.server, command)
        self.retain()
        return result

    def attack(self, subject: dict, target_id: int, *, offset: int | None, lethal: bool = False) -> dict:
        evidence = {'subject': deepcopy(subject), 'target_id': target_id,
                    'allocation_assumption': 'fresh isolated allocator; no two-billion-ID allocation; untracked does not prove globally absent'}
        self.case.setdefault('attacks', []).append(evidence)
        evidence['t0'] = self.entry['t0']
        evidence['gametime_before'] = self.frozen_time()
        evidence['retained_health'] = deepcopy(self.health)
        evidence['before'] = self.health_map(subject)
        evidence['player'] = typed_predicate(self.server,
            f'if entity @a[name={NAME},nbt={{playerGameType:0,Pos:[0.0d,0.0d,2.0d]}}]')
        evidence['player_position'] = self.position(str(PLAYER_ID), (POSITION['x'], POSITION['y'], POSITION['z']))
        evidence['inventory'] = self.client.state()['player_inventory']
        self.retain()
        require(evidence['inventory'] is not None and evidence['inventory']['slots'] == [None] * 46, 'Attack needs actual empty inventory')
        start = len(self.client.events)
        evidence['request'] = self.client.request('dragon_attack', entity_id=target_id)
        self.retain()
        evidence['fence'] = self.client.dragon_fence()
        evidence['after'] = self.health_map(subject)
        evidence['gametime_after'] = self.frozen_time()
        evidence['frozen_after'] = True
        state = self.client.state()
        evidence['healthy_connection'] = {key: state[key] for key in ('phase', 'login', 'packets')}
        evidence['wire'] = [packet for packet in self.client.events[start:] if packet.get('event') == 'wire_packet']
        self.retain()
        validate_attack(evidence, offset=offset, lethal=lethal)
        self.health[subject['uuid']] = {key: evidence['after'][subject['uuid']][key] for key in ('nbt_type', 'value', 'literal')}
        return evidence

    def row(self, name: str, action, cleanup) -> bool:
        self.case = case = self.entry['cases'][name]
        case.update(result='failed', timeline=['started'], boot=self.entry['role'], t0=self.entry['t0'])
        case.pop('error', None)
        self.retain()
        try:
            with interaction_deadline(self.server, 'dragon_parts:' + name):
                action()
                case['evidence_retained'] = True
                case['timeline'].append('evidence_retained')
                self.retain()
                validate_evidence(case)
                case['timeline'].extend(['assertions_passed', 'cleanup_started'])
                self.retain()
                cleanup()
                case['timeline'].append('cleanup_pending')
                case['result'] = 'pending'
                self.retain()
            return True
        except Exception as error:
            case['result'] = 'failed'
            case['timeline'].append('failure')
            case['error'] = f'{type(error).__name__}: {error}'
            self.retain()
            return False  # Preserve failed subjects; only owned shutdown follows.

    def state_case(self, name: str) -> bool:
        def action():
            self.case['gametime_before'] = self.frozen_time()
            phase = None if name == 'default_hover' else int(name[6:]) if name.startswith('phase_') else -1 if name == 'invalid_negative' else 11 if name == 'invalid_eleven' else 9
            subject = self.case['subject'] = self.summon(name, phase=phase,
                death=37 if name == 'nbt_fields' else 0, sitting='2.5f' if name == 'nbt_fields' else '0.0f')
            self.case['values'] = deepcopy(subject['initial'])
            self.case['observations'] = self.observations()
            self.case['gametime_after'] = self.frozen_time()
        return self.row(name, action, lambda: self.resident_cleanup(self.case['subject']))

    def death(self) -> bool:
        def action():
            subject = self.case['subject'] = self.summon('death_200', health='0.01f')
            self.entry['sole_subject'] = {**typed_predicate(self.server,
                'unless entity @e[type=minecraft:ender_dragon,nbt=!{UUID:[I;324478056,610800471,0,25]}]'),
                'roster': [subject['uuid']]}
            self.case['prelethal'] = deepcopy(subject['initial'])
            self.retain()
            require(self.case['prelethal']['DragonPhase']['value'] == 10
                    and 0 < self.case['prelethal']['Health']['value'] < 0.1
                    and self.case['prelethal']['DragonDeathTime']['value'] == 0, 'Wrong prelethal state')
            self.attack(subject, subject['entity_id'] + 1, offset=1, lethal=True)
            self.case['before_step'] = self.facts(subject)  # Direct UUID still finds health0.
            self.retain()
            require(self.case['before_step']['DragonDeathTime']['value'] == 0
                    and self.case['before_step']['Health']['value'] == 0, 'Death owner ticked before stepping')
            self.case['steps'] = []
            elapsed = 0
            for milestone in (1, 20, 155, 199, 200):
                row = {'elapsed': milestone, 'chunks': []}
                self.case['steps'].append(row)
                while elapsed < milestone:
                    ticks = min(100, milestone - elapsed)
                    row['chunks'].append(step_exact(self.server, ticks))
                    elapsed += ticks
                    self.retain()
                row['fence'] = self.client.dragon_fence()
                if milestone < 200:
                    row['values'] = self.facts(subject)
                    row['observations'] = self.observations()
                    self.retain()
                    require(row['values']['DragonDeathTime']['value'] == milestone and row['values']['Health']['value'] == 0,
                            'Incorrect real death counter or premature generic removal')
                    validate_parent(subject, row['observations'])
                else:
                    row['absent'] = typed_predicate(self.server, 'unless entity ' + subject['uuid'])
                    row['natural_lookup_failure'] = command_result(self.server,
                        f'data get entity {subject["uuid"]} DragonDeathTime', 0, mode='success')
                    row['natural_lookup_failure']['success_zero'] = True
                    state = self.client.wait_state(lambda state: any(packet['kind'] == 'remove'
                        and subject['entity_id'] in packet['entity_ids'] for packet in state['dragon_packets']))
                    row['observations'] = self.entry['observations'] = state['dragon_packets']
                    self.retain()
                    validate_parent(subject, row['observations'], removed=True)
        def cleanup():
            self.case['cleanup'] = {'status': 'pending', 'verified': False, 'owned_boot': self.entry['role'],
                                    'already_removed_by_actual_tick200': self.case['steps'][-1]['absent']}
        return self.row('death_200', action, cleanup)

    def exercise(self) -> None:
        if self.entry['role'].endswith('_death1'):
            require(not self.subjects and self.entry['fresh_arena']['roster'] == [], 'Death world is not fresh')
            if not self.death():
                raise AssertionError('Required actual death timeline failed')
            self.entry['observations'] = self.observations()
            return
        for name in STATE_CASES:
            if not self.state_case(name):
                raise AssertionError('Required state case failed: ' + name)
        self.case = self.entry['combat_setup'] = {}
        before = self.frozen_time()
        victims = []
        for part in PARTS:
            with interaction_deadline(self.server, 'dragon_parts:baseline part_' + part):
                subject = self.summon('part_' + part)
            victims.append(subject)
            initial = subject['initial']['Health']
            self.case.setdefault('initial_health', {})[subject['uuid']] = initial
            self.retain()
            require(initial['value'] == 200, 'Part needs fresh Health200')
        require(self.frozen_time() == before, 'Combat preparation ticked origin parts')
        for offset, (part, subject) in enumerate(zip(PARTS, victims), 1):
            def action(subject=subject, offset=offset):
                self.case['subject'] = subject
                self.attack(subject, subject['entity_id'] + offset, offset=offset)
            if not self.row('part_' + part, action, lambda subject=subject: self.resident_cleanup(subject)):
                raise AssertionError('Required part attack failed: ' + part)
        def negative():
            subject = self.case['subject'] = self.summon('unknown_then_positive')
            observations = self.observations()
            require(all(packet.get('entity_id') != -2147483000 for packet in observations), 'Negative ID already tracked')
            self.attack(subject, -2147483000, offset=None)
            self.attack(subject, subject['entity_id'] + 1, offset=1)
        if not self.row('unknown_then_positive', negative, lambda: self.resident_cleanup(self.case['subject'])):
            raise AssertionError('Unknown-ID negative/following positive failed')
        self.entry['observations'] = self.observations()

    def final_sweep(self) -> None:
        self.entry['final_sweep'] = {}
        for subject in self.subjects:
            self.case = sweep = self.entry['final_sweep'][subject['uuid']] = {}
            with interaction_deadline(self.server, 'dragon_parts:final ' + subject['label']):
                sweep.update(self.resident(subject))
                sweep['facts'] = self.facts(subject)
                sweep['reads_by_field'] = {field: deepcopy(self.last_reads[(subject['uuid'], field)]) for field in sweep['facts']}
                sweep['health_read'] = deepcopy(self.last_reads[(subject['uuid'], 'Health')])
                sweep['gametime_after'] = self.frozen_time()
                self.retain()
                require(sweep['facts'] == {**subject['initial'], 'Health': self.health[subject['uuid']]}, 'Final fact mismatch')
                validate_parent(subject, self.observations())


def comparable(boots: dict, engine: str) -> dict:
    """Compare bounded contracts; measured damage magnitudes stay in raw evidence."""
    require(engine in ENGINES, 'Unknown comparison engine')
    for role in BOOTS:
        if role.startswith(engine + '_'):
            validate_boot(boots[role], role)
    result = {}
    for name in CASES:
        case = boots[case_role(engine, name)]['cases'][name]
        validate_case(case)
        if name in STATE_CASES:
            result[name] = {field: value['value'] for field, value in case['values'].items()}
        else:
            result[name] = {'committed_outcomes': [
                'unchanged' if attack['target_id'] == -2147483000 else 'lethal' if name == 'death_200' else 'positive_nonlethal'
                for attack in case['attacks']]}
            if name == 'death_200':
                result[name]['shared_counter'] = [0, *(row['values']['DragonDeathTime']['value'] for row in case['steps'][:-1])]
                result[name]['removed_at'] = case['steps'][-1]['elapsed']
    return result


def input_pins(args: argparse.Namespace, report: dict, original: Path) -> dict:
    require(suite.git_state() == report['source'] and manifest() == report['manifest'], 'Harness changed')
    require(engine_equivalence(args.rust_source_commit, args.current_clean_commit) == report['engine_equivalence'], 'B/H changed')
    contract = primary_contract(args)
    require(contract == report['contract'], 'Primary contract changed')
    official = cached_reference()
    actual = {'official_sha256': digest(official), 'inner_jar_sha256': digest(oracle.CACHE / 'minecraft.jar'),
              'java_sha256': digest(Path(args.java)), 'original_binary_sha256': digest(original),
              'pinned_binary_sha256': digest(args.rust_binary)}
    require(actual['official_sha256'] == report['official']['sha256'] and actual['inner_jar_sha256'] == INNER_JAR_SHA256
            and actual['java_sha256'] == JDK_SHA256 and actual['original_binary_sha256'] == actual['pinned_binary_sha256']
            == report['binary']['sha256'], 'Cached runtime input changed')
    return actual


def run_campaign(args: argparse.Namespace, run: Path, report: dict, original: Path) -> None:
    report.update(boots={role: {'role': role, 'engine': 'vanilla' if role.startswith('java_') else 'rust',
        'result': 'failed', 'cases': {name:
        {'name': name, 'result': 'failed', 'error': 'Not reached; required case remains failed', 'timeline': []} for name in boot_cases(role)},
        'clients': [], 'setup': {}, 'subjects': [], 'observations': []} for role in BOOTS},
        completed_boots=[], runtime_errors=[], cleanup_complete=False)
    for role in BOOTS:
        entry = report['boots'][role]
        fixture = None
        try:
            report['active_boot'] = role
            entry['input_pins'] = input_pins(args, report, original)
            entry['input_pins_checked_before_boot'] = True
            directory = run / role
            require(not directory.exists(), 'Boot requires a fresh directory/world')
            entry['world_absent_before_configure'] = True
            fixture = Fixture(entry['engine'], directory, args, report)
            server = fixture.server
            entry['directory'] = str(server.directory)
            server.configure()
            fixture.start(role)
            entry['status'] = protocol.status_fields(server.port)
            require(entry['status']['protocol'] == 777, 'Live protocol differs')
            server.command('tick freeze')
            require_frozen(server)
            entry['t0'] = suite.game_time(server)
            server.command('forceload add -16 -16 16 16')
            suite.wait_loaded(server)
            with interaction_deadline(server, 'dragon_parts:origin arena'):
                for command in ('fill -8 -1 -8 8 32 8 air', 'fill -8 -1 -8 8 -1 8 stone',
                                'difficulty normal', 'scoreboard objectives add pdprobe dummy'):
                    entry['setup'].setdefault('commands', []).append({'command': command, 'output': server.command(command)})
                entry['setup']['difficulty'] = {'name': 'normal', 'query': command_result(server, 'difficulty', 2)}
                entry['setup']['floor'] = typed_predicate(server, 'if block 0 -1 2 minecraft:stone if block 0 0 2 minecraft:air')
            client = Client(server, log_name='dragon-parts-wire', block_data=True, dragon_parts=True, raw_packets=True)
            fixture.clients.append(client)
            entry['clients'].append({'name': client.name, 'pid': client.process.pid, 'argv': client.process.args})
            client.ready()
            with interaction_deadline(server, 'dragon_parts:player setup'):
                server.command(f'gamemode survival {NAME}')
                server.command(f'clear {NAME}')
                entry['setup']['teleport'] = teleport_exact(server, client, POSITION, expected_server_rotation=(-180.0, 0.0))
                entry['setup']['identity'] = typed_predicate(server,
                    f'if entity @a[name={NAME},nbt={{UUID:[I;{",".join(map(str, UUID_INTS))}],playerGameType:0}}]')
                client.wait_state(lambda state: (state.get('player_inventory') or {}).get('slots') == [None] * 46)
            probe = Probe(fixture, client, entry)
            with interaction_deadline(server, 'dragon_parts:fresh arena roster'):
                entry['fresh_arena'] = {**typed_predicate(server, 'unless entity @e[type=minecraft:ender_dragon]'),
                                        'roster': [], 'gametime': probe.frozen_time()}
            probe.exercise()
            if role.endswith('_frozen24'):
                probe.final_sweep()
            fixture.close(client)
            if role.endswith('_frozen24'):
                entry['after_client_close_gametime'] = probe.frozen_time()
            entry['wire'] = [packet for packet in client.events if packet.get('event') == 'wire_packet']
            entry['observations'] = [{key: value for key, value in packet.items() if key != 'event'}
                                     for packet in client.events if packet.get('event') == 'dragon_packet']
            log = server.directory / 'dragon-parts-wire.jsonl'
            entry['client_log'] = {'path': str(log), 'sha256': digest(log)}
            server.save_flush()
            fixture.stop()
            entry['teardown'] = deepcopy(fixture.processes[-1])
            finalize_cases(entry, role)
        except Exception as error:
            entry['error'] = f'{type(error).__name__}: {error}'
            report['runtime_errors'].append(role + ': ' + entry['error'])
        finally:
            if fixture is not None:
                errors = fixture.cleanup()
                entry['watchdog_events'] = deepcopy(fixture.server.abort_events)
                if fixture.processes:
                    entry['teardown'] = deepcopy(fixture.processes[-1])
                try:
                    validate_teardown(entry, role)
                except Exception as error:
                    errors.append(f'Process boundary: {type(error).__name__}: {error}')
                report['runtime_errors'].extend(role + ': ' + error for error in errors)
                if errors:
                    entry['result'] = 'failed'
                    entry['cleanup_errors'] = errors
                    for case in entry['cases'].values():
                        if case['result'] == 'passed':
                            case['result'] = 'failed'
                write_json(fixture.server.directory / 'boot-evidence.json', entry)
            report['completed_boots'].append(role)
        # A failed gameplay side still permits the other fresh side after clean
        # owned shutdown. A leaked/forced process boundary forbids another boot.
        if entry.get('cleanup_errors'):
            break
    require(not report['runtime_errors'] and report['completed_boots'] == list(BOOTS), 'Required campaign/cleanup failed')
    left, right = (comparable(report['boots'], engine) for engine in ENGINES)
    report['semantic_comparison'] = {'java': left, 'rust': right, 'scope': 'typed state and bounded positive/negative/death contracts; no exact damage multiplier comparison'}
    write_json(run / 'semantic-comparison.json', report['semantic_comparison'])
    require(left == right, 'Bounded paired Dragon contracts differ')
    report['comparison'] = 'equal_bounded_contracts'
    report['cleanup_complete'] = True


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--current-clean-commit', required=True)
    parser.add_argument('--rust-source-commit', required=True)
    parser.add_argument('--rust-binary', type=Path, required=True)
    parser.add_argument('--java', required=True)
    parser.add_argument('--reference-cache', type=Path, required=True)
    parser.add_argument('--primary-root', type=Path, required=True)
    parser.add_argument('--accept-eula', action='store_true')
    parser.add_argument('--workers', type=int, choices=(2,), default=2)
    parser.add_argument('--startup-timeout', type=float, choices=(180,), default=180)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    require(args.accept_eula, 'Explicit local fixture --accept-eula required')
    require(args.root.resolve() == oracle.ROOT.resolve(), '--root must be this harness checkout')
    require(all(re.fullmatch(r'[0-9a-f]{40}', value) for value in (args.current_clean_commit, args.rust_source_commit)), 'Exact H and B hashes required')
    cache = (oracle.ROOT / '.cache').resolve()
    require(args.output.resolve().is_relative_to(cache) and re.fullmatch(r'dragon-parts-[A-Za-z0-9_-]+\.json', args.output.name), 'Use fresh ignored .cache/dragon-parts-*.json')
    require(not args.output.is_symlink() and not any(path.is_symlink() for path in args.output.absolute().parents if path.is_relative_to(cache)), 'Linked output refused')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    output = args.output.open('x')
    runs = cache / 'runs'
    runs.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix='dragon-parts-', dir=runs))
    report = {'schema_version': 2, 'kind': 'dragon_parts', 'result': 'failed', 'created_at': now(),
        'scope': __doc__, 'target': {'version': '26.3', 'protocol': 777, 'data_version': 5023},
        'environment': {'host': socket.gethostname(), 'checkout': str(oracle.ROOT), 'fixture': str(run),
            'binding': '127.0.0.1 isolated', 'workers': 2, 'startup_seconds': 180, 'interaction_seconds': 20},
        'identity': {'name': NAME, 'offline_uuid': str(PLAYER_ID)}, 'gaps': GAPS, 'processes': {},
        'provenance_checked_before': False, 'provenance_checked_after': False}
    began = time.monotonic()
    try:
        report['source'] = suite.git_state()
        require(report['source'] == {'commit': args.current_clean_commit, 'dirty': False}, 'Requires exact current clean harness H')
        report['engine_equivalence'] = engine_equivalence(args.rust_source_commit, args.current_clean_commit)
        report['manifest'] = manifest()
        oracle.CACHE = args.reference_cache.resolve()
        oracle.acquire = suite.acquire = cached_reference  # No missing-cache download, even on a race.
        official = cached_reference()
        require(digest(oracle.CACHE / 'minecraft.jar') == INNER_JAR_SHA256, 'Changed cached inner JAR')
        args.java = str(Path(shutil.which(args.java) or args.java).resolve())
        require(digest(Path(args.java)) == JDK_SHA256, 'Requires pinned cached JDK25')
        report['java'] = {'path': args.java, 'sha256': JDK_SHA256}
        report['official'] = {'path': str(official), 'sha256': digest(official), 'inner_sha256': INNER_JAR_SHA256}
        report['contract'] = primary_contract(args)
        original = args.rust_binary.resolve()
        require(original.is_file() and os.access(original, os.X_OK), 'Existing ROOT-built Rust binary required')
        binary_hash = digest(original)
        args.rust_binary = suite.pin_binary(original, run)
        require(digest(original) == digest(args.rust_binary) == binary_hash, 'Binary changed during pin')
        report['binary'] = {'source': str(original), 'pinned': str(args.rust_binary), 'sha256': binary_hash,
                            'source_commit': args.rust_source_commit, 'association': 'caller supplied; not embedded provenance; build label remains B'}
        report['provenance_checked_before'] = True
        run_campaign(args, run, report, original)
        report['final_input_pins'] = input_pins(args, report, original)
        report['provenance_checked_after'] = True
        validate_campaign(report)
        report['result'] = 'passed'
    except Exception as error:
        report['error'] = f'{type(error).__name__}: {error}'
        report.setdefault('runtime_errors', []).append(report['error'])
    finally:
        report['cases'] = campaign_rows(report)
        report.update(finished=now(), duration_seconds=round(time.monotonic() - began, 3))
        raw = json.dumps(report, indent=2, allow_nan=False) + '\n'
        with (run / 'report.json').open('x') as retained:
            retained.write(raw)
        with output:
            output.write(raw)
    print(f"dragon_parts {report['result']}: {args.output}; artifacts {run}", flush=True)
    return 0 if report['result'] == 'passed' else 1


if __name__ == '__main__':
    raise SystemExit(main())
