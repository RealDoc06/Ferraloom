#!/usr/bin/env python3
"""Bounded ordinary crafting acceptance, exact 26.3/protocol777/DataVersion5023.

Exactly two fresh sequential boots, Java then Rust, with one real client each.
Every grid placement is SWAP2, every cursor acquisition is Pickup0/1. Clicks use
received revisions/cursor, zero predictions, and subsequent AwardStats fences.
All authored tests and runtime cases remain UNRUN until ROOT executes them.
"""
from __future__ import annotations

import argparse
import base64
from collections import Counter
from copy import deepcopy
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import struct
import sys
import tempfile
import time
import uuid

sys.dont_write_bytecode = True
import oracle
from check_block_data import interaction_deadline
from check_container_metadata import Fixture, now, typed_predicate
from check_item_interchange import INNER_JAR_SHA256, atom, compounds, typed_int, typed_snbt
from check_jukeboxes import read_fixture, retained_player
from check_player_data import (JDK_SHA256, NAME, PLAYER_ID, UUID_INTS, cached_reference,
    read_bytes, teleport_exact, unique_object, write_json)
from parity import protocol, suite
from parity.bot import Client
from parity.furnaces import digest, require_frozen
from parity.protocol import require

BOOTS = ('java_fresh', 'rust_fresh')
ITEMS = ('honey_bottle', 'sugar', 'glass_bottle', 'honey_block', 'milk_bucket',
         'bucket', 'egg', 'wheat', 'cake', 'cobblestone')
CRAFTED = ('sugar', 'honey_block', 'cake')
CASES = ('player_offset_left', 'player_offset_right', 'table_offset_left', 'table_offset_right',
         'full_cursor', 'throw_once', 'throw_all', 'player_four_honey', 'table_four_honey',
         'cake_shift', 'offhand_partial', 'offhand_spill', 'selected_before_offhand')
TABLE = {'x': 4104, 'y': 200, 'z': 6}
POSITION = {'x': 4104.5, 'y': 200.0, 'z': 8.5, 'yaw': 0.0, 'pitch': 0.0}
# The reused bounded NBT reader is intentionally fixed to this exact chunk256,0.
DROPS = '@e[type=minecraft:item,x=4096,y=196,z=0,dx=15,dy=16,dz=15]'
GAPS = [
    'No special recipes, recipe book, achievements, client prediction or graphical-client claim.',
    'Only ordinary default stacks; tagged/component fixtures are refused, never approximated with hash0.',
    'No full inventory parity, world interchange, restart or verified-family promotion.',
    'Goal ledger unchanged: 200 total, 27 partial, 173 unverified, 0 verified.',
]
PRIMARY = {
    'r28-crafting-primary-c07/paired-harness-assessment.txt': '48c8dd9601fa9434b45cc572040b17497483dbca48c1334120a34eb880175e55',
    'r28-crafting-primary-c07/wire-contract-addendum.txt': '8b6060dad238376244d39e9a12c45f55ee1d0021fc5e6284d60f26d526c9c7e1',
    'r28-crafting-primary-c07/contract.txt': '01402b18dbb58ad74be000010928d95706b2cdf31ef06a4d7587856ca7c8cbb0',
    'r28-crafting-primary-c07/followup-contract-addendum.txt': '923ee3cb53a04aba3107528954a17c20656b822a1bdbfb61bf0ba3f4a5909510',
    'r28-crafting-primary-c07/final-wire-contract.txt': 'ed6b89fe92cde921d441714638b916325406bc8f13a85c65ded105f0f022f377',
    'r28-crafting-primary-c07/run/Crafting.javap.txt': '95fce42cb0749a3a5f827f90b14f04ebbd2c0503565d3b60afacdec037bab0f9',
    'r28-crafting-primary-c07/followup-preparation/run/CraftingFollowup.javap.txt': 'da876545e7f2d7101ec512b6d4a916bec5465c339c5626f8e79dced69827de53',
    'r28-crafting-containers-primary/run/CraftingContainers.javap.txt': 'cd38cb8ce8baa30ab09cbbd9b68bff0a2415cbd2ec1bed684b395c3ada568ae8',
    'r28-crafting-packets-primary-f405/run/CraftingPackets.javap.txt': 'a8888ec1b75693343e065ca98936deb0bd9c38c8bd2c272e8ae3e3dc6349d169',
    'r28-crafting-packets-primary-f405/run/receipt.json': '504c09f47d658599b05867a005395cccd575bfda7e8bf096c2cda43ec14b6c42',
    'r28-crafting-wire-primary-79a/run/CraftingWireCodecs.javap.txt': 'cffefd6b9d9f8586e9fb19772bc4b0d28e0cc90cc8f50bb0d2a351f6265a5515',
    'r28-crafting-wire-primary-79a/run/receipt.json': 'fa7fe1195abf4aec886ecc5fc7124502ccad88ce3f786ee85b368057cf92bf22',
    'r28-crafting-wire-primary-79a/run/post-validation.json': '56f84066aa06cdd756febbe5b4433c2f13818a5e62f7e6e8efbb3d3a243938fd',
    'r26-horn-packet-primary/HornPackets.javap.txt': '2794ae66b19ab782ded6528debe06fb2eb05cbdb1c1d1dc838f54e3655072ee5',
    'r26-jukebox-packet-primary/JukeboxPackets.javap.txt': '6beb2933a97084fe16e3b60269826d69b4e5b5b544e5792de0ced4d16607468d',
    'r26-jukebox-hit-primary/JukeboxHit.javap.txt': '7d3da4caf09a68f9f60d90801fed6b6f24f0397b5ba3855e66f9d02cfa677a18',
    'r25-instrument-primary/Instrument-Items-Holder-Codecs.javap.txt': '6752df8ed74912db0a51b7b3a32c99f2113d9dc7a5c2fb17324b90ac1c8736bb',
}
REPORT_PINS = {
    'registries.json': '51f62d56f8bd0e9134ee88dd9226b64e9e56cf6a32377540d44291bf47ecdac6',
    'packets.json': '2f0486311f18b7d3ec232028f3bff82c657d2f0240d2d65a3df40408db926886',
}


def stack(name: str, count: int = 1) -> dict:
    require(name in ITEMS and type(count) is int and 1 <= count <= 64, 'Outside ordinary crafting fixture')
    return {'item': 'minecraft:' + name, 'count': count}


