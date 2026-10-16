#!/usr/bin/env python3
"""Opt-in real builtin22 jukebox campaign, exact Java26.3/protocol777/DV5023.

Four sequential boots: Java author -> Rust import -> Rust restart -> Java return.
Two real clients per boot; no downloads, builds, synthetic world writes or UUID
conversion. Whole stopped worlds are copied unchanged. All evidence is UNRUN
until this executable completes, including its final provenance/cleanup checks.
"""
from __future__ import annotations

import argparse
import base64
from copy import deepcopy
from functools import wraps
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import uuid
from typing import Callable

sys.dont_write_bytecode = True
import oracle
from check_block_data import close_menu, interaction_deadline
from check_container_metadata import Fixture, manifest as inherited_manifest, now, typed_predicate, wait_predicate
from check_goat_horns import PRIMARY as HORN_PRIMARY, REPORT_PINS
from check_item_interchange import INNER_JAR_SHA256, atom, compounds, typed_int, typed_snbt, verify_artifacts
from check_player_data import (JDK_SHA256, NAME, PLAYER_ID, UUID_INTS, PlayerReader,
    cached_reference, clone_world, command_result, identity_witness, read_bytes,
    read_player, read_statistics, teleport_exact, unique_object, world_hashes, write_json)
from parity import protocol, suite, worlds
from parity.bot import Client, delivered_chat
from parity.furnaces import digest, require_frozen, step_exact
from parity.protocol import require

# JukeboxSongs.bootstrap properties, not dynamic numeric registry IDs.
PROFILES = dict(zip(('11', '13', '5', 'blocks', 'bounce', 'cat', 'chirp', 'creator',
    'creator_music_box', 'far', 'lava_chicken', 'mall', 'mellohi', 'otherside',
    'pigstep', 'precipice', 'relic', 'stal', 'strad', 'tears', 'wait', 'ward'),
    ((71, 11), (178, 1), (178, 15), (345, 3), (234, 8), (185, 2), (185, 4),
     (176, 12), (73, 11), (174, 5), (134, 9), (197, 6), (96, 7), (195, 14),
     (149, 13), (299, 13), (218, 14), (150, 8), (188, 9), (175, 10), (238, 12), (251, 10))))
BUILTINS = tuple('minecraft:' + name for name in PROFILES)
DEFAULT, CAT = 'minecraft:11', 'minecraft:cat'
COMPONENT = 'minecraft:jukebox_playable'
ITEM, ITEM_ID = 'minecraft:music_disc_11', 1474
OBSERVER = 'DiscObserver'
CHEST, JUKEBOX = (4097, 200, 1), (4100, 200, 1)
COMPARATOR, WIRE = (4100, 200, 2), (4101, 200, 1)
POS = {'x': 4098.5, 'y': 200.0, 'z': 3.5, 'yaw': 90.0, 'pitch': 0.0}
# These distinct Java-authored fixture intents are inspected before any transfer.
# Java may omit finished elapsed when serializing. Never relabel that as a saved
# finished Long or claim a reload of a tag that was not actually persisted.
RESTORES = {'absent': (4104, 200, 8), 'zero': (4106, 200, 8),
            'negative': (4108, 200, 8), 'finished': (4110, 200, 8)}
BOOTS = ('java_source', 'rust_import', 'rust_reload', 'java_return')
CASES = ('jukeboxes.registry_eight_connections', 'jukeboxes.java_prediction_acceptance',
    'jukeboxes.builtin22_transfer', 'jukeboxes.selected_both_hands',
    'jukeboxes.survival_creative', 'jukeboxes.level_events_owner_observer',
    'jukeboxes.default_removal_opaque', 'jukeboxes.disc11_tick1441',
    'jukeboxes.chunk_live_privacy', 'jukeboxes.record_elapsed_continuation',
    'jukeboxes.rust_restart', 'jukeboxes.java_return')
GAPS = [
    'No full-parity or verified-family claim; source/helper tests are not real runtime observations.',
    'Numeric Java HashOps output remains inferential from actual accepted predicted SWAP plus authoritative reopen and same-stream statistics fence.',
    'All22 effective song identities, 21 nondefault patches; default11 patch elision and Identifier aliases are the only item normalizations.',
    'Selected11 andCat gameplay exercises both hands and survival/creative; only11 reaches its natural duration. No all22 natural-duration or rendering claim.',
    'Unknown/custom/overridden songs and direct stream0 are unsupported. Opaque custom_data controls use selected slots, never predicted opaque hashes.',
    'Advancement criteria, sculk/vibration dispatch and elapsed-only reentrant audio timing remain OPEN.',
    'No plugin parity claim: Rust commits elapsed+1 before guard-free notifications; vanilla periodic notification precedes increment.',
    'Missing block-entity insertion is not reachable through this vanilla command/wire setup without an artificial hook; native source tests do not establish paired runtime acceptance.',
    'Recipient checks cover the owner and one nearby observer in the current overworld; no radius boundary or other-dimension exclusion claim.',
    'Finished/absent elapsed fixture intents retain actual Java serialization. If a tag is omitted by Java, no saved-finished-Long transfer claim is made.',
    'The executable is hash-pinned; its source-commit association is supplied by the caller, not embedded in or independently recovered from the binary.',
]
PRIMARY = {**HORN_PRIMARY,
    'r26-jukebox-primary/Jukebox.javap.txt': 'a5e00dc33c14550b1f70c88c525d7fad45eb702f0b70f217ead439e814f7fa6c',
    'r26-jukebox-primary/receipt.json': '83d81e6de22450cbb3c3a0fb9fd4d7264819d860617ed769485a49ca025b0015',
    'r26-jukebox-parent-primary/JukeboxParents.javap.txt': '4bbc9cc360139608bacf57adfc7676954c8beda7542114458248a43974ca6340',
    'r26-jukebox-parent-primary/receipt.json': '872c12fb5a89ac0f44016cf7545eb189d32fca5baff16090eb99d8123db3c5b5',
    'r26-jukebox-base-primary/JukeboxBase.javap.txt': 'aa7cc017055b15ba2a8eb783db10d433eebece0268a0c5c64af3b2676731c162',
    'r26-jukebox-base-primary/receipt.json': 'ed5e2c8744ffc27aa43f701c04eaf35a9e3920e4ea1d088781d0f356fb84085a',
    'r26-jukebox-packet-primary/JukeboxPackets.javap.txt': '6beb2933a97084fe16e3b60269826d69b4e5b5b544e5792de0ced4d16607468d',
    'r26-jukebox-packet-primary/receipt.json': '4a8d52ea1413acb285d5710ade5d30ff3add3b9f3bac2c67c479f7a66a219152',
    'r26-jukebox-hit-primary/JukeboxHit.javap.txt': '7d3da4caf09a68f9f60d90801fed6b6f24f0397b5ba3855e66f9d02cfa677a18',
    'r26-jukebox-hit-primary/receipt.json': '733b08dc344f0182a910acad7e1500739e0f08d6920ac9d2eb8583afe6e5351d',
}


def pos(value: tuple[int, int, int]) -> str:
    return ' '.join(map(str, value))


def bounded(action: Callable) -> Callable:
    @wraps(action)
    def guarded(fixture: Fixture, *args, **kwargs):
        with interaction_deadline(fixture.server, action.__name__):
            return action(fixture, *args, **kwargs)
    return guarded


def song_name(value: object) -> str:
    require(isinstance(value, str), 'JukeboxPlayable must be an Identifier STRING')
    canonical = 'minecraft' + value if value.startswith(':') else value if ':' in value else 'minecraft:' + value
    require(canonical in BUILTINS, 'Unsupported song namespace/reference')
    return canonical


