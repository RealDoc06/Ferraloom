"""Synthetic helper-validator tests, never runtime packets or Java hash goldens."""
import base64
from copy import deepcopy
import hashlib
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
import check_jukeboxes as discs


def inventory(*entries):
    return {'nbt_type': 'List', 'element_type': 10, 'value': list(entries)}


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
            entity = {'id': 'minecraft:item', 'UUID': {'nbt_type': 'IntArray', 'value': [1, 2, 3, 4]}, 'Item': actual}
            with self.subTest(actual=actual), patch.object(discs, 'read_fixture', return_value={'typed': {'Entities': inventory(entity)}}), \
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


if __name__ == '__main__':
    unittest.main()
