"""Synthetic helper-validator tests, never runtime packets or Java hash goldens."""
import base64
from copy import deepcopy
import hashlib
from pathlib import Path
import struct
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch
import zlib

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
import check_jukeboxes as discs


class CurrentPlayerEquipmentTests(unittest.TestCase):
    def test_offhand_participates_in_exact_item_comparison_and_legacy_duplicates_refuse(self):
        disc = discs.stack(discs.CAT)
        player = {'Inventory': inventory(), 'equipment': {'offhand': disc}}
        original = deepcopy(player)
        self.assertEqual(discs.player_items(player), {-106: disc})
        self.assertEqual(player, original)
        self.assertEqual(discs.player_items({'Inventory': inventory()}), {})
        with self.assertRaises(AssertionError):
            discs.player_items({'Inventory': inventory({'Slot': discs.atom('Byte', -106), **disc}),
                                'equipment': {'offhand': disc}})
        with self.assertRaises(AssertionError):
            discs.player_items({'Inventory': inventory(), 'equipment': {'offhand': {}}})


def inventory(*entries):
    return {'nbt_type': 'List', 'element_type': 10, 'value': list(entries)}


def saved_pos(*coordinates):
    return {'nbt_type': 'List', 'element_type': 6,
            'value': [{'nbt_type': 'Double', 'value': coordinate} for coordinate in coordinates]}


def registry():
    # Clearly synthetic envelope for composite validation, not a codec fixture.
    raw = b'SYNTHETIC VALIDATOR INPUT, NOT A SERVER PACKET'
    return {'registry': 'minecraft:jukebox_song', 'entries': [
        {'registry_id': index, 'reference_id': index + 1, 'identifier': 'minecraft:' + name,
         'data': {'sound_event': 'minecraft:music_disc.' + name, 'length_in_seconds': seconds,
                  'comparator_output': comparator, 'description': {'translate': 'jukebox_song.minecraft.' + name}}}
        for index, (name, (seconds, comparator)) in enumerate(reversed(tuple(discs.PROFILES.items())))],
        'payload_base64': base64.b64encode(raw).decode(), 'payload_sha256': hashlib.sha256(raw).hexdigest(),
        'payload_bytes': len(raw), 'decoded_bytes': len(raw)}


def complete_report():
    """Synthetic composite inputs exercise failure gates, not executable results."""
    report = {'completed_boots': list(discs.BOOTS), 'runtime_errors': [], 'cleanup_complete': True,
        'provenance_checked_before': True, 'provenance_checked_after': True, 'boots': {}, 'observations': []}
    for role in discs.BOOTS:
        clients = [{'name': name, 'pid': index + 1, 'argv': ['SYNTHETIC'], 'registry': registry()}
                   for index, name in enumerate((discs.NAME, discs.OBSERVER))]
        report['boots'][role] = {'engine': 'rust' if role.startswith('rust') else 'vanilla', 'result': 'passed',
            'clients': clients, 'saved': {'synthetic': True},
            'exercise': {'complete': True, 'builtin_items': [{'effective_song': name, 'patch_present': name != discs.DEFAULT} for name in discs.BUILTINS]},
            'teardown': {'boot': role, 'pid': 100, 'argv': ['SYNTHETIC'], 'exit_code': 0, 'forced_stop': False,
                'clients': [{'name': c['name'], 'pid': c['pid'], 'exit_code': 0, 'forced_close': False, 'failure': None} for c in clients]}}
        if role in discs.BOOTS[:2]:
            report['observations'] += [{'boot': role, 'kind': 'predicted_swap', 'slot': slot, 'accepted_prediction': True,
                'replies': [], 'same_stream_fence': {'synthetic': True}, 'authoritative_reopen': {'synthetic': True}} for slot in range(23)]
            report['observations'] += [{'boot': role, 'kind': stage} for stage in ('disc11_tick1440', 'disc11_tick1441')]
    hashes = {'files': {'SYNTHETIC.dat': '0' * 64}, 'directories': [], 'bytes': 1}
    report['world_transfers'] = [{'source_role': source, 'target_role': target,
        'source_immutable': True, 'source_before_copy': deepcopy(hashes), 'source_after_copy': deepcopy(hashes),
        'destination_after_copy': deepcopy(hashes), 'destination_before_copy': {'exists': False}}
        for source, target in (('java_source', 'rust_import'), ('rust_reload', 'java_return'))]
    return report