def wire_stack(value: object, items: dict) -> dict | None:
    if value is None:
        return None
    require(isinstance(value, dict) and set(value) == {'item_id', 'count'}, 'Nonplain wire stack')
    names = [name for name, identifier in items.items() if identifier == value['item_id']]
    require(type(value['item_id']) is int and len(names) == 1, 'Unknown crafting item ID')
    return stack(names[0], value['count'])


def saved_stack(value: object) -> dict:
    require(isinstance(value, dict) and set(value) <= {'id', 'count', 'components'}
            and isinstance(value.get('id'), str) and value['id'].startswith('minecraft:')
            and value.get('components', {}) == {}, 'Unsupported saved stack/component patch')
    count = typed_int(value.get('count', atom('Int', 1)), 'Int', 1, 64)
    return stack(value['id'][10:], count)


def view(received: dict, items: dict) -> dict:
    active = received.get('active')
    require(active in ('player', 'table'), 'Unknown active menu')
    menu, player = received.get(active), received.get('player')
    require(isinstance(menu, dict) and isinstance(player, dict)
            and menu['kind'] == active and menu['baseline_sequence'] is not None
            and len(menu['slots']) == len(player['slots']) == 46, 'Missing received full menu baseline')
    require((menu['container_id'] == 0) == (active == 'player'), 'Wrong menu identity')
    slots = [wire_stack(value, items) for value in menu['slots']]
    player_slots = [wire_stack(value, items) for value in player['slots']]
    require(player_slots[5:9] == [None] * 4, 'Armor outside ordinary crafting fixture')
    offset = 36 if active == 'player' else 37
    main = slots[offset:offset + 9] + slots[9 if active == 'player' else 10:offset]
    require(main == player_slots[36:45] + player_slots[9:36], 'Player/table inventory projection disagrees')
    return {'grid': slots[1:5 if active == 'player' else 10], 'result': slots[0],
            'cursor': wire_stack(received['cursor'], items), 'main': main, 'offhand': player_slots[45]}


def expected_view(kind: str, grid: dict[int, dict], *, result=None, cursor=None, main=None, offhand=None) -> dict:
    return {'grid': [grid.get(slot) for slot in range(1, 5 if kind == 'player' else 10)],
            'result': result, 'cursor': cursor, 'main': deepcopy(main) if main is not None else [None] * 36, 'offhand': offhand}


def item_totals(values: list) -> dict:
    totals = Counter()
    for value in values:
        if value is not None:
            require(set(value) == {'item', 'count'}, 'Nonplain semantic item')
            totals[value['item']] += value['count']
    return dict(sorted(totals.items()))


def owned_items(value: dict) -> dict:
    # Result slot is a recipe preview, not owned output.
    return item_totals(value['grid'] + value['main'] + [value['cursor'], value['offhand']])


def player_items(typed: dict) -> tuple[list, dict | None]:
    main = [None] * 36
    for entry in compounds(typed['Inventory']):
        slot = typed_int(entry.get('Slot'), 'Byte', 0, 35)
        require(main[slot] is None, 'Duplicate saved inventory slot')
        main[slot] = saved_stack({key: value for key, value in entry.items() if key != 'Slot'})
    equipment = typed.get('equipment', {})
    require(isinstance(equipment, dict) and set(equipment) <= {'offhand'}, 'Unexpected equipment/legacy offhand')
    offhand = saved_stack(equipment['offhand']) if 'offhand' in equipment else None
    return main, offhand


def crafted_counts(saved: dict) -> dict:
    entries = saved['data']['stats'].get('minecraft:crafted', {})
    return {name: entries.get('minecraft:' + name, 0) for name in CRAFTED}


def validate_outcome(actual: dict, expected: dict, drops: dict, expected_drops: dict,
                     before_stats: dict, after_stats: dict, awards: dict) -> None:
    require(actual == expected, {'expected': expected, 'received': actual})
    require(drops == expected_drops, {'expected_drops': expected_drops, 'received_drops': drops})
    require(set(before_stats) == set(after_stats) == set(CRAFTED), 'Incomplete crafted counters')
    delta = {name: after_stats[name] - before_stats[name] for name in CRAFTED}
    require(delta == {name: awards.get(name, 0) for name in CRAFTED}, {'crafted_delta': delta, 'expected_awards': awards})


def payload(evidence: dict) -> bytes:
    raw = base64.b64decode(evidence['payload_base64'], validate=True)
    require(0 < len(raw) <= 8 * 1024 * 1024 and hashlib.sha256(raw).hexdigest() == evidence.get('payload_sha256'),
            'Missing/changed raw packet evidence')
    return raw


def validate_fence(fence: dict, after_sequence: int) -> None:
    sent, response = fence['request'], fence['response']
    require(sent.get('written') is True and sent.get('packet_id') == 12 and sent.get('action_id') == 1
            and type(sent.get('after_sequence')) is int and sent['after_sequence'] >= after_sequence,
            'Not a subsequent REQUEST_STATS fence')
    require(response.get('packet_id') == 3 and type(response.get('sequence')) is int
            and response['sequence'] > sent['after_sequence'], 'Missing later same-stream AwardStats')
    raw = payload(response)
    require(raw[0] == 3 and response['payload_bytes'] == response['decoded_bytes'] == len(raw), 'Invalid AwardStats envelope')