def stack(song: str | None = None, *, removed: bool = False, opaque: bool = False) -> dict:
    components = {}
    if song is not None and song_name(song) != DEFAULT:
        components[COMPONENT] = song_name(song)
    if removed:
        require(song is None, 'Conflicting expected components')
        components['!' + COMPONENT] = {}
    if opaque:
        components['minecraft:custom_data'] = {'jukebox_playable': CAT}
    return {'id': ITEM, 'count': atom('Int', 1), **({'components': components} if components else {})}


def semantic_stack(value: object) -> dict:
    require(isinstance(value, dict) and set(value) <= {'id', 'count', 'components'}, 'Unknown/missing item fields')
    result = deepcopy(value)
    require(result.get('id') == ITEM, 'Wrong fixture item')
    typed_int(result.get('count'), 'Int', 1, 1)
    components = result.get('components', {})
    require(isinstance(components, dict) and set(components) <= {COMPONENT, '!' + COMPONENT, 'minecraft:custom_data'}, 'Unsupported component patch')
    require(not (COMPONENT in components and '!' + COMPONENT in components), 'Conflicting component addition/removal')
    if COMPONENT in components:
        components[COMPONENT] = song_name(components[COMPONENT])
        if components[COMPONENT] == DEFAULT:
            del components[COMPONENT]
    if '!' + COMPONENT in components:
        require(components['!' + COMPONENT] == {}, 'Invalid removal marker')
    if 'minecraft:custom_data' in components:
        require(components['minecraft:custom_data'] == {'jukebox_playable': CAT}, 'Unsupported opaque control')
    if not components:
        result.pop('components', None)
    return result


def validate_ejected_item(value: object, expected_disc: dict) -> None:
    require(semantic_stack(value) == semantic_stack(expected_disc), {'wrong_ejected_item': value, 'expected': expected_disc})


def validate_offhand_record(value: dict, expected_disc: dict, ticks: int | None) -> None:
    require(record_state(value) == {'record': semantic_stack(expected_disc), 'elapsed': ticks},
            {'offhand_changed_record': value, 'expected': expected_disc, 'expected_elapsed': ticks})


def item_slots(value: object, minimum: int, maximum: int) -> dict[int, dict]:
    result = {}
    for entry in compounds(value):
        entry = deepcopy(entry)
        slot = typed_int(entry.pop('Slot', None), 'Byte', minimum, maximum)
        require(slot not in result, 'Duplicate inventory slot')
        result[slot] = semantic_stack(entry)
    return result


def expected() -> tuple[dict[int, dict], dict[int, dict]]:
    return ({**{slot: stack(name) for slot, name in enumerate(BUILTINS)}, 22: stack()},
            {1: stack(opaque=True), 2: stack(removed=True)})


def wire_stack(value: dict | None) -> dict | None:
    if value is None:
        return None
    components = deepcopy(semantic_stack(value).get('components', {}))
    removed = components.pop('!' + COMPONENT, None) is not None
    return {'item_id': ITEM_ID, 'count': 1, **({'components': components} if components else {}),
            **({'removed_components': [COMPONENT]} if removed else {})}


def menu_semantics(slots: list) -> list:
    result = []
    require(isinstance(slots, list) and len(slots) in (46, 63), 'Wrong menu length')
    for value in slots:
        if value is None:
            result.append(None)
            continue
        require(isinstance(value, dict) and set(value) <= {'item_id', 'count', 'components', 'removed_components'}, 'Unknown wire stack fields')
        require(type(value.get('item_id')) is int and value['item_id'] == ITEM_ID and type(value.get('count')) is int and value['count'] == 1, 'Wrong wire item/count')
        components = deepcopy(value.get('components', {}))
        removed = value.get('removed_components', [])
        require(removed in ([], [COMPONENT]) and isinstance(components, dict), 'Invalid removals/components')
        if removed:
            require(COMPONENT not in components, 'Conflicting wire component')
            components['!' + COMPONENT] = {}
        result.append(wire_stack({'id': ITEM, 'count': atom('Int', 1), 'components': components}))
    return result


def menu_slots() -> list:
    chest, player = expected()
    slots = [None] * 63
    for slot, value in chest.items():
        slots[slot] = wire_stack(value)
    for slot, value in player.items():
        slots[54 + slot] = wire_stack(value)
    return slots


def validate_registry(value: dict) -> None:
    require(isinstance(value, dict) and value.get('registry') == 'minecraft:jukebox_song', 'Missing actual song registry')
    entries = value.get('entries')
    require(isinstance(entries, list) and len(entries) == 22, 'Expected22 sent entries')
    seen = set()
    for index, entry in enumerate(entries):
        require(isinstance(entry, dict) and type(entry.get('registry_id')) is int and type(entry.get('reference_id')) is int
                and entry['registry_id'] == index and entry['reference_id'] == index + 1, 'Invalid received registry order')
        name = song_name(entry.get('identifier'))
        require(name == entry['identifier'] and name not in seen, 'Duplicate/noncanonical sent song')
        seen.add(name)
        data = entry.get('data')
        seconds, comparator = PROFILES[name[10:]]
        require(isinstance(data, dict) and set(data) == {'sound_event', 'description', 'length_in_seconds', 'comparator_output'}, 'Unsupported song fields')
        require(type(data['length_in_seconds']) in (int, float) and math.isfinite(data['length_in_seconds']) and data['length_in_seconds'] == seconds
                and type(data['comparator_output']) in (int, float) and data['comparator_output'] == comparator
                and data['sound_event'] == 'minecraft:music_disc.' + name[10:], 'Unsupported song profile')
        description = data['description']
        require(isinstance(description, dict) and set(description) <= {'translate', 'type'}
                and description.get('translate') == 'jukebox_song.minecraft.' + name[10:]
                and description.get('type', 'translatable') == 'translatable', 'Unsupported song description')
    raw = base64.b64decode(value.get('payload_base64', ''), validate=True)
    require(0 < len(raw) <= 65536 and value.get('payload_bytes') == value.get('decoded_bytes') == len(raw)
            and hashlib.sha256(raw).hexdigest() == value.get('payload_sha256'), 'Missing/mismatched raw configuration evidence')


def elapsed(value: dict) -> int | None:
    return None if 'ticks_since_song_started' not in value else typed_int(value['ticks_since_song_started'], 'Long', -(2**63), 2**63 - 1)


def record_state(value: dict) -> dict:
    require(isinstance(value, dict) and value.get('id') == 'minecraft:jukebox', 'Wrong block entity')
    return {'record': semantic_stack(value['RecordItem']) if 'RecordItem' in value else None, 'elapsed': elapsed(value)}


def validate_events(events: list[dict], expected_events: list[int], registry: dict, song: str | None,
                    position: tuple[int, int, int] = JUKEBOX) -> None:
    require(len(events) == len(expected_events), {'expected': expected_events, 'actual': events})
    binding = next((e for e in registry['entries'] if e['identifier'] == song), None) if song else None
    for event, expected_event in zip(events, expected_events):
        require(type(event.get('packet_id')) is int and type(event.get('event')) is int
                and all(type(event.get(axis)) is int for axis in ('x', 'y', 'z'))
                and event['packet_id'] == 47 and event['event'] == expected_event and event.get('global_event') is False
                and tuple(event.get(axis) for axis in ('x', 'y', 'z')) == position, 'Wrong level_event transport/position/scope')
        require(type(event.get('data')) is int, 'Level event data must be Int32')
        if expected_event == 1010:
            require(binding is not None and event['data'] == binding['registry_id'] and event.get('song') == song, 'Wrong raw song ID/name')
        else:
            require(expected_event == 1011 and event['data'] == 0, 'Wrong stop event')