def restore_snapshot(ticks, time=0):
    be = {'id': 'minecraft:jukebox', 'RecordItem': discs.stack()}
    if ticks is not None:
        be['ticks_since_song_started'] = discs.atom('Long', ticks)
    return {'world_time': time, 'chunk': {'typed': {name: deepcopy(be) for name in discs.RESTORES}}}


class JukeboxCampaignTests(unittest.TestCase):
    def test_snapshot_dirties_real_chunk_before_flush_and_keeps_exact_elapsed(self):
        # Synthetic action witnesses test sequencing, not a captured Java result.
        trace = []
        server = SimpleNamespace(directory=Path('/SYNTHETIC'),
            command=Mock(side_effect=lambda command: trace.append(('live', command)) or ['SYNTHETIC live data']),
            save_flush=Mock(side_effect=lambda: trace.append(('flush',))))
        fixture = SimpleNamespace(server=server, report={'active_boot': 'java_source'})
        chunk = {'typed': {'jukebox': {'id': 'minecraft:jukebox', 'RecordItem': discs.stack(),
                                      'ticks_since_song_started': discs.atom('Long', 1440)}}}

        def predicate(_server, command):
            trace.append(('predicate', command))
            return {'predicate': command}

        def change(_server, command, expected, *, mode):
            trace.append(('change', command, expected, mode))
            return {'command': command, 'expected': expected, 'mode': mode}

        def read(*_args):
            trace.append(('read',))
            return chunk

        with patch.object(discs.suite, 'game_time', side_effect=[1457, 1457]), \
                patch.object(discs, 'require_frozen', side_effect=lambda _: trace.append(('frozen',))), \
                patch.object(discs, 'typed_predicate', side_effect=predicate), \
                patch.object(discs, 'command_result', side_effect=change), \
                patch.object(discs, 'read_fixture', side_effect=read), \
                patch.object(discs, 'retained_player', return_value={}), patch.object(discs, 'write_json'):
            result = discs.snapshot.__wrapped__(fixture, 'disc11_tick1440')
        marker = discs.pos(discs.SNAPSHOT_MARKER)
        self.assertEqual(trace, [
            ('live', f'data get block {discs.pos(discs.JUKEBOX)}'),
            ('live', f'data get block {discs.pos(discs.COMPARATOR)}'),
            ('frozen',), ('predicate', f'if loaded {marker}'),
            ('predicate', f'if block {marker} minecraft:air'),
            ('change', f'setblock {marker} minecraft:stone', 1, 'success'),
            ('change', f'setblock {marker} minecraft:air', 1, 'success'),
            ('predicate', f'if block {marker} minecraft:air'), ('flush',), ('read',)])
        self.assertEqual(discs.record_state(result['chunk']['typed']['jukebox']),
                         {'record': discs.stack(), 'elapsed': 1440})
        self.assertEqual(result['terrain_checkpoint']['world_time_after'], result['world_time'])
        self.assertEqual(len(result['terrain_checkpoint']['changes']), 2)
        self.assertIs(result, fixture.report['observations'][0])
        self.assertEqual(discs.SNAPSHOT_MARKER[0] // 16, discs.JUKEBOX[0] // 16)
        self.assertEqual(discs.SNAPSHOT_MARKER[2] // 16, discs.JUKEBOX[2] // 16)

    def test_failed_terrain_checkpoint_never_flushes_or_reads_a_stale_snapshot(self):
        for failure in ('frozen', 'unloaded', 'initial_air', 'stone', 'air', 'final_air', 'time'):
            server = SimpleNamespace(directory=Path('/SYNTHETIC'), command=Mock(return_value=[]), save_flush=Mock())
            fixture = SimpleNamespace(server=server, report={'active_boot': 'java_source'})
            predicates = []

            def predicate(_server, command):
                predicates.append(command)
                stage = ('unloaded', 'initial_air', 'final_air')[len(predicates) - 1]
                if failure == stage:
                    raise AssertionError('SYNTHETIC failed predicate')
                return {'predicate': command}

            def change(_server, command, expected, *, mode):
                self.assertEqual((expected, mode), (1, 'success'))
                if command.endswith('minecraft:' + failure):
                    raise AssertionError('SYNTHETIC real command returned zero')
                return {'command': command}

            with self.subTest(failure=failure), \
                    patch.object(discs.suite, 'game_time', side_effect=[1457, 1458 if failure == 'time' else 1457]), \
                    patch.object(discs, 'require_frozen', side_effect=AssertionError('SYNTHETIC unfrozen') if failure == 'frozen' else None), \
                    patch.object(discs, 'typed_predicate', side_effect=predicate), \
                    patch.object(discs, 'command_result', side_effect=change) as writes, \
                    patch.object(discs, 'read_fixture') as read:
                with self.assertRaises(AssertionError):
                    discs.snapshot.__wrapped__(fixture, 'disc11_tick1440')
                server.save_flush.assert_not_called()
                read.assert_not_called()
                self.assertEqual(writes.call_count, 0 if failure in ('frozen', 'unloaded', 'initial_air')
                                 else 1 if failure == 'stone' else 2)

    def test_unpadded_terrain_and_entity_records_keep_strict_payload_bounds(self):
        # Synthetic NBT boundaries, not runtime goldens. Java RegionFile pads on
        # close, while the campaign reads the still-open file after save-all flush.
        def named_int(name, value):
            key = name.encode()
            return b'\x03' + struct.pack('>H', len(key)) + key + struct.pack('>i', value)

        for entities in (False, True):
            with self.subTest(entities=entities), tempfile.TemporaryDirectory() as directory:
                world = Path(directory)
                region = world / ('entities' if entities else 'region') / 'r.8.0.mca'
                region.parent.mkdir()
                list_name = b'Entities' if entities else b'block_entities'
                raw = (b'\x0a\x00\x00' + named_int('DataVersion', 5023)
                       + named_int('xPos', 256) + named_int('zPos', 0)
                       + b'\x09' + struct.pack('>H', len(list_name)) + list_name
                       + b'\x0a\x00\x00\x00\x00\x00')
                compressed = zlib.compress(raw)
                record = struct.pack('>I', len(compressed) + 1) + b'\x02' + compressed
                header = ((2 << 8) | 1).to_bytes(4, 'big') + bytes(8188)
                valid = header + record
                self.assertLess(len(valid), 12288)
                region.write_bytes(valid)
                artifact = world / 'valid.nbt'
                result = discs.read_fixture(world, artifact, entities=entities)
                self.assertEqual(artifact.read_bytes(), raw)
                self.assertEqual(result['typed']['DataVersion'] if entities else result['typed'],
                                 discs.atom('Int', 5023) if entities else {})
                for index, damaged in enumerate((
                        valid[:100], b'\x00\x00\x01\x01' + valid[4:],
                        b'\x00\x00\x02\x00' + valid[4:], valid[:8195],
                        header + struct.pack('>I', 4093) + record[4:], valid[:-1])):
                    region.write_bytes(damaged)
                    with self.subTest(case=index), self.assertRaises(AssertionError):
                        discs.read_fixture(world, world / f'bad-{index}.nbt', entities=entities)

    def test_all_composites_require_boot_order_copies_provenance_cleanup_and_actual_observations(self):
        report = complete_report()
        self.assertEqual({row['result'] for row in discs.campaign_rows(report)}, {'passed'})
        for defect in ('order', 'partial', 'duplicate', 'provenance', 'cleanup', 'forced', 'client', 'wrong_teardown', 'copy',
                       'immutable', 'copy_order', 'registry', 'missing_patch', 'prediction', 'correction', 'slot_correction', 'duration'):
            broken = deepcopy(report)
            if defect == 'order':
                broken['completed_boots'].reverse()
            elif defect == 'partial':
                broken['completed_boots'].pop()
            elif defect == 'duplicate':
                broken['completed_boots'].append(discs.BOOTS[-1])
            elif defect == 'provenance':
                broken['provenance_checked_after'] = False
            elif defect == 'cleanup':
                broken['runtime_errors'] = ['forced shutdown']
            elif defect == 'forced':
                broken['boots']['rust_import']['teardown']['forced_stop'] = True
            elif defect == 'client':
                broken['boots']['rust_reload']['clients'].pop()
            elif defect == 'wrong_teardown':
                broken['boots']['rust_reload']['teardown']['boot'] = 'rust_import'
            elif defect == 'copy':
                broken['world_transfers'][0]['destination_after_copy']['files']['SYNTHETIC.dat'] = 'changed'
            elif defect == 'immutable':
                broken['world_transfers'][1]['source_immutable'] = False
            elif defect == 'copy_order':
                broken['world_transfers'].reverse()
            elif defect == 'registry':
                broken['boots']['java_return']['clients'][1]['registry']['payload_sha256'] = '0' * 64
            elif defect == 'missing_patch':
                broken['boots']['java_return']['exercise']['builtin_items'][1]['patch_present'] = False
            elif defect == 'prediction':
                broken['observations'][0]['accepted_prediction'] = False
            elif defect == 'correction':
                broken['observations'][0]['replies'] = [{'kind': 'content'}]
            elif defect == 'slot_correction':
                broken['observations'][0]['replies'] = [{'kind': 'slot'}]
            else:
                broken['observations'] = [r for r in broken['observations'] if r['kind'] != 'disc11_tick1441']
            with self.subTest(defect=defect):
                self.assertEqual({row['result'] for row in discs.campaign_rows(broken)}, {'failed'})

    def test_registry_validation_rejects_duplicate_nonfinite_custom_and_unbound_profiles(self):
        value = registry()
        discs.validate_registry(value)
        for defect in ('duplicate', 'short', 'inline', 'namespace', 'nan', 'infinity', 'bool_id', 'wrong_id', 'raw', 'length'):
            broken = deepcopy(value)
            if defect == 'duplicate':
                broken['entries'][1]['identifier'] = broken['entries'][0]['identifier']
            elif defect == 'short':
                broken['entries'].pop()
            elif defect == 'inline':
                broken['entries'][0]['identifier'] = {'song': 'minecraft:11'}
            elif defect == 'namespace':
                broken['entries'][0]['identifier'] = 'test:ward'
            elif defect in ('nan', 'infinity'):
                broken['entries'][0]['data']['length_in_seconds'] = float(defect.replace('infinity', 'inf'))
            elif defect == 'bool_id':
                broken['entries'][1]['registry_id'] = True
            elif defect == 'wrong_id':
                broken['entries'][0]['reference_id'] = 0
            elif defect == 'raw':
                broken['payload_base64'] = '!invalid'
            else:
                broken['decoded_bytes'] -= 1
            with self.subTest(defect=defect), self.assertRaises((AssertionError, ValueError)):
                discs.validate_registry(broken)

    def test_identifier_default_normalization_preserves_removal_and_opaque_data(self):
        base = discs.stack()
        for name in ('11', ':11', 'minecraft:11'):
            self.assertEqual(discs.semantic_stack({**base, 'components': {discs.COMPONENT: name}}), base)
        for value in (discs.stack(removed=True), discs.stack(opaque=True)):
            original = deepcopy(value)
            self.assertNotEqual(discs.semantic_stack(value), base)
            self.assertEqual(value, original)
        for value in ({**base, 'components': {discs.COMPONENT: 'test:11'}},
                      {**base, 'components': {discs.COMPONENT: {'song': discs.DEFAULT}}},
                      {**base, 'components': {discs.COMPONENT: discs.DEFAULT, '!' + discs.COMPONENT: {}}},
                      {**base, 'components': {'minecraft:custom_data': {'jukebox_playable': ':cat'}}}):
            with self.assertRaises(AssertionError):
                discs.semantic_stack(value)

    def test_inventory_types_counts_duplicates_and_wire_removals_fail_closed(self):
        entry = {'Slot': discs.atom('Byte', 0), **discs.stack(discs.CAT)}
        self.assertEqual(discs.item_slots(inventory(entry), 0, 26), {0: discs.stack(discs.CAT)})
        for values in ([entry, entry], [{**entry, 'Slot': discs.atom('Int', 0)}],
                       [{**entry, 'count': discs.atom('Int', True)}], [{**entry, 'count': discs.atom('Int', 2)}],
                       [{**entry, 'unknown': {}}], [{**entry, 'Slot': discs.atom('Byte', 27)}]):
            with self.assertRaises(AssertionError):
                discs.item_slots(inventory(*values), 0, 26)
        slots = discs.menu_slots()
        self.assertEqual(len(slots), 63)
        self.assertEqual(slots[56]['removed_components'], [discs.COMPONENT])
        self.assertEqual(slots[55]['components'], {'minecraft:custom_data': {'jukebox_playable': discs.CAT}})
        self.assertEqual(sum(s is not None for s in slots[:27]), 23)
        conflict = deepcopy(slots)
        conflict[5]['removed_components'] = [discs.COMPONENT]
        with self.assertRaises(AssertionError):
            discs.menu_semantics(conflict)

    def test_actual_ejected_item_must_match_record_including_count_and_components(self):
        for expected in (discs.stack(), discs.stack(discs.CAT), discs.stack(opaque=True)):
            actual = deepcopy(expected)
            discs.validate_ejected_item(actual, expected)
            self.assertEqual(actual, expected)
        for name in ('11', ':11', discs.DEFAULT):
            discs.validate_ejected_item({**discs.stack(), 'components': {discs.COMPONENT: name}}, discs.stack())
        wrong = (
            {'id': 'minecraft:stone', 'count': discs.atom('Int', 1)},
            {**discs.stack(), 'count': discs.atom('Int', 2)},
            {**discs.stack(), 'count': discs.atom('Byte', 1)},
            {**discs.stack(), 'count': discs.atom('Int', True)},
            discs.stack(discs.CAT), discs.stack(removed=True), discs.stack(opaque=True),
            {**discs.stack(), 'components': {'minecraft:custom_name': 'wrong'}},
        )
        for actual in wrong:
            with self.subTest(actual=actual), self.assertRaises(AssertionError):
                discs.validate_ejected_item(actual, discs.stack())
        for expected in (discs.stack(discs.CAT), discs.stack(opaque=True)):
            with self.assertRaises(AssertionError):
                discs.validate_ejected_item(discs.stack(), expected)

    def test_creative_recovery_rejects_wrong_saved_drop_before_kill(self):
        # Synthetic action inputs exercise the real recovery path, without a server.
        for actual, expected in (
                ({'id': 'minecraft:stone', 'count': discs.atom('Int', 1)}, discs.stack()),
                ({**discs.stack(), 'count': discs.atom('Int', 2)}, discs.stack()),
                (discs.stack(discs.CAT), discs.stack()), (discs.stack(), discs.stack(opaque=True))):
            server = SimpleNamespace(directory=Path('/SYNTHETIC'), command=Mock(return_value=['SYNTHETIC Item']), save_flush=Mock())
            fixture = SimpleNamespace(server=server, report={'active_boot': 'java_source'})
            entity = {'id': 'minecraft:item', 'UUID': {'nbt_type': 'IntArray', 'value': [1, 2, 3, 4]}, 'Item': actual,
                      'Pos': saved_pos(4100.0, 201.0, 1.0)}
            with self.subTest(actual=actual), patch.object(discs, 'read_fixture', return_value={'typed': {'Entities': inventory(entity)}}), \
                    patch.object(discs, 'retained_player', return_value={'player': {'typed': {'Pos': saved_pos(4098.5, 200.0, 3.5)}}}), \
                    patch.object(discs, 'typed_predicate', return_value={'synthetic': True}), patch.object(discs, 'command_result') as removal:
                with self.assertRaises(AssertionError):
                    discs.recover.__wrapped__(fixture, [], expected, creative=True)
                removal.assert_not_called()
                self.assertEqual(fixture.report['observations'][0]['drops'][0]['Item'], actual)
                server.command.assert_called_once()

    def test_offhand_intervening_record_rejects_silent_ejection_or_replacement(self):
        for ticks in (None, 2):
            original = {'id': 'minecraft:jukebox', 'RecordItem': discs.stack()}
            if ticks is not None:
                original['ticks_since_song_started'] = discs.atom('Long', ticks)
            discs.validate_offhand_record(original, discs.stack(), ticks)
            for record in (None, discs.stack(discs.CAT), {**discs.stack(), 'count': discs.atom('Int', 2)}):
                changed = deepcopy(original)
                if record is None:
                    del changed['RecordItem']
                else:
                    changed['RecordItem'] = record
                with self.subTest(ticks=ticks, record=record), self.assertRaises(AssertionError):
                    discs.validate_offhand_record(changed, discs.stack(), ticks)
            changed = {**original, 'ticks_since_song_started': discs.atom('Long', 0)}
            with self.assertRaises(AssertionError):
                discs.validate_offhand_record(changed, discs.stack(), ticks)

    def test_level_events_require_exact_raw_song_id_position_transport_and_single_delivery(self):
        reg = registry()
        binding = next(e for e in reg['entries'] if e['identifier'] == discs.CAT)
        event = {'packet_id': 47, 'event': 1010, 'x': 4100, 'y': 200, 'z': 1,
                 'data': binding['registry_id'], 'song': discs.CAT, 'global_event': False}
        discs.validate_events([event], [1010], reg, discs.CAT)
        for change in ({'packet_id': 118}, {'packet_id': 119}, {'data': binding['reference_id']},
                       {'event': 1011}, {'x': 4099}, {'global_event': True}, {'song': discs.DEFAULT}):
            with self.assertRaises(AssertionError):
                discs.validate_events([{**event, **change}], [1010], reg, discs.CAT)
        with self.assertRaises(AssertionError):
            discs.validate_events([event, event], [1010], reg, discs.CAT)
        with self.assertRaises(AssertionError):
            discs.validate_events([], [1010], reg, discs.CAT)
        discs.validate_events([], [], reg, None)

    def test_privacy_requires_retained_chunk_entry_and_empty_or_absent_tag(self):
        entry = {'x': 4100, 'y': 200, 'z': 1, 'block_entity_type': 4, 'data': None}
        for data in (None, {}):
            discs.validate_privacy([{'block_entities': [{**entry, 'data': data}]}], [], [discs.JUKEBOX])
        for entries in ([], [entry, entry], [{**entry, 'data': {'RecordItem': discs.stack()}}],
                        [{**entry, 'data': {'ticks_since_song_started': 0}}]):
            with self.assertRaises(AssertionError):
                discs.validate_privacy([{'block_entities': entries}], [], [discs.JUKEBOX])
        with self.assertRaises(AssertionError):
            discs.validate_privacy([], [{**entry, 'data': {'RecordItem': discs.stack()}}], [])
        discs.validate_privacy([], [{**entry, 'packet_id': 6, 'data': {}}], [])
        for change in ({'packet_id': 47}, {'block_entity_type': 1}, {'data': None}, {'y': True}):
            with self.assertRaises(AssertionError):
                discs.validate_privacy([], [{**entry, 'packet_id': 6, 'data': {}, **change}], [])

    def test_signed_elapsed_tracks_real_world_delta_without_normalizing_missing_to_zero(self):
        self.assertIsNone(discs.record_state(restore_snapshot(None)['chunk']['typed']['absent'])['elapsed'])
        self.assertEqual(discs.record_state(restore_snapshot(0)['chunk']['typed']['zero'])['elapsed'], 0)
        for before, after, ticks in ((-7, 13, 20), (0, 20, 20), (1439, 1440, 1), (1440, None, 1), (None, None, 20)):
            discs.validate_continuation(restore_snapshot(before), restore_snapshot(after, ticks), exact_ticks=ticks)
        for before, after, ticks in ((-7, 20, 20), (0, None, 20), (None, 0, 20), (1439, None, 1), (1440, 1441, 1)):
            with self.assertRaises(AssertionError):
                discs.validate_continuation(restore_snapshot(before), restore_snapshot(after, ticks), exact_ticks=ticks)
        with self.assertRaises(AssertionError):
            discs.validate_continuation(restore_snapshot(1439), restore_snapshot(None, 1))
        for raw in (None, discs.atom('Int', 0), discs.atom('Long', True), discs.atom('Long', 2**63)):
            with self.assertRaises(AssertionError):
                discs.elapsed({'ticks_since_song_started': raw})


class JukeboxRecoveryTests(unittest.TestCase):
    """Synthetic saved inputs exercise the actual recovery helper, never a server."""

    def setUp(self):
        self.server = SimpleNamespace(directory=Path('/SYNTHETIC'), command=Mock(return_value=['SYNTHETIC Item']), save_flush=Mock())
        self.fixture = SimpleNamespace(server=self.server, report={'active_boot': 'rust_import'})
        # Deliberately differs from the initial fixture POS: recovery must use
        # the current player from this flush, rather than the starting location.
        self.player = {'player': {'typed': {'Pos': saved_pos(4100.5, 200.0, 2.0)}}}
        self.entities = {'typed': {'Entities': inventory()}}
        self.entries = self.entities['typed']['Entities']['value']
        self.drop = {'id': 'minecraft:item', 'UUID': {'nbt_type': 'IntArray', 'value': [1, -2, 3, -4]},
                     'Pos': saved_pos(4100.757757004915, 201.01, 1.2855519824102464), 'Item': discs.stack()}
        self.ambient = {'id': 'minecraft:item', 'UUID': {'nbt_type': 'IntArray', 'value': [5, 6, 7, 8]},
                        'Pos': saved_pos(4109.208270328679, 65.0, 3.056599071602812),
                        'Item': {'id': 'minecraft:chicken', 'count': discs.atom('Int', 1)}}
        self.read = self.enterContext(patch.object(discs, 'read_fixture', return_value=self.entities))
        self.retained = self.enterContext(patch.object(discs, 'retained_player', return_value=self.player))
        self.predicate = self.enterContext(patch.object(discs, 'typed_predicate', side_effect=lambda _server, command: {'predicate': command}))
        self.validation = self.enterContext(patch.object(discs, 'validate_ejected_item', wraps=discs.validate_ejected_item))
        self.removal = self.enterContext(patch.object(discs, 'command_result', return_value={'synthetic': True}))
        self.advance = self.enterContext(patch.object(discs, 'advance'))
        self.actions = Mock()
        for name, mock in (('flush', self.server.save_flush), ('entities', self.read), ('player', self.retained),
                           ('predicate', self.predicate), ('validate_item', self.validation), ('remove', self.removal), ('advance', self.advance)):
            self.actions.attach_mock(mock, name)

    def test_recovery_scopes_saved_drops_to_live_sphere_and_keeps_all_evidence(self):
        chicken = {'id': 'minecraft:chicken', 'Pos': saved_pos(4101.579315597794, 57.0, 0.5620457922896476)}
        self.entries.extend([chicken, self.ambient, self.drop])
        original = deepcopy((self.entities, self.player))
        for creative in (True, False):
            with self.subTest(creative=creative):
                self.actions.reset_mock()
                self.server.command.reset_mock()
                self.fixture.report['observations'] = []
                discs.recover.__wrapped__(self.fixture, [], discs.stack(), creative=creative)
                evidence = self.fixture.report['observations'][0]
                self.assertIs(evidence['entities'], self.entities)
                self.assertEqual(evidence['drops'], [self.ambient, self.drop])
                self.assertEqual(evidence['nearby_drops'], [self.drop])
                self.assertEqual((self.entities, self.player), original)
                artifact = self.server.directory / 'rust_import-0001-drop.nbt'
                self.read.assert_called_once_with(self.server.directory / 'world', artifact, entities=True)
                self.retained.assert_called_once_with(self.server.directory / 'world', artifact)
                self.assertEqual([action[0] for action in self.actions.mock_calls],
                                 ['flush', 'entities', 'player', 'predicate', 'predicate', 'validate_item', 'remove']
                                 + ([] if creative else ['advance']) + ['predicate'])
                self.assertEqual([args[0][1] for args in self.predicate.call_args_list], [
                    f'at {discs.NAME} if entity @a[name={discs.NAME},x=4100.5,y=200.0,z=2.0,distance=..0]',
                    f'at {discs.NAME} if data entity @e[type=minecraft:item,distance=..8,limit=1,sort=nearest] {{UUID:[I;1,-2,3,-4]}}',
                    f'at {discs.NAME} unless entity @e[type=minecraft:item,distance=..8]'])
                self.validation.assert_called_once_with(self.drop['Item'], discs.stack())
                command = (f'kill @e[type=minecraft:item,distance=..8]' if creative
                           else f'tp @e[type=minecraft:item,distance=..8] {discs.NAME}')
                self.removal.assert_called_once_with(self.server, f'execute at {discs.NAME} run {command}', 1)
                if creative:
                    self.advance.assert_not_called()
                else:
                    self.advance.assert_called_once_with(self.fixture, [], 20)
                self.server.command.assert_called_once_with(
                    f'execute at {discs.NAME} run data get entity @e[type=minecraft:item,distance=..8,limit=1,sort=nearest] Item')

    def test_recovery_sphere_includes_exactly_eight_blocks_from_current_saved_player(self):
        # This item is outside eight blocks of the initial POS, but exactly
        # eight blocks above the current player saved by this recovery flush.
        self.drop['Pos'] = saved_pos(4100.5, 208.0, 2.0)
        self.entries.extend([self.ambient, self.drop])
        discs.recover.__wrapped__(self.fixture, [], discs.stack(), creative=True)
        self.assertEqual(self.fixture.report['observations'][0]['nearby_drops'], [self.drop])
        self.validation.assert_called_once_with(self.drop['Item'], discs.stack())
        self.removal.assert_called_once()

    def test_recovery_rejects_zero_or_multiple_nearby_items_before_removal(self):
        boundary = {**self.drop, 'Pos': saved_pos(4100.5, 208.0, 2.0)}
        diagonal = {**self.drop, 'Pos': saved_pos(4106.5, 206.0, 2.0)}
        just_outside = {**self.drop, 'Pos': saved_pos(4100.5, 208.000001, 2.0)}
        for entries in ([], [self.ambient], [diagonal], [just_outside], [self.ambient, self.drop, boundary]):
            with self.subTest(entries=entries), self.assertRaisesRegex(AssertionError, 'exactly one saved fixture drop within 8 blocks'):
                self.entries[:] = entries
                discs.recover.__wrapped__(self.fixture, [], discs.stack(), creative=True)
            self.validation.assert_not_called()
            self.removal.assert_not_called()
            self.advance.assert_not_called()
            self.assertEqual(self.fixture.report['observations'][-1]['drops'], entries)

    def test_recovery_rejects_nearby_wrong_items_even_with_expected_item_outside(self):
        for actual, expected in (
                ({'id': 'minecraft:stone', 'count': discs.atom('Int', 1)}, discs.stack()),
                ({**discs.stack(), 'count': discs.atom('Int', 2)}, discs.stack()),
                (discs.stack(discs.CAT), discs.stack()), (discs.stack(), discs.stack(opaque=True)),
                (discs.stack(opaque=True), discs.stack())):
            with self.subTest(actual=actual, expected=expected):
                self.actions.reset_mock()
                self.entries[:] = [{**self.ambient, 'Item': expected}, {**self.drop, 'Item': actual}]
                with self.assertRaises(AssertionError):
                    discs.recover.__wrapped__(self.fixture, [], expected, creative=True)
                self.validation.assert_called_once_with(actual, expected)
                self.assertIn('UUID:[I;1,-2,3,-4]', self.predicate.call_args[0][1])
                self.removal.assert_not_called()
                self.advance.assert_not_called()
                self.assertEqual(self.fixture.report['observations'][-1]['drops'], self.entries)

    def test_recovery_rejects_malformed_or_nonfinite_player_and_drop_positions_before_removal(self):
        valid = self.player['player']['typed']['Pos']
        malformed = (None, [4100.5, 200.0, 2.0], {**valid, 'element_type': 5},
                     {**valid, 'element_type': 6.0}, saved_pos(4100.5, 200.0), saved_pos(4100.5, 200.0, 2.0, 0.0),
                     {**valid, 'value': [discs.atom('Float', 4100.5), *valid['value'][1:]]},
                     {**valid, 'extra': True}, saved_pos(4100, 200.0, 2.0), saved_pos(True, 200.0, 2.0),
                     saved_pos('4100.5', 200.0, 2.0), saved_pos(float('nan'), 200.0, 2.0),
                     saved_pos(4100.5, float('inf'), 2.0), saved_pos(4100.5, 200.0, -float('inf')))
        for subject in ('player', 'drop', 'ambient'):
            for position in malformed:
                with self.subTest(subject=subject, position=position):
                    self.player['player']['typed']['Pos'] = position if subject == 'player' else valid
                    self.entries[:] = [{**self.ambient, **({'Pos': position} if subject == 'ambient' else {})},
                                       {**self.drop, **({'Pos': position} if subject == 'drop' else {})}]
                    with self.assertRaisesRegex(AssertionError, 'Invalid saved Pos'):
                        discs.recover.__wrapped__(self.fixture, [], discs.stack(), creative=True)
                    self.validation.assert_not_called()
                    self.removal.assert_not_called()
                    self.advance.assert_not_called()

    def test_recovery_refuses_failed_live_player_or_uuid_binding_before_removal(self):
        self.entries.extend([self.ambient, self.drop])
        for failure in ('distance=..0', 'UUID:'):
            def predicate(_server, command):
                if failure in command:
                    raise AssertionError('SYNTHETIC live binding mismatch')
                return {'predicate': command}

            with self.subTest(failure=failure):
                self.predicate.side_effect = predicate
                with self.assertRaisesRegex(AssertionError, 'live binding mismatch'):
                    discs.recover.__wrapped__(self.fixture, [], discs.stack(), creative=True)
                self.validation.assert_not_called()
                self.removal.assert_not_called()
                self.advance.assert_not_called()


if __name__ == '__main__':
    unittest.main()