def validate_click(phase: dict) -> None:
    request = phase['request']
    require(request.get('written') is True and request.get('changed_slots') == {}, 'Click needs zero changed predictions')
    before = request['before']
    menu = before[before['active']]
    action, button, slot = request['input'], request['button'], request['slot']
    require(action in ('pickup', 'quick_move', 'swap', 'throw') and type(button) is int
            and 0 <= button <= (8 if action == 'swap' else 1) and type(slot) is int and 0 <= slot < 46, 'Invalid click grammar')
    cursor = before['cursor']
    if cursor is None:
        carried = b'\x00'
    else:
        require(set(cursor) == {'item_id', 'count'} and all(type(cursor[k]) is int and cursor[k] > 0 for k in cursor), 'Nonplain pre-action cursor')
        carried = b'\x01' + protocol.varint(cursor['item_id']) + protocol.varint(cursor['count']) + b'\x00\x00'
    expected = (b'\x12' + protocol.varint(menu['container_id']) + protocol.varint(menu['state_id'])
                + struct.pack('>hB', slot, button) + bytes([{'pickup': 0, 'quick_move': 1, 'swap': 2, 'throw': 4}[action], 0]) + carried)
    require(payload(request) == expected, 'Request differs from received pre-action revision/cursor')
    validate_fence(phase['fence'], request['after_sequence'])
    wire = phase['wire']
    require(all(payload(row)[0] == row['packet_id'] for row in wire), 'Raw packet IDs/hashes disagree')
    sent = [index for index, row in enumerate(wire) if row.get('direction') == 'serverbound' and row.get('packet_id') == 18]
    fences = [index for index, row in enumerate(wire) if row.get('direction') == 'serverbound' and row.get('packet_id') == 12]
    replies = [index for index, row in enumerate(wire) if row.get('direction') == 'clientbound'
               and row.get('sequence') == phase['fence']['response']['sequence']]
    require(len(sent) == len(fences) == len(replies) == 1 and sent[0] < fences[0] < replies[0], 'Missing ordered same-connection raw click/fence')
    require(payload(wire[sent[0]]) == expected and payload(wire[fences[0]]) == b'\x0c\x01'
            and payload(wire[replies[0]]) == payload(phase['fence']['response']), 'Raw request/fence mismatch')
    require(len({row['pid'] for row in wire}) == 1, 'Wire events span different clients')
    raw_replies = {row['sequence'] for row in wire if row.get('direction') == 'clientbound' and row.get('packet_id') in (18, 20, 60, 98, 110)}
    require({row['sequence'] for row in phase['observations']} == raw_replies, 'Lost received menu/cursor/player update evidence')
    # Rejections can legitimately have no inventory delta. Their fence plus
    # saved/live semantic checks establish the bounded unchanged outcome.
    require(phase.get('received') == phase['fence']['received'], 'Changed post-fence derived view')
    require(phase['observations'] or phase['received'] == before, 'A changed view needs actual received updates')


def validate_drop_entities(entities: list[dict]) -> tuple[dict, list[str]]:
    require(len(entities) <= 16, 'Drop entity budget exceeded')
    identities, items = [], []
    for entity in entities:
        require(entity.get('id') == 'minecraft:item', 'Non-item drop evidence')
        identity = entity.get('UUID')
        require(isinstance(identity, dict) and set(identity) == {'nbt_type', 'value'} and identity['nbt_type'] == 'IntArray'
                and isinstance(identity['value'], list) and len(identity['value']) == 4
                and all(type(value) is int and -2**31 <= value < 2**31 for value in identity['value']), 'Invalid drop UUID')
        identities.append(str(uuid.UUID(bytes=struct.pack('>4i', *identity['value']))))
        position = entity.get('Pos')
        require(isinstance(position, dict) and position.get('nbt_type') == 'List' and position.get('element_type') == 6
                and len(position.get('value', [])) == 3, 'Invalid typed drop position')
        xyz = []
        for coordinate in position['value']:
            require(isinstance(coordinate, dict) and coordinate.get('nbt_type') == 'Double'
                    and type(coordinate.get('value')) in (int, float), 'Invalid drop coordinate')
            xyz.append(coordinate['value'])
        require(4096 <= xyz[0] < 4112 and 196 <= xyz[1] < 213 and 0 <= xyz[2] < 16, 'Drop outside retained chunk256,0')
        items.append(saved_stack(entity.get('Item')))
    require(len(set(identities)) == len(identities), 'Duplicate saved drop UUID')
    return item_totals(items), identities


def validate_case(case: dict) -> None:
    require(case.get('result') == 'passed' and case.get('evidence_retained') is True, 'Case did not retain/pass evidence')
    phases = case.get('phases', [])
    require(phases and all(phase.get('complete') is True for phase in phases), 'Missing/incomplete phase')
    for phase in phases:
        if phase['kind'] == 'click':
            validate_click(phase)
    clicks = [(p['purpose'], p['request']['slot'], p['request']['input'], p['request']['button'])
              for p in phases if p['kind'] == 'click']
    require(clicks == required_clicks(case['name']), 'Missing/reordered required crafting action')
    kind, grid, _ = recipe_fixture(case['name'])
    opens = [p for p in phases if p['kind'] == 'table_open']
    require(len(opens) == (1 if kind == 'table' else 0), 'Missing/unexpected real table open')
    if opens:
        opened = opens[0]
        validate_fence(opened['fence'], opened['after_sequence'])
        screens = [p for p in opened['packets'] if p['kind'] == 'open']
        require(len(screens) == 1 and screens[0]['menu_type'] == 12 and screens[0]['container_id'] > 0, 'Wrong received table screen')
        require(any(p['kind'] == 'content' and p['container_id'] == screens[0]['container_id']
                    and len(p['slots']) == 46 for p in opened['packets']), 'Missing received table content')
        wire = opened['wire']
        require(any(p['direction'] == 'serverbound' and p['packet_id'] == 66 and payload(p)[0] == 66 for p in wire)
                and any(p['direction'] == 'clientbound' and p['sequence'] == screens[0]['sequence'] and payload(p)[0] == 60 for p in wire),
                'Missing actual use_item_on/open wire evidence')
    placements = [p for p in phases if p.get('purpose') == 'grid_setup']
    require([(p['request']['slot'], p['request']['button']) for p in placements] == [tuple(pair) for pair in case['placements']]
            == [(slot, index) for index, slot in enumerate(grid)], 'Missing actual grid SWAP phase')
    require(all(p['request']['input'] == 'swap' for p in placements), 'Grid injected without SWAP2')
    checks = case.get('checks', [])
    require(checks and all(check.get('validated') is True for check in checks), 'Missing outcome checkpoint')
    labels = ['prepared', 'cursor_full', 'rejected', 'deposited', 'extracted'] if case['name'] == 'full_cursor' else (
        ['prepared', 'rejected', 'extracted'] if case['name'] == 'cake_shift' else ['prepared', 'extracted'])
    require([check['label'] for check in checks] == labels, 'Missing required outcome/rejection phase')
    for check in checks:
        validate_outcome(check['actual'], check['expected'], check['drops'], check['expected_drops'],
                         check['before_stats'], check['after_stats'], check['awards'])
    timeline = case.get('timeline', [])
    require('evidence_retained' in timeline and 'assertions_passed' in timeline
            and timeline.index('evidence_retained') < timeline.index('assertions_passed'), 'Assertions preceded evidence retention')
    if 'cleanup_started' in timeline:
        require('failure' not in timeline and timeline.index('assertions_passed') < timeline.index('cleanup_started')
                and timeline[-1] == 'cleanup_complete', 'Cleanup altered failed/unretained expectations')