def validate_privacy(chunks: list[dict], updates: list[dict], positions: list[tuple[int, int, int]]) -> None:
    for chunk in chunks:
        coordinates = [tuple(e.get(axis) for axis in ('x', 'y', 'z')) for e in chunk['block_entities']]
        require(len(set(coordinates)) == len(coordinates), 'Duplicate block entity in one chunk')
    for position in positions:
        entries = [entry for chunk in chunks for entry in chunk['block_entities']
                   if tuple(entry.get(axis) for axis in ('x', 'y', 'z')) == position]
        require(entries and all(e.get('block_entity_type') == 4 and e.get('data') in (None, {}) for e in entries),
                {'missing_or_private_chunk_entry': position, 'entries': entries})
    for update in updates:
        require(update.get('packet_id') == 6 and update.get('block_entity_type') == 4
                and all(type(update.get(axis)) is int for axis in ('x', 'y', 'z'))
                and update.get('data') == {}, 'Malformed/private live jukebox update')


def validate_campaign(report: dict) -> None:
    require(report.get('completed_boots') == list(BOOTS) and not report.get('runtime_errors'), 'All four ordered boots and clean cleanup required')
    require(report.get('provenance_checked_before') is True and report.get('provenance_checked_after') is True, 'Final provenance not verified')
    require(report.get('cleanup_complete') is True, 'Missing final process teardown')
    boots = report.get('boots', {})
    require(set(boots) == set(BOOTS), 'Missing/extra boot')
    for role in BOOTS:
        entry = boots[role]
        require(entry.get('result') == 'passed' and entry.get('engine') == ('rust' if role.startswith('rust') else 'vanilla'), 'Wrong boot provenance')
        clients = entry.get('clients', [])
        require(len(clients) == 2 and [c.get('name') for c in clients] == [NAME, OBSERVER], 'Missing owner/observer')
        for client in clients:
            require(type(client.get('pid')) is int and client['pid'] > 0 and client.get('argv'), 'Missing client process provenance')
            validate_registry(client['registry'])
        teardown = entry.get('teardown', {})
        require(teardown.get('boot') == role and type(teardown.get('pid')) is int and teardown['pid'] > 0
                and teardown.get('argv'), 'Missing/mismatched server process provenance')
        require(teardown.get('exit_code') == 0 and teardown.get('forced_stop') is False and len(teardown.get('clients', [])) == 2, 'Unclean server/client teardown')
        require([(c.get('name'), c.get('pid')) for c in teardown['clients']] == [(c['name'], c['pid']) for c in clients], 'Wrong clients in teardown receipt')
        require(all(c.get('exit_code') == 0 and c.get('forced_close') is False and not c.get('failure') for c in teardown['clients']), 'Client forced/failed cleanup')
        require(entry.get('saved') and entry.get('exercise', {}).get('complete') is True, 'Missing actual observations')
        identities = entry['exercise'].get('builtin_items', [])
        require([row.get('effective_song') for row in identities] == list(BUILTINS)
                and all(row.get('patch_present') is True for row in identities if row.get('effective_song') != DEFAULT), 'Missing all22 effective identities/21 nondefault patches')
    observations = report.get('observations', [])
    for role in BOOTS[:2]:
        accepted = [row for row in observations if row.get('boot') == role and row.get('kind') == 'predicted_swap'
                    and row.get('accepted_prediction') is True]
        require(set(row.get('slot') for row in accepted) >= set(range(23)), 'Missing real predictions for all22 references/default control')
        require(all(not any(reply.get('kind') in ('content', 'slot') for reply in row.get('replies', []))
                    and row.get('same_stream_fence') and row.get('authoritative_reopen') for row in accepted), 'Corrected/unfenced prediction cannot establish acceptance')
        require(all(any(row.get('boot') == role and row.get('kind') == stage for row in observations)
                    for stage in ('disc11_tick1440', 'disc11_tick1441')), 'Missing actual natural-duration observations')
    transfers = report.get('world_transfers', [])
    require(len(transfers) == 2, 'Exactly two whole-world copies required')
    for transfer, pair in zip(transfers, (('java_source', 'rust_import'), ('rust_reload', 'java_return'))):
        require((transfer.get('source_role'), transfer.get('target_role')) == pair and transfer.get('source_immutable') is True, 'Wrong transfer order/immutability')
        require(transfer.get('destination_before_copy') == {'exists': False} and transfer.get('source_before_copy', {}).get('files')
                and transfer['source_before_copy'] == transfer.get('source_after_copy') == transfer.get('destination_after_copy'), 'Whole-world copy was changed')


def campaign_rows(report: dict) -> list[dict]:
    try:
        validate_campaign(report)
        result, errors = 'passed', []
    except (AssertionError, KeyError, TypeError, ValueError) as error:
        result, errors = 'failed', [str(error)]
    return [{'id': case, 'features': [], 'result': result, 'errors': errors,
             'completed_boots': report.get('completed_boots', []),
             'composite': 'four boots, eight received registries, two unchanged copies, final provenance and clean teardown required'} for case in CASES]


def read_fixture(world: Path, artifact: Path, *, entities: bool = False) -> dict:
    """Retain exact terrain/entity chunk256,0 with bounded Anvil/NBT readers."""
    folder = 'entities' if entities else 'region'
    directories = [p for p in (world / folder, world / 'dimensions/minecraft/overworld' / folder) if p.is_dir()]
    require(len(directories) == 1, 'Expected one overworld region directory')
    region = directories[0] / 'r.8.0.mca'
    with region.open('rb') as source:
        header = source.read(worlds.SECTOR_BYTES * 2)
        require(len(header) == worlds.SECTOR_BYTES * 2, 'Truncated Anvil header')
        location = int.from_bytes(header[:4], 'big')
        sector, count = location >> 8, location & 255
        require(sector >= 2 and count > 0 and (sector + count) * worlds.SECTOR_BYTES <= region.stat().st_size, 'Invalid chunk allocation')
        source.seek(sector * worlds.SECTOR_BYTES)
        length = int.from_bytes(source.read(4), 'big')
        require(2 <= length <= min(count * worlds.SECTOR_BYTES - 4, worlds.MAX_NBT_BYTES), 'Invalid chunk length')
        record = source.read(length)
        require(len(record) == length, 'Truncated chunk')
    raw = worlds.decompress_nbt(record[1:], record[0])
    with artifact.open('xb') as retained:
        retained.write(raw)
    reader = PlayerReader(raw)
    require(reader.number('B') == 10, 'Expected compound root')
    reader.string()
    chunk = reader.value(10)
    write_json(artifact.with_suffix('.typed.json'), {'typed': chunk, 'decoded_bytes': reader.offset, 'payload_bytes': len(raw)})
    require(reader.offset == len(raw), 'Trailing chunk bytes')
    if entities:
        require(chunk.get('DataVersion') == atom('Int', 5023), 'Wrong entity chunk version')
        # Preserve writer-specific position metadata; recovery binds the saved
        # drop to the live nearest entity by its exact typed UUID.
        return {'path': str(region), 'region_sha256': digest(region), 'raw_nbt': str(artifact),
                'raw_nbt_sha256': hashlib.sha256(raw).hexdigest(), 'full_typed': str(artifact.with_suffix('.typed.json')), 'typed': chunk}
    require(chunk.get('DataVersion') == atom('Int', 5023) and chunk.get('xPos') == atom('Int', 256)
            and chunk.get('zPos') == atom('Int', 0), 'Wrong chunk identity/version')
    entries = compounds(chunk['block_entities'])
    selected = {}
    for name, position in {'chest': CHEST, 'jukebox': JUKEBOX, 'comparator': COMPARATOR, **RESTORES}.items():
        matches = [be for be in entries if tuple(be.get(axis) for axis in ('x', 'y', 'z')) == tuple(atom('Int', n) for n in position)]
        require(len(matches) <= 1, 'Duplicate block entity position')
        if matches:
            selected[name] = matches[0]
    return {'path': str(region), 'region_sha256': digest(region), 'raw_nbt': str(artifact),
            'raw_nbt_sha256': hashlib.sha256(raw).hexdigest(), 'full_typed': str(artifact.with_suffix('.typed.json')), 'typed': selected}


def retained_player(world: Path, stem: Path) -> dict:
    """Keep each saved player/statistics input before a later save replaces it."""
    artifacts = {}
    for label, relative, suffix in (
            ('player', f'players/data/{PLAYER_ID}.dat', '.player.dat'),
            ('statistics', f'players/stats/{PLAYER_ID}.json', '.stats.json')):
        raw, artifact = read_bytes(world / relative), stem.with_suffix(suffix)
        with artifact.open('xb') as output:
            output.write(raw)
        artifacts[label] = {'path': str(artifact), 'sha256': hashlib.sha256(raw).hexdigest()}
    player, statistics_file = read_player(world), read_statistics(world)
    write_json(stem.with_suffix('.player.typed.json'), {'player': player, 'statistics': statistics_file})
    require(player['sha256'] == artifacts['player']['sha256'] and statistics_file['sha256'] == artifacts['statistics']['sha256'],
            'Player/statistics changed during snapshot')
    return {'player': player, 'statistics': statistics_file, 'retained_inputs': artifacts}


def stopped_time(world: Path, stem: Path) -> dict:
    raw = read_bytes(world / 'level.dat')
    artifact = stem.with_suffix('.level.dat')
    with artifact.open('xb') as output:
        output.write(raw)
    decoded = worlds.decompress_nbt(raw, 1)
    reader = PlayerReader(decoded)
    require(reader.number('B') == 10, 'Expected compound level.dat root')
    reader.string()
    level = reader.value(10)
    write_json(stem.with_suffix('.level.typed.json'), {'typed': level, 'decoded_bytes': reader.offset, 'payload_bytes': len(decoded)})
    require(reader.offset == len(decoded) and level['Data'].get('DataVersion') == atom('Int', 5023), 'Trailing/wrong-version level.dat')
    return {'world_time': typed_int(level['Data']['Time'], 'Long', 0, 2**63 - 1),
            'artifact': str(artifact), 'sha256': hashlib.sha256(raw).hexdigest()}


def retain(fixture: Fixture, kind: str, value: dict) -> dict:
    rows = fixture.report.setdefault('observations', [])
    require(len(rows) < 2048, 'Campaign observation budget exceeded')
    rows.append({'boot': fixture.report['active_boot'], 'kind': kind, **value})
    return rows[-1]


@bounded
def snapshot(fixture: Fixture, label: str) -> dict:
    server = fixture.server
    evidence = retain(fixture, label, {'commands': [], 'world_time': suite.game_time(server)})
    for target in (JUKEBOX, COMPARATOR):
        evidence['commands'].append({'command': f'data get block {pos(target)}', 'output': server.command(f'data get block {pos(target)}')})
    server.save_flush()
    stem = f"{fixture.report['active_boot']}-{len(fixture.report['observations']):04d}"
    evidence['chunk'] = read_fixture(server.directory / 'world', server.directory / (stem + '.nbt'))
    evidence.update(retained_player(server.directory / 'world', server.directory / stem))
    write_json(server.directory / (stem + '.json'), evidence)
    return evidence


def statistics(client: Client, contract: dict) -> dict:
    request = client.request('request_stats')
    state = client.wait_state(lambda s: s.get('statistics_packet') is not None and s['statistics_packet']['sequence'] > request['after_sequence'])
    packet = state['statistics_packet']
    values = {}
    for name, pair in {'used': (contract['used_category'], ITEM_ID), 'play_record': (contract['custom_category'], contract['play_record'])}.items():
        matches = [e['value'] for e in packet['entries'] if (e['category_id'], e['statistic_id']) == pair]
        require(len(matches) <= 1, 'Duplicate statistic')
        values[name] = matches[0] if matches else 0
    return {'request': request, 'packet': packet, **values}


def fence(server, clients: list[Client]) -> dict:
    marker = 'DISC_' + uuid.uuid4().hex
    output = server.command('say ' + marker)
    received = [delivered_chat(c.wait_state(lambda s: delivered_chat(s, marker) is not None), marker) for c in clients]
    return {'output': output, 'received': received}


def open_menu(client: Client) -> dict:
    close_menu(client)
    before = client.state()['packets']
    client.request('use_item_on', **dict(zip(('x', 'y', 'z'), CHEST)))
    state = client.wait_state(lambda s: s.get('menu') is not None and s['menu']['opened_sequence'] > before and len(s['menu']['slots']) == 63)
    require(state['menu']['menu_type'] == 2 and state['menu']['carried'] is None, 'Wrong menu/cursor')
    return state['menu']


@bounded
def transfer(fixture: Fixture, owner: Client, slot: int) -> dict:
    evidence = retain(fixture, 'predicted_swap', {'slot': slot, 'before': open_menu(owner)})
    predicted = deepcopy(evidence['before']['slots'])
    predicted[slot], predicted[54] = predicted[54], predicted[slot]
    mark = owner.state()['packets']
    evidence['sent'] = owner.request('swap', slot=slot, hotbar=0)
    evidence['same_stream_fence'] = statistics(owner, fixture.report['contract'])
    evidence['replies'] = [p for p in owner.state()['menu_packets'] if p['sequence'] > mark and p['container_id'] == evidence['before']['container_id']]
    evidence['authoritative_reopen'] = open_menu(owner)
    close_menu(owner)
    evidence['predicted'] = predicted
    # Any content/slot correction disqualifies the checksum inference, even when
    # the click applied. The authoritative reopen is retained either way.
    require(not any(p['kind'] in ('content', 'slot') for p in evidence['replies']), {'prediction_corrected': evidence})
    require(menu_semantics(evidence['authoritative_reopen']['slots']) == menu_semantics(predicted), 'Authoritative reopen differs from prediction')
    evidence['accepted_prediction'] = True
    evidence['hash_claim'] = 'Inference from actual Java/Rust action acceptance and same-stream replies, not an observed numeric Java checksum'
    return evidence


def marked(clients: list[Client]) -> list[int]:
    return [c.state()['packets'] for c in clients]


def observe_effects(fixture: Fixture, clients: list[Client], marks: list[int], expected_events: list[int], song: str | None) -> dict:
    evidence = retain(fixture, 'effects', {'fence': fence(fixture.server, clients), 'clients': []})
    for client, mark in zip(clients, marks):
        state = client.state()
        events = [e for e in state['jukebox_events'] if e['sequence'] > mark and tuple(e[a] for a in ('x', 'y', 'z')) == JUKEBOX]
        # Song audio must use1010/1011. Retain sound transports so they cannot be
        # silently substituted for level events by a decoder or assertion.
        sounds = [e for e in state['sounds'] + state['horn_entity_sounds'] if e['sequence'] > mark
                  and (e.get('registry_id') in fixture.report['contract']['song_sound_ids']
                       or e.get('sound_id') in ['minecraft:music_disc.' + name for name in PROFILES])]
        updates = [e for e in state['jukebox_updates'] if e['sequence'] > mark]
        evidence['clients'].append({'name': client.name, 'events': events, 'sound_transports': sounds, 'updates': updates})
    for client, entry in zip(clients, evidence['clients']):
        validate_events(entry['events'], expected_events, client.state()['jukebox_registry'], song)
        require(not entry['sound_transports'], 'Song sound118/119 is not level_event1010/1011')
        validate_privacy([], entry['updates'], [])
    return evidence