def validate_campaign(report: dict) -> None:
    require(report.get('completed_boots') == list(BOOTS) and list(report.get('boots', {})) == list(BOOTS), 'Requires both ordered fresh boots')
    require(report.get('provenance_checked_before') is True and report.get('provenance_checked_after') is True,
            'Missing source/binary/JAR/primary pins')
    source, binary, official, java, contract = (report.get(key, {}) for key in ('source', 'binary', 'official', 'java', 'contract'))
    require(source.get('dirty') is False and re.fullmatch(r'[0-9a-f]{40}', source.get('commit', ''))
            and re.fullmatch(r'[0-9a-f]{40}', binary.get('source_commit', ''))
            and re.fullmatch(r'[0-9a-f]{64}', binary.get('sha256', ''))
            and official.get('inner_sha256') == INNER_JAR_SHA256 and java.get('sha256') == JDK_SHA256,
            'Missing actual source/binary/JAR/JDK provenance')
    require({name: value.get('sha256') for name, value in contract.get('primary', {}).items()} == PRIMARY
            and contract.get('reports') == REPORT_PINS and contract.get('wire_original_result') == 'failed'
            and contract.get('wire_post_validation_result') == 'passed' and report.get('manifest'), 'Missing exact primary/source manifest')
    require(report.get('cleanup_complete') is True and not report.get('runtime_errors'), 'Unclean campaign lifecycle')
    require(not report.get('world_transfers'), 'This campaign requires fresh worlds, not interchange')
    previous_stop = None
    for role in BOOTS:
        boot = report['boots'][role]
        require(boot.get('result') == 'passed' and boot.get('engine') == ('vanilla' if role == BOOTS[0] else 'rust'), 'Wrong boot engine/result')
        clients, stop = boot.get('clients', []), boot.get('teardown', {})
        require(len(clients) == 1 and clients[0].get('name') == NAME and '--crafting' in clients[0].get('argv', []), 'One actual crafting client required')
        require(stop.get('boot') == role and stop.get('exit_code') == 0 and stop.get('forced_stop') is False
                and not stop.get('unexpected_exit') and len(stop.get('clients', [])) == 1, 'Unclean server teardown')
        require(stop.get('pid') and stop.get('argv') and stop.get('started') and stop.get('stopped')
                and stop['started'] < stop['stopped'] and (previous_stop is None or previous_stop <= stop['started']), 'Missing/nonsequential owned process interval')
        previous_stop = stop['stopped']
        client = stop['clients'][0]
        require(client.get('pid') == clients[0].get('pid') and client.get('exit_code') == 0
                and client.get('forced_close') is False and client.get('failure') is None, 'Unclean client teardown')
        require(boot.get('saved') and list(boot.get('cases', {})) == list(CASES), 'Missing stopped inputs/cases')
        for case in boot['cases'].values():
            validate_case(case)
            require(case['timeline'][-1] == 'cleanup_complete', 'Incomplete case reset')
    require(report.get('comparison') == 'equal', 'Missing paired semantic comparison')


def campaign_rows(report: dict) -> list[dict]:
    try:
        validate_campaign(report)
        result, errors = 'passed', []
    except (AssertionError, KeyError, TypeError, ValueError) as error:
        result, errors = 'failed', [str(error)]
    return [{'id': 'crafting.' + name, 'features': [], 'result': result, 'errors': errors,
             'composite': 'both fresh ordered boots, every action fence, retained outcomes, pins and clean shutdown'} for name in CASES]


def recipe_fixture(name: str) -> tuple[str, dict[int, dict], dict]:
    kind = 'table' if name.startswith('table_') or name == 'cake_shift' else 'player'
    if name == 'cake_shift':
        grid = {1: stack('milk_bucket'), 2: stack('milk_bucket'), 3: stack('milk_bucket'),
                4: stack('sugar'), 5: stack('egg'), 6: stack('sugar'),
                7: stack('wheat'), 8: stack('wheat'), 9: stack('wheat')}
        return kind, grid, stack('cake')
    if name.endswith('four_honey'):
        return kind, {slot: stack('honey_bottle') for slot in ((1, 2, 3, 4) if kind == 'player' else (5, 6, 8, 9))}, stack('honey_block')
    count = 3 if name == 'throw_all' else 2 if name in ('throw_once', 'offhand_partial', 'offhand_spill', 'selected_before_offhand') else 1
    return kind, {9 if kind == 'table' else 4: stack('honey_bottle', count)}, stack('sugar', 3)


def required_clicks(name: str) -> list[tuple[str, int, str, int]]:
    kind, grid, _ = recipe_fixture(name)
    placements = [('grid_setup', slot, 'swap', hotbar) for hotbar, slot in enumerate(grid)]
    if name == 'full_cursor':
        return placements + [('fill_cursor', 37, 'pickup', 0), ('reject_cursor', 0, 'pickup', 0),
                             ('deposit_cursor', 37, 'pickup', 0), ('extract', 0, 'pickup', 0)]
    if name == 'cake_shift':
        return placements + [('reject_shift', 0, 'quick_move', 0), ('extract', 0, 'quick_move', 0)]
    action = 'throw' if name in ('throw_once', 'throw_all', 'offhand_partial', 'offhand_spill', 'selected_before_offhand') else 'pickup'
    button = 1 if name.endswith('_right') or name == 'throw_all' else 0
    return placements + [('extract', 0, action, button)]


class Probe:
    """One boot's bounded case evidence, using the existing owned lifecycle."""
    def __init__(self, fixture: Fixture, client: Client, contract: dict):
        self.fixture, self.client, self.contract = fixture, client, contract
        self.server = fixture.server
        self.items = contract['items']
        self.case: dict = {}
        self.serial = 0
        self.baseline = {name: 0 for name in CRAFTED}  # Replaced by actual saved baseline before any action.

    def retain(self) -> None:
        write_json(self.server.directory / (self.case['name'] + '.evidence.json'), self.case)

    def wire_counts(self) -> dict:
        result = {}
        for name in CRAFTED:
            observed = self.client.statistics_values[(self.contract['crafted_category'], self.items[name])]
            result[name] = observed['entry']['value'] if observed is not None else self.baseline[name]
        return result

    def click(self, purpose: str, slot: int, action: str, button: int = 0) -> dict:
        phase = {'kind': 'click', 'purpose': purpose, 'complete': False}
        self.case['phases'].append(phase)
        self.retain()
        mark = len(self.client.events)
        try:
            with interaction_deadline(self.server, self.case['name'] + ':' + purpose):
                phase['request'] = self.client.crafting_click(slot, action, button)
                self.retain()  # Queued request survives a missing or stuck fence.
                phase['fence'] = self.client.crafting_fence()
                fence = phase['fence']
                phase['received'] = fence['received']
                phase['observations'] = [row for row in fence['observations'] if row['sequence'] > phase['request']['after_sequence']]
                phase['wire'] = [row for row in self.client.events[mark:] if row.get('event') == 'wire_packet'
                                 and (row['direction'] == 'serverbound' or
                                      phase['request']['after_sequence'] < row['sequence'] <= fence['after_sequence'])]
                phase['crafted_counts'] = self.wire_counts()
                self.retain()
                validate_click(phase)
                phase['complete'] = True
                return phase
        finally:
            self.retain()

    def inventory(self, entries: dict[int, dict | None]) -> None:
        """Command-write only ordinary player hotbar/main/offhand fixture slots."""
        phase = {'kind': 'inventory_setup', 'entries': entries, 'commands': [], 'complete': False}
        self.case['phases'].append(phase)
        try:
            with interaction_deadline(self.server, self.case['name'] + ':inventory_setup'):
                for index, value in entries.items():
                    require(index in range(36) or index == 40, 'No command injection into crafting slots')
                    slot = f'hotbar.{index}' if index < 9 else f'inventory.{index - 9}' if index < 36 else 'weapon.offhand'
                    item = 'minecraft:air' if value is None else value['item'] + ' ' + str(value['count'])
                    command = f'item replace entity {NAME} {slot} with {item}'
                    phase['commands'].append({'command': command, 'output': self.server.command(command)})
                def observed(state: dict) -> bool:
                    actual = view(state['crafting'], self.items)
                    return all((actual['offhand'] if index == 40 else actual['main'][index]) == value
                               for index, value in entries.items())
                self.client.wait_state(observed)
                phase['fence'] = self.client.crafting_fence()
                self.retain()
                actual = view(phase['fence']['received'], self.items)
                for index, value in entries.items():
                    require((actual['offhand'] if index == 40 else actual['main'][index]) == value,
                            {'command_inventory_index': index, 'expected': value, 'received': actual})
                phase['complete'] = True
        finally:
            self.retain()

    def capture(self, label: str) -> dict:
        """Retain raw saved inputs before semantic assertions or fixture reset."""
        self.serial += 1
        stem = self.server.directory / f'evidence-{self.serial:03d}-{label}'
        evidence = {'label': label, 'stem': str(stem)}
        self.case.setdefault('snapshots', []).append(evidence)
        try:
            with interaction_deadline(self.server, self.case['name'] + ':' + label):
                require_frozen(self.server)
                evidence['time_before'] = suite.game_time(self.server)
                evidence['fence'] = self.client.crafting_fence()
                evidence['wire_counts'] = self.wire_counts()
                self.server.save_flush()
                evidence['saved'] = retained_player(self.server.directory / 'world', stem)
                # Preserve all live entities, including unexpected drops, before
                # comparing their saved UUIDs/typed Item counts or invoking kill.
                evidence['live_items'] = self.server.command(f'execute as {DROPS} run data get entity @s')
                marker = 'CRAFT_DROPS_' + uuid.uuid4().hex
                evidence['presence'] = self.server.command(f'execute if entity {DROPS} run say {marker}')
                present = any(suite.is_server_say(line, marker) for line in evidence['presence'])
                if present:
                    evidence['entities'] = read_fixture(self.server.directory / 'world', stem.with_suffix('.entities.nbt'), entities=True)
                    drops = [entity for entity in compounds(evidence['entities']['typed']['Entities']) if entity.get('id') == 'minecraft:item']
                else:
                    drops = []
                evidence['drops'] = drops
                self.retain()
                totals, identities = validate_drop_entities(drops)
                require(present == bool(drops), 'Live/saved drop presence disagrees')
                self.server.command('scoreboard players set CraftDrops pdprobe 0')
                evidence['live_count_command'] = self.server.command(
                    f'execute as {DROPS} run scoreboard players add CraftDrops pdprobe 1')
                evidence['live_count'] = typed_predicate(self.server, f'if score CraftDrops pdprobe matches {len(drops)}')
                evidence['live_bindings'] = []
                for identity, entity in zip(identities, drops):
                    evidence['live_bindings'].append({'uuid': identity, 'saved_uuid': entity['UUID'],
                        'full_live': self.server.command(f'data get entity {identity}'),
                        'item': typed_predicate(self.server,
                            f'if data entity {identity} ' + typed_snbt({'Item': entity['Item']}))})
                evidence['drop_totals'] = totals
                evidence['actual'] = view(evidence['fence']['received'], self.items)
                evidence['crafted_counts'] = crafted_counts(evidence['saved']['statistics'])
                evidence['time_after'] = suite.game_time(self.server)
                self.retain()
                require(evidence['time_before'] == evidence['time_after'], 'Frozen drop interval advanced world time')
                main, offhand = player_items(evidence['saved']['player']['typed'])
                require(evidence['saved']['player']['typed'].get('SelectedItemSlot') == atom('Int', 0), 'Fixture requires selected hotbar0')
                require(main == evidence['actual']['main'] and offhand == evidence['actual']['offhand'], 'Saved/player received inventory differs')
                require(evidence['crafted_counts'] == evidence['wire_counts'], 'Saved named stats disagree with retained absolute wire values')
                return evidence
        finally:
            self.retain()

    def check(self, label: str, expected: dict, *, drops: dict | None = None, awards: dict | None = None) -> dict:
        evidence = self.capture(label)
        check = {'label': label, 'actual': evidence['actual'], 'expected': expected,
                 'drops': evidence['drop_totals'], 'expected_drops': drops or {}, 'before_stats': self.baseline,
                 'after_stats': evidence['crafted_counts'], 'awards': awards or {}, 'validated': False,
                 'snapshot': evidence['stem']}
        self.case['checks'].append(check)
        self.retain()
        validate_outcome(check['actual'], expected, check['drops'], check['expected_drops'],
                         self.baseline, check['after_stats'], check['awards'])
        check['validated'] = True
        self.retain()
        return evidence

    def prepare(self, name: str) -> tuple[str, dict, dict]:
        kind, grid, result = recipe_fixture(name)
        self.case = {'name': name, 'result': 'failed', 'phases': [], 'checks': [], 'timeline': [],
                     'placements': [(slot, index) for index, slot in enumerate(grid)]}
        self.fixture.report['boots'][self.fixture.report['active_boot']]['cases'][name] = self.case
        self.retain()
        # The independently saved fresh/reset state anchors omitted zero stats.
        self.server.save_flush()
        initial = retained_player(self.server.directory / 'world', self.server.directory / (name + '-baseline'))
        self.case['baseline_saved'] = initial
        self.baseline = crafted_counts(initial['statistics'])
        for counter in CRAFTED:
            self.client.watch_statistic(self.contract['crafted_category'], self.items[counter])
        baseline = self.capture('baseline')
        require(baseline['actual'] == expected_view('player', {}) and not baseline['drop_totals'], 'Case began with residual items')
        if kind == 'table':
            before = self.client.state()['packets']
            mark = len(self.client.events)
            phase = {'kind': 'table_open', 'after_sequence': before, 'complete': False}
            self.case['phases'].append(phase)
            with interaction_deadline(self.server, name + ':open'):
                phase['request'] = self.client.request('use_item_on', **TABLE)
                state = self.client.wait_state(lambda state: state['crafting']['active'] == 'table'
                    and state['crafting']['table']['slots'] is not None)
                phase['packets'] = [p for p in state['crafting_packets'] if p['sequence'] > before]
                phase['fence'] = self.client.crafting_fence()
                phase['wire'] = [row for row in self.client.events[mark:] if row.get('event') == 'wire_packet'
                                 and (row['direction'] == 'serverbound' or before < row['sequence'] <= phase['fence']['after_sequence'])]
                self.retain()
                opens = [p for p in phase['packets'] if p['kind'] == 'open']
                require(len(opens) == 1 and opens[0]['menu_type'] == 12
                        and any(p['kind'] == 'content' and p['container_id'] == opens[0]['container_id'] for p in phase['packets']), 'Missing actual table open/content')
                require(view(phase['fence']['received'], self.items) == expected_view('table', {}), 'Nonempty fresh table')
                phase['complete'] = True
        self.inventory({index: value for index, value in enumerate(grid.values())})
        for index, slot in enumerate(grid):
            phase = self.click('grid_setup', slot, 'swap', index)
            before = view(phase['request']['before'], self.items)
            after = view(phase['received'], self.items)
            expected = deepcopy(before)
            expected['grid'][slot - 1], expected['main'][index] = before['main'][index], before['grid'][slot - 1]
            # Intermediate recipe previews are observed, not predicted.
            require({key: value for key, value in after.items() if key != 'result'} ==
                    {key: value for key, value in expected.items() if key != 'result'}, 'SWAP setup changed unrelated slots/cursor')
            require(phase['crafted_counts'] == self.baseline, 'Grid setup advanced crafted statistics')
        self.check('prepared', expected_view(kind, grid, result=result))
        return kind, grid, result

    def reset(self) -> None:
        validate_case(self.case)
        require('failure' not in self.case['timeline'], 'Cannot clean failed expectations')
        self.case['timeline'].append('cleanup_started')
        self.retain()
        with interaction_deadline(self.server, self.case['name'] + ':reset'):
            before = self.case['checks'][-1]['actual']
            evidence = self.case['reset'] = {'before': before}
            evidence['close'] = self.client.request('close_menu')
            def evacuated_view(state: dict) -> bool:
                current = view(state['crafting'], self.items)
                return current['grid'] == [None] * 4 and current['result'] is None and current['cursor'] is None
            self.client.wait_state(evacuated_view)
            evidence['closed_fence'] = self.client.crafting_fence()
            self.retain()
            evacuated = self.capture('evacuated')
            closed = evacuated['actual']
            before_total = Counter(owned_items(before)) + Counter(self.case['checks'][-1]['drops'])
            after_total = Counter(owned_items(closed)) + Counter(evacuated['drop_totals'])
            require(closed['grid'] == [None] * 4 and closed['result'] is None and closed['cursor'] is None
                    and before_total == after_total, 'Menu evacuation lost/duplicated grid or cursor items')
            evidence['clear'] = self.server.command(f'clear {NAME}')
            # Only already validated drops may be removed. Never run this path
            # after failure, including a timeout in Throw1.
            evidence['kill'] = self.server.command(f'kill {DROPS}')
            evidence['no_drops'] = typed_predicate(self.server, f'unless entity {DROPS}')
            self.client.wait_state(lambda state: view(state['crafting'], self.items) == expected_view('player', {}))
            evidence['fence'] = self.client.crafting_fence()
            self.retain()
            require(view(evidence['fence']['received'], self.items) == expected_view('player', {}), 'Reset left residual inventory/grid/cursor')
        self.case['timeline'].append('cleanup_complete')
        self.retain()

    def exercise(self, name: str) -> None:
        try:
            kind, grid, output = self.prepare(name)
            if name == 'full_cursor':
                self.inventory({1: stack('sugar', 64)})
                self.click('fill_cursor', 37, 'pickup')
                self.check('cursor_full', expected_view(kind, grid, result=output, cursor=stack('sugar', 64)))
                self.click('reject_cursor', 0, 'pickup')
                self.check('rejected', expected_view(kind, grid, result=output, cursor=stack('sugar', 64)))
                self.click('deposit_cursor', 37, 'pickup')
                main = [None] * 36
                main[1] = stack('sugar', 64)
                self.check('deposited', expected_view(kind, grid, result=output, main=main))
                self.click('extract', 0, 'pickup')
                self.check('extracted', expected_view(kind, {4: stack('glass_bottle')}, cursor=output, main=main), awards={'sugar': 3})
            elif name == 'cake_shift':
                full = [stack('cobblestone', 64) for _ in range(36)]
                self.inventory({**dict(enumerate(full)), 40: stack('cobblestone', 64)})
                self.click('reject_shift', 0, 'quick_move')
                self.check('rejected', expected_view(kind, grid, result=output, main=full, offhand=stack('cobblestone', 64)))
                self.inventory({8: None})
                self.click('extract', 0, 'quick_move')
                main = deepcopy(full)
                main[8] = output
                self.check('extracted', expected_view(kind, {1: stack('bucket'), 2: stack('bucket'), 3: stack('bucket')},
                    main=main, offhand=stack('cobblestone', 64)), awards={'cake': 1})
            elif name in ('offhand_partial', 'offhand_spill'):
                full = [stack('cobblestone', 64) for _ in range(36)]
                self.inventory({**dict(enumerate(full)), 40: stack('glass_bottle', 63 if name == 'offhand_partial' else 64)})
                self.click('extract', 0, 'throw')
                drops = {'minecraft:sugar': 3}
                if name == 'offhand_spill':
                    drops['minecraft:glass_bottle'] = 1
                self.check('extracted', expected_view(kind, {4: stack('honey_bottle')}, result=output, main=full,
                    offhand=stack('glass_bottle', 64)), drops=drops, awards={'sugar': 3})
            elif name == 'selected_before_offhand':
                self.inventory({0: stack('glass_bottle', 62), 9: stack('glass_bottle', 63), 40: stack('glass_bottle', 63)})
                self.click('extract', 0, 'throw')
                main = [None] * 36
                main[0], main[9] = stack('glass_bottle', 63), stack('glass_bottle', 63)
                self.check('extracted', expected_view(kind, {4: stack('honey_bottle')}, result=output, main=main,
                    offhand=stack('glass_bottle', 63)), drops={'minecraft:sugar': 3}, awards={'sugar': 3})
            elif name in ('throw_once', 'throw_all'):
                self.click('extract', 0, 'throw', 0 if name == 'throw_once' else 1)
                main = [None] * 36
                main[0] = stack('glass_bottle', 1 if name == 'throw_once' else 2)
                once = name == 'throw_once'
                self.check('extracted', expected_view(kind, {4: stack('honey_bottle') if once else stack('glass_bottle')},
                    result=output if once else None, main=main), drops={'minecraft:sugar': 3 if once else 9}, awards={'sugar': 3 if once else 9})
            else:
                self.click('extract', 0, 'pickup', 1 if name.endswith('_right') else 0)
                bottles = {slot: stack('glass_bottle') for slot in grid}
                self.check('extracted', expected_view(kind, bottles, cursor=output),
                           awards={'honey_block': 1} if name.endswith('four_honey') else {'sugar': 3})
            self.case['evidence_retained'] = True
            self.case['timeline'].extend(['evidence_retained', 'assertions_passed'])
            self.case['result'] = 'passed'
            self.retain()
            self.reset()
        except Exception as error:
            self.case['result'] = 'failed'
            self.case['timeline'].append('failure')
            self.case['error'] = f'{type(error).__name__}: {error}'
            self.retain()
            raise  # Stop before any reset can alter failed expectations.