@bounded
def advance(fixture: Fixture, clients: list[Client], count: int, *, stop: bool = False) -> dict:
    require(type(count) is int and 0 < count <= 141, 'Tick step exceeds ordinary watchdog budget')
    evidence = retain(fixture, 'actual_world_ticks', {'requested': count})
    marks = marked(clients)
    evidence['step'] = step_exact(fixture.server, count)
    evidence['effects'] = observe_effects(fixture, clients, marks, [1011] if stop else [], None)
    return evidence


@bounded
def check_block(fixture: Fixture, *, song: str | None, ticks: int | None, playing: bool) -> dict:
    server = fixture.server
    result = retain(fixture, 'block_state', {'data_get': server.command(f'data get block {pos(JUKEBOX)}')})
    result['has_record'] = typed_predicate(server, f'if block {pos(JUKEBOX)} minecraft:jukebox[has_record={str(song is not None).lower()}]')
    result['power'] = typed_predicate(server, f'if block {pos(WIRE)} minecraft:redstone_wire[power={15 if playing else 0}]')
    comparator = PROFILES[song[10:]][1] if song else 0
    result['comparator'] = command_result(server, f'data get block {pos(COMPARATOR)} OutputSignal', comparator)
    if song is not None:
        result['record'] = typed_predicate(server, f'if data block {pos(JUKEBOX)} {{RecordItem:{typed_snbt(stack(song))}}}')
    else:
        result['record'] = typed_predicate(server, f'unless data block {pos(JUKEBOX)} RecordItem')
    if ticks is None:
        result['elapsed'] = typed_predicate(server, f'unless data block {pos(JUKEBOX)} ticks_since_song_started')
    else:
        result['elapsed'] = command_result(server, f'data get block {pos(JUKEBOX)} ticks_since_song_started', ticks)
    return result


@bounded
def interact(fixture: Fixture, clients: list[Client], hand: int, *, song: str | None,
             events: list[int], awards: int) -> dict:
    owner = clients[0]
    evidence = retain(fixture, 'use_item_on', {'hand': hand, 'before': statistics(owner, fixture.report['contract'])})
    marks = marked(clients)
    evidence['sent'] = owner.request('jukebox_use', hand=hand, **dict(zip(('x', 'y', 'z'), JUKEBOX)))
    evidence['after'] = statistics(owner, fixture.report['contract'])
    evidence['effects'] = observe_effects(fixture, clients, marks, events, song)
    require(evidence['before']['used'] == evidence['after']['used'] == 0, 'Builtin insertion/ejection/fallback awarded Used')
    require(evidence['after']['play_record'] - evidence['before']['play_record'] == awards, 'Wrong PlayRecord delta')
    return evidence


@bounded
def offhand_retained(fixture: Fixture, expected_disc: dict, ticks: int | None) -> dict:
    evidence = snapshot(fixture, 'after_offhand_occupied')
    evidence['has_record'] = typed_predicate(fixture.server, f'if block {pos(JUKEBOX)} minecraft:jukebox[has_record=true]')
    evidence['no_drop'] = typed_predicate(fixture.server, f'at {NAME} unless entity @e[type=minecraft:item,distance=..8]')
    validate_offhand_record(evidence['chunk']['typed']['jukebox'], expected_disc, ticks)
    return evidence


@bounded
def recover(fixture: Fixture, clients: list[Client], expected_disc: dict, *, creative: bool) -> None:
    server = fixture.server
    evidence = retain(fixture, 'real_ejected_item', {'data': server.command(
        f'execute at {NAME} run data get entity @e[type=minecraft:item,distance=..8,limit=1,sort=nearest] Item')})
    server.save_flush()
    stem = f"{fixture.report['active_boot']}-{len(fixture.report['observations']):04d}-drop.nbt"
    evidence['entities'] = read_fixture(server.directory / 'world', server.directory / stem, entities=True)
    drops = [entity for entity in compounds(evidence['entities']['typed']['Entities']) if entity.get('id') == 'minecraft:item']
    evidence['drops'] = drops
    require(len(drops) == 1, 'Expected exactly one saved fixture drop')
    drop = drops[0]
    identity = drop.get('UUID')
    require(isinstance(identity, dict) and set(identity) == {'nbt_type', 'value'} and identity['nbt_type'] == 'IntArray'
            and isinstance(identity['value'], list) and len(identity['value']) == 4
            and all(type(value) is int and -2**31 <= value < 2**31 for value in identity['value']), 'Invalid dropped entity UUID')
    evidence['live_identity'] = typed_predicate(server,
        f'at {NAME} if data entity @e[type=minecraft:item,distance=..8,limit=1,sort=nearest] {{UUID:[I;{",".join(map(str, identity["value"]))}]}}')
    validate_ejected_item(drop.get('Item'), expected_disc)
    command = f'execute at {NAME} run ' + (f'kill @e[type=minecraft:item,distance=..8]' if creative else f'tp @e[type=minecraft:item,distance=..8] {NAME}')
    evidence['command'] = command_result(server, command, 1)
    if not creative:
        advance(fixture, clients, 20)
    evidence['no_remaining_drop'] = typed_predicate(server, f'at {NAME} unless entity @e[type=minecraft:item,distance=..8]')


def cycle(fixture: Fixture, clients: list[Client], song: str, *, hand: int, creative: bool, natural: bool = False,
          opaque: bool = False) -> None:
    owner, server = clients[0], fixture.server
    if not opaque:
        transfer(fixture, owner, BUILTINS.index(song))
    with interaction_deadline(server, 'select_actual_hand_and_mode'):
        owner.request('jukebox_select', hotbar=1 if opaque else 0)
        server.command(f'gamemode {"creative" if creative else "survival"} {NAME}')
        typed_predicate(server, f'if entity @a[name={NAME},gamemode={"creative" if creative else "survival"}]')
        if hand:
            owner.request('swap_hands')
            statistics(owner, fixture.report['contract'])
    interact(fixture, clients, hand, song=song, events=[1010], awards=1)
    inserted = snapshot(fixture, 'inserted')
    held_slot = -106 if hand else (1 if opaque else 0)
    inventory = item_slots(inserted['player']['typed']['Inventory'], -106, 35)
    expected_disc = stack(opaque=True) if opaque else stack(song)
    require(inventory.get(held_slot) == (expected_disc if creative else None), 'Wrong survival consumption/creative retention')
    require(record_state(inserted['chunk']['typed']['jukebox']) == {'record': expected_disc, 'elapsed': 0}, 'Insertion record/elapsed differs')
    advance(fixture, clients, 2)
    check_block(fixture, song=song, ticks=2, playing=True)
    if natural:
        remaining = 1438
        while remaining:
            count = min(141, remaining)
            advance(fixture, clients, count)
            remaining -= count
        at1440 = snapshot(fixture, 'disc11_tick1440')
        require(record_state(at1440['chunk']['typed']['jukebox']) == {'record': stack(DEFAULT), 'elapsed': 1440}, 'Disc11 did not remain active through actual1440 ticks')
        check_block(fixture, song=DEFAULT, ticks=1440, playing=True)
        advance(fixture, clients, 1, stop=True)
        stopped = snapshot(fixture, 'disc11_tick1441')
        require(record_state(stopped['chunk']['typed']['jukebox']) == {'record': stack(DEFAULT), 'elapsed': None}, 'Natural completion removed record or retained active elapsed')
        check_block(fixture, song=DEFAULT, ticks=None, playing=False)
    # Vanilla TRY_WITH_EMPTY_HAND fallback runs for main hand. Offhand occupied
    # no-op is separately observed; it must not eject or award anything.
    interact(fixture, clients, 1, song=None, events=[], awards=0)
    offhand_retained(fixture, expected_disc, None if natural else 2)
    interact(fixture, clients, 0, song=None, events=[] if natural else [1011], awards=0)
    recover(fixture, clients, expected_disc, creative=creative)
    if creative and hand:
        with interaction_deadline(server, 'restore_actual_hand'):
            owner.request('swap_hands')
            statistics(owner, fixture.report['contract'])
    # Comparator output is scheduled; settling uses real ticks, not a counter edit.
    advance(fixture, clients, 2)
    check_block(fixture, song=None, ticks=None, playing=False)
    recovered = snapshot(fixture, 'recovered')
    slots = item_slots(recovered['player']['typed']['Inventory'], -106, 35)
    require(slots.get(1 if opaque else 0) == expected_disc and -106 not in slots, 'Ejected item/hand did not recover')
    if not opaque:
        transfer(fixture, owner, BUILTINS.index(song))
    owner.request('jukebox_select', hotbar=0)


@bounded
def author(fixture: Fixture, owner: Client) -> dict:
    server = fixture.server
    evidence = retain(fixture, 'java_author', {'commands': []})
    require(fixture.kind == 'vanilla' and fixture.report['active_boot'] == 'java_source', 'Only source Java may author items')
    command_result(server, f'data get entity {NAME} Inventory', 0)
    commands = [f"item replace block {pos(CHEST)} container.{slot} with {ITEM}[{COMPONENT}='{name}']" for slot, name in enumerate(BUILTINS)]
    commands += [f'item replace block {pos(CHEST)} container.22 with {ITEM}',
        f'item replace entity {NAME} hotbar.1 with {ITEM}[minecraft:custom_data={{jukebox_playable:"{CAT}"}}]',
        f'item replace entity {NAME} hotbar.2 with {ITEM}[!{COMPONENT}]']
    for command in commands:
        evidence['commands'].append(command_result(server, command, 1, mode='success'))
    return evidence


@bounded
def author_restore_fixtures(fixture: Fixture) -> dict:
    require(fixture.kind == 'vanilla' and fixture.report['active_boot'] == 'java_source', 'Restore provenance must be real source Java')
    evidence = retain(fixture, 'java_restore_intents', {'commands': []})
    for name, ticks in (('absent', None), ('zero', 0), ('negative', -10000), ('finished', 1441)):
        tag = '{RecordItem:' + typed_snbt(stack()) + ('' if ticks is None else f',ticks_since_song_started:{ticks}L') + '}'
        command = f'setblock {pos(RESTORES[name])} minecraft:jukebox[has_record=true]{tag}'
        evidence['commands'].append(command_result(fixture.server, command, 1, mode='success'))
    return evidence


def validate_continuation(before: dict, after: dict, *, exact_ticks: int | None = None) -> None:
    require(type(before.get('world_time')) is int and type(after.get('world_time')) is int, 'Missing actual World tick observations')
    delta = after['world_time'] - before['world_time']
    require(type(delta) is int and delta >= 0 and (exact_ticks is None or delta == exact_ticks), 'World time regressed/stepped incorrectly')
    for name in RESTORES:
        old, new = record_state(before['chunk']['typed'][name]), record_state(after['chunk']['typed'][name])
        require(old['record'] == new['record'] == stack(), 'Persisted restore record changed')
        start, end = old['elapsed'], new['elapsed']
        if start is None:
            require(end is None, 'Inactive saved record restarted')
        elif exact_ticks is not None:
            require(end == (start + delta if start + delta <= 1440 else None), 'Restored playback did not advance by actual world ticks')
        else:
            # Startup may load this chunk after earlier World ticks. Retain the
            # actual bounds; do not invent exact elapsed during server startup.
            require((end is not None and start <= end <= min(start + delta, 1440)) or
                    (end is None and start + delta >= 1441), 'Restore elapsed outside actual startup tick bound')


def exercise(fixture: Fixture, clients: list[Client], *, full: bool, detail: dict) -> None:
    owner, server = clients[0], fixture.server
    with interaction_deadline(server, 'initial_authoritative_menu'):
        detail['initial_menu'] = open_menu(owner)
        close_menu(owner)
    require(menu_semantics(detail['initial_menu']['slots']) == menu_slots(), 'All22/controls menu differs')
    detail['builtin_items'] = [{'slot': slot, 'effective_song': song,
        'patch_present': COMPONENT in detail['initial_menu']['slots'][slot].get('components', {})} for slot, song in enumerate(BUILTINS)]
    require(all(row['patch_present'] for row in detail['builtin_items'] if row['effective_song'] != DEFAULT), 'Missing nondefault reference patch')
    detail['identity_counts'] = {'effective_identities': 22, 'nondefault_patches': 21,
        'received_reference_patches': sum(row['patch_present'] for row in detail['builtin_items'])}
    if full:
        for slot in range(23):
            transfer(fixture, owner, slot)
            transfer(fixture, owner, slot)
    for song in (DEFAULT, CAT):
        for creative in (False, True):
            for hand in (0, 1):
                cycle(fixture, clients, song, hand=hand, creative=creative,
                      natural=full and song == DEFAULT and not creative and hand == 0)
    with interaction_deadline(server, 'select_removed_control'):
        owner.request('jukebox_select', hotbar=2)
        server.command(f'gamemode survival {NAME}')
    interact(fixture, clients, 0, song=None, events=[], awards=0)
    cycle(fixture, clients, DEFAULT, hand=0, creative=True, opaque=True)
    with interaction_deadline(server, 'final_authoritative_menu'):
        owner.request('jukebox_select', hotbar=0)
        server.command(f'gamemode survival {NAME}')
        detail['final_menu'] = open_menu(owner)
        require(menu_semantics(detail['final_menu']['slots']) == menu_slots(), 'Final menu did not retain all items/controls')
        close_menu(owner)
        detail['statistics'] = statistics(owner, fixture.report['contract'])
    detail['complete'] = True


def saved(fixture: Fixture, role: str, before_stop: dict, expected_play_record: int) -> dict:
    world = fixture.server.directory / 'world'
    stem = fixture.server.directory / (role + '-stopped')
    time_evidence = stopped_time(world, stem)
    evidence = retain(fixture, 'stopped_world', {'world_time': time_evidence['world_time'], 'level': time_evidence,
        'chunk': read_fixture(world, stem.with_suffix('.nbt')), **retained_player(world, stem)})
    artifact = fixture.server.directory / (role + '-stopped.json')
    write_json(artifact, evidence)
    player = evidence['player']['typed']
    require(player.get('UUID') == {'nbt_type': 'IntArray', 'value': list(UUID_INTS)}
            and player.get('DataVersion') == atom('Int', 5023), 'Player identity/version changed')
    require(player.get('SelectedItemSlot') == atom('Int', 0), 'Wrong saved selected slot')
    chest, inventory = expected()
    require(item_slots(evidence['chunk']['typed']['chest']['Items'], 0, 26) == chest, 'Saved chest changed')
    require(item_slots(player['Inventory'], -106, 35) == inventory, 'Saved player items changed')
    require(record_state(evidence['chunk']['typed']['jukebox']) == {'record': None, 'elapsed': None}, 'Used jukebox did not save empty')
    stats = evidence['statistics']['data']['stats']
    require(stats.get('minecraft:used', {}).get(ITEM, 0) == 0 and
            stats.get('minecraft:custom', {}).get('minecraft:play_record', 0) == expected_play_record, 'Saved statistics differ from actual wire')
    validate_continuation(before_stop, evidence, exact_ticks=0)
    return {'artifact': str(artifact), 'sha256': digest(artifact), 'evidence': evidence}