def primary_contract(args: argparse.Namespace) -> dict:
    primary = {}
    for name, expected in PRIMARY.items():
        path = args.primary_root / name
        require(path.is_file() and digest(path) == expected, 'Changed/missing retained primary: ' + name)
        primary[name] = {'path': str(path), 'sha256': expected}
    reports = {}
    for name, expected in REPORT_PINS.items():
        path = args.reference_cache / 'generated/reports' / name
        require(digest(path) == expected, 'Changed pinned official report: ' + name)
        reports[name] = json.loads(read_bytes(path), object_pairs_hook=unique_object)
    registries, packets = reports['registries.json'], reports['packets.json']
    items = {name: registries['minecraft:item']['entries']['minecraft:' + name]['protocol_id'] for name in ITEMS}
    require(items['sugar'] == 1217 and items['glass_bottle'] == 1270, 'Changed primary-derived unit byte witnesses')
    require(registries['minecraft:menu']['entries']['minecraft:crafting']['protocol_id'] == 12, 'Changed crafting menu type')
    category = registries['minecraft:stat_type']['entries']['minecraft:crafted']['protocol_id']
    require(category == 1, 'Changed crafted statistic category')
    for direction, name, identifier in (
            ('serverbound', 'container_click', 18), ('serverbound', 'container_close', 19),
            ('serverbound', 'client_command', 12), ('serverbound', 'use_item_on', 66),
            ('clientbound', 'container_set_content', 18), ('clientbound', 'container_set_slot', 20),
            ('clientbound', 'open_screen', 60), ('clientbound', 'set_cursor_item', 98),
            ('clientbound', 'set_player_inventory', 110), ('clientbound', 'award_stats', 3)):
        require(packets['play'][direction]['minecraft:' + name]['protocol_id'] == identifier, 'Changed packet ID: ' + name)
    original = json.loads(read_bytes(args.primary_root / 'r28-crafting-wire-primary-79a/run/receipt.json'))
    post = json.loads(read_bytes(args.primary_root / 'r28-crafting-wire-primary-79a/run/post-validation.json'))
    require(original['result'] == 'failed' and original['exit_code'] == 0 and post['result'] == 'passed'
            and post['original_result'] == 'failed' and len(post['class_sections']) == 12, 'Do not rewrite original primary failure')
    return {'primary': primary, 'reports': REPORT_PINS, 'items': items, 'crafted_category': category,
            'wire_original_result': original['result'], 'wire_post_validation_result': post['result'],
            'scope': 'static primary, not runtime gold or numeric component hashes'}