def run_campaign(args: argparse.Namespace, run: Path, report: dict) -> None:
    fixtures, completed, errors, immutable = [], [], [], []
    report['boots'] = {}
    previous_world, previous_saved, previous_play_record = None, None, 0
    try:
        for index, role in enumerate(BOOTS):
            report['active_boot'] = role
            kind = 'rust' if role.startswith('rust') else 'vanilla'
            if role == 'rust_reload':
                fixture = fixtures[-1]
            else:
                fixture = Fixture(kind, run / role, args, report)
                fixtures.append(fixture)
                fixture.server.configure()
                if previous_world is not None:
                    target = fixture.server.directory / 'world'
                    transfer_evidence = {'source_role': BOOTS[index - 1], 'target_role': role,
                        'discarded_unstarted_scaffold': world_hashes(target)}
                    shutil.rmtree(target)  # Just-created, unstarted owned datapack scaffold.
                    report.setdefault('world_transfers', []).append(transfer_evidence)
                    clone_world(previous_world, target, transfer_evidence)
                    immutable.append((previous_world, world_hashes(previous_world), transfer_evidence))
            entry = report['boots'][role] = {'engine': kind, 'directory': str(fixture.server.directory), 'started': now(), 'result': 'failed'}
            old_pid = fixture.server.process.pid if fixture.server.process is not None else None
            fixture.start(role)
            require(fixture.server.process.pid != old_pid, 'Restart must create a fresh process')
            server = fixture.server
            entry['status'] = protocol.status_fields(server.port)
            server.command('tick freeze')
            require_frozen(server)
            if index == 0:
                server.command('forceload add 4096 0')
                suite.wait_loaded(server, x=4096, z=0)
                for command in ('fill 4096 199 0 4111 199 15 stone', 'difficulty peaceful',
                                'scoreboard objectives add pdprobe dummy', f'setblock {pos(CHEST)} chest',
                                f'setblock {pos(JUKEBOX)} jukebox', f'setblock {pos(COMPARATOR)} comparator[facing=north]',
                                f'setblock {pos(WIRE)} redstone_wire'):
                    server.command(command)
            clients = []
            entry['clients'] = []
            for name in (NAME, OBSERVER):
                client = Client(server, name=name, log_name=f'{role}-{name}', jukeboxes=True, raw_packets=True)
                fixture.clients.append(client)
                clients.append(client)
                client.ready()
                client.wait_state(lambda s: s['chunks'] >= 49, timeout=60)
                state = client.state()
                received = {'name': name, 'pid': client.process.pid, 'argv': client.process.args,
                    'registry': state.get('jukebox_registry'), 'login_header': state.get('horn_login_header')}
                entry['clients'].append(received)
                validate_registry(received['registry'])
                require(received['login_header'] is not None, 'Missing actual Login entity ID')
                identity = uuid.UUID(bytes=hashlib.md5(('OfflinePlayer:' + name).encode(), usedforsecurity=False).digest(), version=3)
                ints = [int.from_bytes(identity.bytes[start:start + 4], 'big', signed=True) for start in range(0, 16, 4)]
                typed_predicate(server, f'if entity @a[name={name},nbt={{UUID:[I;{",".join(map(str, ints))}]}}]')
            owner, observer = clients
            require(entry['clients'][0]['login_header']['entity_id'] != entry['clients'][1]['login_header']['entity_id'], 'Duplicate owner/observer entity IDs')
            teleport_exact(server, owner, POS)
            server.command(f'tp {OBSERVER} 4102.5 200 3.5')
            observer.wait_state(lambda s: s.get('server_position') is not None and
                all(abs(s['server_position'][axis] - value) < .01 for axis, value in (('x', 4102.5), ('y', 200), ('z', 3.5))))
            entry['identity'] = identity_witness(server, owner)
            entry['recipient_scope'] = [typed_predicate(server,
                f'if entity @a[name={name},nbt={{Dimension:"minecraft:overworld",Pos:[{x}d,200.0d,3.5d]}}]')
                for name, x in ((NAME, 4098.5), (OBSERVER, 4102.5))]
            for client in clients:
                client.wait_state(lambda s: any(c['x'] == 256 and c['z'] == 0 for c in s['chunk_packets']))
            entry['initial_privacy'] = [{'name': c.name, 'chunks': [p for p in c.state()['chunk_packets'] if p['x'] == 256 and p['z'] == 0],
                'updates': c.state()['jukebox_updates']} for c in clients]
            for privacy in entry['initial_privacy']:
                validate_privacy(privacy['chunks'], privacy['updates'], [JUKEBOX, *(RESTORES.values() if index else [])])
            initial = statistics(owner, report['contract'])
            entry['initial_statistics'] = initial
            require(initial['used'] == 0 and initial['play_record'] == previous_play_record, 'Statistics changed across transfer/restart')
            if index == 0:
                entry['author'] = author(fixture, owner)
            else:
                restored = snapshot(fixture, 'restored')
                entry['restored'] = restored
                entry['restore_wire'] = [{'name': c.name, 'events': c.state()['jukebox_events']} for c in clients]
                for wire in entry['restore_wire']:
                    require(not any(e['event'] == 1010 and tuple(e[a] for a in ('x', 'y', 'z')) in RESTORES.values()
                                    for e in wire['events']), 'Reload incorrectly emitted a start event')
                validate_continuation(previous_saved, restored)
                advance(fixture, clients, 20)
                continued = snapshot(fixture, 'restore_actual20ticks')
                entry['continued'] = continued
                validate_continuation(restored, continued, exact_ticks=20)
            entry['exercise'] = {}
            exercise(fixture, clients, full=index < 2, detail=entry['exercise'])
            previous_play_record = entry['exercise']['statistics']['play_record']
            require(previous_play_record == initial['play_record'] + 9, 'Unexpected awards outside the nine actual successful insertions')
            if index == 0:
                entry['restore_author'] = author_restore_fixtures(fixture)
            before_stop = snapshot(fixture, 'before_stop')
            entry['before_stop'] = before_stop
            if index == 0:
                entry['java_persisted_restore_states'] = {name: record_state(before_stop['chunk']['typed'][name]) for name in RESTORES}
                require(entry['java_persisted_restore_states']['absent']['elapsed'] is None
                        and entry['java_persisted_restore_states']['zero']['elapsed'] == 0
                        and entry['java_persisted_restore_states']['negative']['elapsed'] == -10000,
                        'Source Java did not persist the required distinct absent/zero/negative fixtures')
            for client in clients:
                fixture.close(client)
            wait_predicate(server, 'unless entity @a')
            server.save_flush()
            fixture.stop()
            entry['teardown'] = deepcopy(fixture.processes[-1])
            entry['saved'] = saved(fixture, role, before_stop, previous_play_record)
            entry.update(result='passed', finished=now())
            completed.append(role)
            previous_world, previous_saved = server.directory / 'world', entry['saved']['evidence']
        for path, hashes, transfer_evidence in immutable:
            require(world_hashes(path) == hashes, f'Transferred source changed: {path}')
            transfer_evidence['source_immutable'] = True
    except Exception as error:
        errors.append(f'{type(error).__name__}: {error}')
    finally:
        for fixture in reversed(fixtures):
            errors.extend(fixture.cleanup())
        report['client_artifacts'] = [{'path': str(path), 'sha256': digest(path), 'bytes': path.stat().st_size}
            for fixture in fixtures for name in (NAME, OBSERVER) for path in sorted(fixture.server.directory.glob(f'*-{name}.jsonl'))]
        alive = [p.pid for f in fixtures for p in (f.server.process, *(c.process for c in f.clients)) if p is not None and p.poll() is None]
        if alive:
            errors.append(f'Owned processes still alive: {alive}')
        report['completed_boots'], report['runtime_errors'] = completed, errors
        report['cleanup_complete'] = not alive and not errors and completed == list(BOOTS)
        report['cases'] = campaign_rows(report)  # Remains failed until final provenance check.
    require(not errors and completed == list(BOOTS), errors)


def manifest() -> dict:
    return {**inherited_manifest(), **{path: digest(oracle.ROOT / path) for path in
        ('tools/check_jukeboxes.py', 'tools/check_goat_horns.py', 'tools/check_item_interchange.py', 'tests/test_jukeboxes.py')}}


def primary_contract(args: argparse.Namespace) -> dict:
    evidence = {name: verify_artifacts(args.primary_root / Path(name).parent,
                {Path(name).name: pin})[Path(name).name] for name, pin in PRIMARY.items()}
    reports = verify_artifacts(args.reference_cache / 'generated/reports', REPORT_PINS)
    registries = json.loads(read_bytes(args.reference_cache / 'generated/reports/registries.json'), object_pairs_hook=unique_object)
    packets = json.loads(read_bytes(args.reference_cache / 'generated/reports/packets.json'), object_pairs_hook=unique_object)
    entries = lambda name: registries['minecraft:' + name]['entries']
    require(entries('item')[ITEM]['protocol_id'] == ITEM_ID and entries('data_component_type')[COMPONENT]['protocol_id'] == 66
            and entries('block_entity_type')['minecraft:jukebox']['protocol_id'] == 4, 'Static fixture IDs differ from official report')
    for phase, direction, name, identifier in (
            ('configuration', 'clientbound', 'registry_data', 7), ('play', 'clientbound', 'block_entity_data', 6),
            ('play', 'clientbound', 'level_event', 47), ('play', 'serverbound', 'use_item_on', 66),
            ('play', 'serverbound', 'set_carried_item', 54)):
        require(packets[phase][direction]['minecraft:' + name]['protocol_id'] == identifier, 'Wrong pinned packet ID: ' + name)
    # Only the static sound registry is resolved from the official report. Song
    # numeric IDs are always taken from each connection's actual configuration.
    return {'primary': evidence, 'reports': reports, 'used_category': entries('stat_type')['minecraft:used']['protocol_id'],
        'custom_category': entries('stat_type')['minecraft:custom']['protocol_id'],
        'play_record': entries('custom_stat')['minecraft:play_record']['protocol_id'],
        'song_sound_ids': [entries('sound_event')['minecraft:music_disc.' + name]['protocol_id'] for name in PROFILES],
        'song_registry_mapping': 'per-connection captured configuration only'}


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
    parser.add_argument('--startup-timeout', type=float, default=180)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    require(args.accept_eula, 'Explicit isolated reference EULA acceptance required')
    require(args.root.resolve() == oracle.ROOT.resolve(), '--root must be this harness checkout')
    require(0 < args.startup_timeout <= 180 and all(re.fullmatch(r'[0-9a-f]{40}', v) for v in (args.current_clean_commit, args.rust_source_commit)), 'Invalid startup bound/commit')
    cache = (oracle.ROOT / '.cache').resolve()
    require(args.output.resolve().is_relative_to(cache) and re.fullmatch(r'jukeboxes-[A-Za-z0-9_-]+\.json', args.output.name), 'Use fresh .cache/jukeboxes-*.json')
    require(not args.output.is_symlink() and not any(p.is_symlink() for p in args.output.absolute().parents if p.is_relative_to(cache)), 'Linked output refused')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    output = args.output.open('x')
    runs = cache / 'runs'
    runs.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix='jukeboxes-', dir=runs))
    report = {'schema_version': 1, 'kind': 'jukeboxes', 'result': 'failed', 'created_at': now(),
        'scope': __doc__, 'target': {'version': '26.3', 'protocol': 777, 'data_version': 5023},
        'environment': {'host': socket.gethostname(), 'checkout': str(oracle.ROOT), 'fixture': str(run),
            'binding': '127.0.0.1 isolated', 'workers': 2, 'startup_seconds': args.startup_timeout, 'interaction_seconds': 20},
        'identities': {'owner': NAME, 'owner_offline_uuid': str(PLAYER_ID), 'observer': OBSERVER,
            'observer_offline_uuid': str(uuid.UUID(bytes=hashlib.md5(('OfflinePlayer:' + OBSERVER).encode(), usedforsecurity=False).digest(), version=3))},
        'gaps': GAPS, 'processes': {}, 'provenance_checked_before': False, 'provenance_checked_after': False}
    report['cases'] = campaign_rows(report)
    began = time.monotonic()
    try:
        report['source'] = suite.git_state()
        require(report['source'] == {'commit': args.current_clean_commit, 'dirty': False}, 'Requires exact current clean commit')
        report['manifest'] = manifest()
        oracle.CACHE = args.reference_cache.resolve()
        oracle.acquire = suite.acquire = cached_reference
        official = cached_reference()
        require(digest(oracle.CACHE / 'minecraft.jar') == INNER_JAR_SHA256, 'Changed pinned inner JAR')
        args.java = str(Path(shutil.which(args.java) or args.java).resolve())
        require(digest(Path(args.java)) == JDK_SHA256, 'Requires pinned cached JDK25')
        version = subprocess.run([args.java, '-version'], capture_output=True, text=True, check=True, timeout=15)
        report['java'] = {'path': args.java, 'sha256': JDK_SHA256, 'version': version.stdout + version.stderr}
        report['official'] = {'path': str(official), 'sha256': digest(official), 'inner_sha256': INNER_JAR_SHA256,
            'outer_sha1': '33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c'}
        report['contract'] = primary_contract(args)
        original = args.rust_binary.resolve()
        require(original.is_file() and os.access(original, os.X_OK), 'Existing Rust executable required')
        binary_hash = digest(original)
        args.rust_binary = suite.pin_binary(original, run)
        require(digest(original) == digest(args.rust_binary) == binary_hash, 'Binary changed during pin')
        report['binary'] = {'source': str(original), 'pinned': str(args.rust_binary), 'sha256': binary_hash,
            'source_commit': args.rust_source_commit, 'association': 'caller attestation, not embedded provenance'}
        report['provenance_checked_before'] = True
        run_campaign(args, run, report)
        require(suite.git_state() == report['source'] and manifest() == report['manifest'], 'Source changed during run')
        require(primary_contract(args) == report['contract'], 'Primary/report inputs changed during run')
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
    print(f"jukeboxes {report['result']}: {args.output}; artifacts {run}", flush=True)
    return 0 if report['result'] == 'passed' else 1


if __name__ == '__main__':
    raise SystemExit(main())