def manifest() -> dict:
    paths = [*sorted((oracle.ROOT / 'tools').glob('check_*.py')),
             oracle.ROOT / 'tests/protocol_client.test.ts', oracle.ROOT / 'tests/test_crafting.py']
    return {**suite.harness_manifest(), **{str(path.relative_to(oracle.ROOT)): digest(path) for path in paths}}


def comparable(boot: dict) -> dict:
    return {name: [{'label': check['label'], 'view': check['actual'], 'drops': check['drops'],
                    'crafted_delta': {key: check['after_stats'][key] - check['before_stats'][key] for key in CRAFTED}}
                   for check in case['checks']] for name, case in boot['cases'].items()}


def run_campaign(args: argparse.Namespace, run: Path, report: dict) -> None:
    report.update(boots={}, completed_boots=[], runtime_errors=[], cleanup_complete=False)
    for role, kind in zip(BOOTS, ('vanilla', 'rust')):
        report['active_boot'] = role
        fixture = Fixture(kind, run / role, args, report)
        server = fixture.server
        entry = report['boots'][role] = {'engine': kind, 'directory': str(server.directory), 'result': 'failed', 'cases': {}, 'clients': []}
        probe = None
        try:
            server.configure()
            fixture.start(role)
            entry['status'] = protocol.status_fields(server.port)
            require(entry['status']['protocol'] == 777, 'Live server protocol differs')
            server.command('tick freeze')
            require_frozen(server)
            server.command('forceload add 4096 0')
            suite.wait_loaded(server, x=4096, z=0)
            for command in ('fill 4096 199 0 4111 199 15 stone', 'difficulty peaceful',
                            'scoreboard objectives add pdprobe dummy', 'setblock 4104 200 6 crafting_table'):
                server.command(command)
            client = Client(server, log_name='crafting-wire', crafting=True, raw_packets=True)
            fixture.clients.append(client)
            entry['clients'].append({'name': client.name, 'pid': client.process.pid, 'argv': client.process.args})
            client.ready()
            client.wait_state(lambda state: state['crafting']['player'] is not None)
            server.command(f'gamemode survival {NAME}')
            entry['teleport'] = teleport_exact(server, client, POSITION)
            entry['identity'] = typed_predicate(server,
                f'if entity @a[name={NAME},nbt={{UUID:[I;{",".join(map(str, UUID_INTS))}],playerGameType:0,SelectedItemSlot:0}}]')
            client.wait_state(lambda state: any(packet['x'] == 256 and packet['z'] == 0 for packet in state['chunk_packets']))
            probe = Probe(fixture, client, report['contract'])
            for name in CASES:
                probe.exercise(name)
            entry['before_stop'] = probe.capture('before_stop')
            fixture.close(client)
            server.save_flush()
            fixture.stop()
            entry['teardown'] = deepcopy(fixture.processes[-1])
            entry['saved'] = retained_player(server.directory / 'world', server.directory / 'stopped')
            require(suite.data_version(server.directory / 'world') == 5023, 'Wrong stopped DataVersion')
            require(crafted_counts(entry['saved']['statistics']) == {'sugar': 36, 'honey_block': 2, 'cake': 1}, 'Wrong complete campaign crafted totals')
            require(player_items(entry['saved']['player']['typed']) == ([None] * 36, None), 'Stopped fixture retained unexpected player items')
            entry['result'] = 'passed'
        except Exception as error:
            entry['error'] = f'{type(error).__name__}: {error}'
            report['runtime_errors'].append(entry['error'])
            # Preserve failed live expectations before normal owned shutdown.
            # A broken server already has its watchdog/partial wire evidence.
            if probe is not None and probe.case and not server.broken and not probe.client.failure:
                try:
                    entry['failure_snapshot'] = probe.capture('failure')
                except Exception as capture_error:
                    entry['failure_capture_error'] = f'{type(capture_error).__name__}: {capture_error}'
            write_json(server.directory / 'boot-failure.json', entry)
            raise
        finally:
            errors = fixture.cleanup()
            report['runtime_errors'].extend(errors)
            if fixture.processes:
                entry['teardown'] = deepcopy(fixture.processes[-1])
            write_json(server.directory / 'boot-evidence.json', entry)
        require(not report['runtime_errors'], 'Owned cleanup failed')
        report['completed_boots'].append(role)
    left, right = (comparable(report['boots'][role]) for role in BOOTS)
    report['semantic_comparison'] = {'java': left, 'rust': right}
    write_json(run / 'semantic-comparison.json', report['semantic_comparison'])
    require(left == right, 'Paired crafting semantics differ; raw outcomes retained')
    report['comparison'] = 'equal'
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
    require(args.accept_eula, 'Explicit --accept-eula required for the isolated reference')
    require(args.root.resolve() == oracle.ROOT.resolve(), '--root must be this harness checkout')
    require(all(re.fullmatch(r'[0-9a-f]{40}', value) for value in (args.current_clean_commit, args.rust_source_commit)), 'Exact source hashes required')
    cache = (oracle.ROOT / '.cache').resolve()
    require(args.output.resolve().is_relative_to(cache) and re.fullmatch(r'crafting-[A-Za-z0-9_-]+\.json', args.output.name), 'Use fresh ignored .cache/crafting-*.json')
    require(not args.output.is_symlink() and not any(path.is_symlink() for path in args.output.absolute().parents
            if path.is_relative_to(cache)), 'Linked output refused')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    output = args.output.open('x')
    runs = cache / 'runs'
    runs.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix='crafting-', dir=runs))
    report = {'schema_version': 1, 'kind': 'crafting', 'result': 'failed', 'created_at': now(),
        'scope': __doc__, 'target': {'version': '26.3', 'protocol': 777, 'data_version': 5023},
        'environment': {'host': socket.gethostname(), 'checkout': str(oracle.ROOT), 'fixture': str(run),
            'binding': '127.0.0.1 isolated', 'workers': 2, 'startup_seconds': 180, 'interaction_seconds': 20},
        'identity': {'name': NAME, 'offline_uuid': str(PLAYER_ID)}, 'gaps': GAPS, 'processes': {},
        'provenance_checked_before': False, 'provenance_checked_after': False}
    began = time.monotonic()
    try:
        report['source'] = suite.git_state()
        require(report['source'] == {'commit': args.current_clean_commit, 'dirty': False}, 'Requires exact current clean commit')
        report['manifest'] = manifest()
        oracle.CACHE = args.reference_cache.resolve()
        oracle.acquire = suite.acquire = cached_reference  # Never download, including a missing-file race.
        official = cached_reference()
        require(digest(oracle.CACHE / 'minecraft.jar') == INNER_JAR_SHA256, 'Changed pinned inner JAR')
        args.java = str(Path(shutil.which(args.java) or args.java).resolve())
        require(digest(Path(args.java)) == JDK_SHA256, 'Requires pinned cached JDK25')
        report['java'] = {'path': args.java, 'sha256': JDK_SHA256}
        report['official'] = {'path': str(official), 'sha256': digest(official), 'inner_sha256': INNER_JAR_SHA256}
        report['contract'] = primary_contract(args)
        original = args.rust_binary.resolve()
        require(original.is_file() and os.access(original, os.X_OK), 'Existing ROOT-built Rust executable required')
        binary_hash = digest(original)
        args.rust_binary = suite.pin_binary(original, run)
        require(digest(original) == digest(args.rust_binary) == binary_hash, 'Binary changed during pin')
        report['binary'] = {'source': str(original), 'pinned': str(args.rust_binary), 'sha256': binary_hash,
                            'source_commit': args.rust_source_commit, 'association': 'caller attestation, not embedded provenance'}
        report['provenance_checked_before'] = True
        run_campaign(args, run, report)
        require(suite.git_state() == report['source'] and manifest() == report['manifest'], 'Source changed during campaign')
        require(primary_contract(args) == report['contract'], 'Primary/report inputs changed during campaign')
        require(digest(original) == digest(args.rust_binary) == binary_hash and digest(official) == report['official']['sha256']
                and digest(oracle.CACHE / 'minecraft.jar') == INNER_JAR_SHA256 and digest(Path(args.java)) == JDK_SHA256, 'Runtime inputs changed')
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
    print(f"crafting {report['result']}: {args.output}; artifacts {run}", flush=True)
    return 0 if report['result'] == 'passed' else 1


if __name__ == '__main__':
    raise SystemExit(main())
