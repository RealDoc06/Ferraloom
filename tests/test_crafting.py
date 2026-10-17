"""Pure synthetic validator inputs, not executed servers or captured packet gold."""
import base64
from contextlib import nullcontext
from copy import deepcopy
import hashlib
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
import check_crafting as craft


def envelope(hex_bytes, **fields):
    raw = bytes.fromhex(hex_bytes)
    return {'payload_base64': base64.b64encode(raw).decode(), 'payload_sha256': hashlib.sha256(raw).hexdigest(), **fields}


def state(revision, grid=None, result=None, cursor=None, hotbar=None):
    slots = [None] * 46
    slots[0], slots[4], slots[36] = result, grid, hotbar
    return {'active': 'player', 'player': {'container_id': 0, 'kind': 'player', 'state_id': revision,
            'revision_sequence': 10, 'baseline_sequence': 1, 'slots': slots},
            'table': None, 'cursor': cursor, 'cursor_sequence': 10}


def phase(purpose, request_hex, before, after, mark, replies, action, slot):
    response = envelope('0300', packet_id=3, sequence=mark + 10, payload_bytes=2, decoded_bytes=2, entries=[])
    fence = {'request': {'written': True, 'packet_id': 12, 'action_id': 1, 'after_sequence': mark},
             'response': response, 'received': after}
    request = envelope(request_hex, written=True, changed_slots={}, before=before, after_sequence=mark,
                       input=action, button=0, slot=slot)
    wire = [envelope(request_hex, direction='serverbound', packet_id=18, pid=1),
            envelope('0c01', direction='serverbound', packet_id=12, pid=1)]
    observations = []
    for number, (hex_bytes, packet_id) in enumerate(replies, mark + 1):
        wire.append(envelope(hex_bytes, direction='clientbound', packet_id=packet_id, sequence=number, pid=1))
        observations.append({'packet_id': packet_id, 'sequence': number})
    wire.append({**response, 'direction': 'clientbound', 'pid': 1})
    return {'kind': 'click', 'purpose': purpose, 'complete': True, 'request': request,
            'fence': fence, 'received': after, 'wire': wire, 'observations': observations}


def sample_case():
    # Item1 is a synthetic stand-in in the decoded view only. The request has
    # an empty cursor, hence no fabricated item/component hash on the wire.
    honey, sugar, bottle = {'item_id': 1, 'count': 1}, {'item_id': 1217, 'count': 3}, {'item_id': 1270, 'count': 1}
    start = state(7, hotbar=honey)
    prepared = state(10, grid=honey, result=sugar)
    extracted = state(12, grid=bottle, cursor=sugar)
    setup = phase('grid_setup', '120007000400020000', start, prepared, 10,
                  [('140008000401010000', 20), ('140009002400', 20), ('14000a000003c1090000', 20)], 'swap', 4)
    take = phase('extract', '12000a000000000000', prepared, extracted, 20,
                 [('14000b000000', 20), ('14000c000401f6090000', 20), ('6203c1090000', 98)], 'pickup', 0)
    zero = {'sugar': 0, 'honey_block': 0, 'cake': 0}
    before = craft.expected_view('player', {4: craft.stack('honey_bottle')}, result=craft.stack('sugar', 3))
    after = craft.expected_view('player', {4: craft.stack('glass_bottle')}, cursor=craft.stack('sugar', 3))
    checks = [{'label': 'prepared', 'actual': before, 'expected': deepcopy(before), 'drops': {}, 'expected_drops': {},
               'before_stats': zero, 'after_stats': zero, 'awards': {}, 'validated': True},
              {'label': 'extracted', 'actual': after, 'expected': deepcopy(after), 'drops': {}, 'expected_drops': {},
               'before_stats': zero, 'after_stats': {**zero, 'sugar': 3}, 'awards': {'sugar': 3}, 'validated': True}]
    return {'name': 'player_offset_left', 'result': 'passed', 'evidence_retained': True, 'placements': [(4, 0)],
            'phases': [setup, take], 'checks': checks, 'timeline': ['evidence_retained', 'assertions_passed']}


def drop(item='sugar', count=3, identity=(1, 2, 3, 4)):
    return {'id': 'minecraft:item', 'Item': {'id': 'minecraft:' + item, 'count': craft.atom('Int', count)},
            'UUID': {'nbt_type': 'IntArray', 'value': list(identity)},
            'Pos': {'nbt_type': 'List', 'element_type': 6,
                    'value': [craft.atom('Double', value) for value in (4104.5, 201.5, 8.5)]}}


class CakePreparationTests(unittest.TestCase):
    def setUp(self):
        # Fixed SYNTHETIC boundary replies, not packet gold or an inventory engine.
        # Commands and clicks only record calls; they never mutate these replies.
        self.trace = []
        self.items = {name: index for index, name in enumerate(craft.ITEMS, 1)}
        self.empty = state(1)
        seeded = deepcopy(self.empty)
        seeded['player']['slots'][45] = {'item_id': self.items['cobblestone'], 'count': 64}
        opened = {**deepcopy(seeded), 'active': 'table', 'table': {
            'container_id': 4, 'kind': 'table', 'state_id': 1, 'baseline_sequence': 1,
            'revision_sequence': 10, 'slots': [None] * 46}}
        ingredients = [{'item_id': self.items[name], 'count': 1} for name in
            ('milk_bucket', 'milk_bucket', 'milk_bucket', 'sugar', 'egg', 'sugar', 'wheat', 'wheat', 'wheat')]
        frames = []
        for placed in range(10):
            frame = deepcopy(opened)
            frame['table']['slots'][1:10] = ingredients[:placed] + [None] * (9 - placed)
            frame['table']['slots'][37:46] = [None] * placed + ingredients[placed:]
            frame['player']['slots'][36:45] = [None] * placed + ingredients[placed:]
            frames.append(frame)
        frames[-1]['table']['slots'][0] = {'item_id': self.items['cake'], 'count': 1}
        full = deepcopy(frames[-1])
        full['table']['slots'][10:46] = [{'item_id': self.items['cobblestone'], 'count': 64}] * 36
        full['player']['slots'][9:45] = deepcopy(full['table']['slots'][10:46])
        space = deepcopy(full)
        space['table']['slots'][45] = space['player']['slots'][44] = None
        replies = iter((seeded, opened, frames[0], full, space))
        fences = iter((seeded, opened, frames[0], full, space))
        swaps = iter(zip(frames, frames[1:]))
        zero = dict.fromkeys(craft.CRAFTED, 0)

        def observe(predicate):
            received = next(replies)
            self.trace.append(('observe', received['active']))
            response = {'crafting': received, 'crafting_packets': [
                {'kind': 'open', 'container_id': 4, 'menu_type': 12, 'sequence': 11},
                {'kind': 'content', 'container_id': 4, 'sequence': 12}]}
            self.assertTrue(predicate(response), 'SYNTHETIC required received inventory missing')
            return response

        def fence():
            received = next(fences)
            self.trace.append(('fence', received['active']))
            return {'received': received, 'after_sequence': 20}

        def click(purpose, slot, action, button=0):
            self.trace.append(('click', purpose, slot, action, button))
            if purpose == 'grid_setup':
                before, after = next(swaps)
                return {'request': {'before': before}, 'received': after, 'crafted_counts': zero}

        server = SimpleNamespace(directory=Path('/SYNTHETIC-cake-prepare'),
            save_flush=Mock(side_effect=lambda: self.trace.append(('flush',))),
            command=Mock(side_effect=lambda command: self.trace.append(('command', command)) or ['SYNTHETIC command success']))
        self.client = SimpleNamespace(events=[], watch_statistic=Mock(), state=Mock(return_value={'packets': 10}),
            wait_state=Mock(side_effect=observe), crafting_fence=Mock(side_effect=fence),
            request=Mock(side_effect=lambda action, **fields: self.trace.append(('open', action)) or {'SYNTHETIC': True}))
        fixture = SimpleNamespace(server=server,
            report={'active_boot': 'SYNTHETIC', 'boots': {'SYNTHETIC': {'cases': {}}}})
        self.probe = craft.Probe(fixture, self.client, {'items': self.items, 'crafted_category': 0})
        self.probe.retain = Mock()
        self.probe.capture = Mock(side_effect=lambda label: self.trace.append(('capture', label)) or {
            'actual': craft.view(self.empty, self.items), 'drop_totals': {}})
        self.probe.click = Mock(side_effect=click)
        self.probe.check = Mock(side_effect=lambda label, expected, **fields: self.trace.append(('check', label)))
        self.probe.reset = Mock()
        for replacement in (
            patch.object(craft, 'retained_player', side_effect=lambda *_: self.trace.append(('saved_baseline',)) or {
                'SYNTHETIC': True, 'statistics': {'data': {'stats': {}}}}),
            patch.object(craft, 'interaction_deadline', side_effect=lambda *_: nullcontext()),
        ):
            self.addCleanup(replacement.stop)
            replacement.start()

    def test_cake_prepare_observes_offhand_before_table_swaps_and_main_fill(self):
        self.probe.exercise('cake_shift')
        command = f'item replace entity {craft.NAME} weapon.offhand with minecraft:cobblestone 64'
        self.assertEqual(self.trace[:8], [('flush',), ('saved_baseline',), ('capture', 'baseline'),
            ('command', command), ('observe', 'player'), ('fence', 'player'), ('open', 'use_item_on'), ('observe', 'table')])
        self.client.request.assert_called_once_with('use_item_on', **craft.TABLE)
        self.assertEqual([row for row in self.trace if row[0] == 'click'],
            [('click', 'grid_setup', slot, 'swap', slot - 1) for slot in range(1, 10)] +
            [('click', 'reject_shift', 0, 'quick_move', 0), ('click', 'extract', 0, 'quick_move', 0)])
        phases = self.probe.case['phases']
        self.assertEqual([row['kind'] for row in phases],
            ['inventory_setup', 'table_open', 'inventory_setup', 'inventory_setup', 'inventory_setup'])
        self.assertTrue(all(row['complete'] for row in phases))
        self.assertEqual(phases[0]['entries'], {40: craft.stack('cobblestone', 64)})
        self.assertEqual(list(phases[2]['entries']), list(range(9)))
        self.assertEqual(phases[3]['entries'], dict.fromkeys(range(36), craft.stack('cobblestone', 64)))
        self.assertEqual(phases[4]['entries'], {8: None})
        first_swap = self.trace.index(('click', 'grid_setup', 1, 'swap', 0))
        prepared = self.trace.index(('check', 'prepared'))
        reject = self.trace.index(('click', 'reject_shift', 0, 'quick_move', 0))
        self.assertEqual(self.trace[8:first_swap], [('fence', 'table')] +
            [('command', row['command']) for row in phases[2]['commands']] + [('observe', 'table'), ('fence', 'table')])
        self.assertEqual(self.trace[first_swap:prepared],
            [('click', 'grid_setup', slot, 'swap', slot - 1) for slot in range(1, 10)])
        self.assertEqual(self.trace[prepared + 1:reject], [('command',
            f'item replace entity {craft.NAME} ' + (f'hotbar.{index}' if index < 9 else f'inventory.{index - 9}') +
            ' with minecraft:cobblestone 64') for index in range(36)] + [('observe', 'table'), ('fence', 'table')])
        kind, grid, result = craft.recipe_fixture('cake_shift')
        self.assertEqual(self.probe.check.call_args_list[0].args,
            ('prepared', craft.expected_view(kind, grid, result=result, offhand=craft.stack('cobblestone', 64))))
        self.assertEqual(sum('weapon.offhand' in row['command'] for phase in phases for row in phase.get('commands', [])), 1)

    def test_cake_prepare_rejects_missing_player45_before_table_open(self):
        def absent(predicate):
            self.assertFalse(predicate({'crafting': self.empty}))
            raise AssertionError('SYNTHETIC missing received player45')

        self.client.wait_state.side_effect = absent
        with self.assertRaisesRegex(AssertionError, 'SYNTHETIC missing received player45'):
            self.probe.prepare('cake_shift')
        self.client.request.assert_not_called()
        self.client.crafting_fence.assert_not_called()
        self.probe.click.assert_not_called()
        self.probe.check.assert_not_called()
        self.assertEqual(len(self.probe.case['phases']), 1)
        self.assertFalse(self.probe.case['phases'][0]['complete'])
        self.assertEqual(len(self.probe.case['phases'][0]['commands']), 1)

    def test_cake_prepare_rechecks_player45_at_setup_fence_before_table_open(self):
        self.client.crafting_fence.side_effect = None
        self.client.crafting_fence.return_value = {'received': self.empty}
        with self.assertRaises(AssertionError):
            self.probe.prepare('cake_shift')
        self.client.request.assert_not_called()
        self.probe.click.assert_not_called()
        self.probe.check.assert_not_called()
        self.assertFalse(self.probe.case['phases'][0]['complete'])


class CraftingValidatorTests(unittest.TestCase):
    def test_exercise_predeclares_requested_awards_and_preserves_whole_physical_output(self):
        # Capture the real exercise's declarations only. No inventory simulation or runtime gold.
        class DeclarationProbe(craft.Probe):
            def __init__(self):
                self.checks, self.clicks = [], []

            def prepare(self, name):
                self.case = {'name': name, 'timeline': []}
                return craft.recipe_fixture(name)

            def inventory(self, entries):
                pass

            def click(self, purpose, slot, action, button=0):
                self.clicks.append((purpose, slot, action, button))

            def check(self, label, expected, *, drops=None, awards=None):
                self.checks.append((label, expected, drops or {}, awards or {}))

            def retain(self):
                pass

            def reset(self):
                pass

        expected = {
            'player_offset_left': {'sugar': 3}, 'player_offset_right': {'sugar': 2},
            'table_offset_left': {'sugar': 3}, 'table_offset_right': {'sugar': 2},
            'full_cursor': {'sugar': 3}, 'throw_once': {'sugar': 1}, 'throw_all': {'sugar': 9},
            'player_four_honey': {'honey_block': 1}, 'table_four_honey': {'honey_block': 1},
            'cake_shift': {'cake': 1}, 'offhand_partial': {'sugar': 1},
            'offhand_spill': {'sugar': 1}, 'selected_before_offhand': {'sugar': 1},
        }
        self.assertEqual(tuple(expected), craft.CASES)
        self.assertEqual(len(craft.CASES), 13)
        totals = dict.fromkeys(craft.CRAFTED, 0)
        for name, awards in expected.items():
            with self.subTest(name=name):
                probe = DeclarationProbe()
                probe.exercise(name)
                extracted = [row for row in probe.checks if row[0] == 'extracted']
                self.assertEqual(len(extracted), 1)
                _, actual, drops, declared = extracted[0]
                self.assertEqual(declared, awards)
                self.assertEqual(probe.clicks[-1], craft.required_clicks(name)[-1])
                if name.endswith(('_left', '_right')) or name == 'full_cursor':
                    self.assertEqual(actual['cursor'], craft.stack('sugar', 3))
                if name in ('throw_once', 'offhand_partial', 'offhand_spill', 'selected_before_offhand'):
                    self.assertEqual(drops['minecraft:sugar'], 3)
                if name == 'throw_all':
                    self.assertEqual(drops['minecraft:sugar'], 9)
                for item, count in declared.items():
                    totals[item] += count
        self.assertEqual(totals, {'sugar': 26, 'honey_block': 2, 'cake': 1})

    def test_requested_awards_reject_returned_stack_count_for_right_pickup_and_throw_zero(self):
        zero = dict.fromkeys(craft.CRAFTED, 0)
        for requested in (2, 1):
            with self.subTest(requested=requested):
                expected = craft.expected_view('player', {4: craft.stack('glass_bottle')},
                    cursor=craft.stack('sugar', 3) if requested == 2 else None)
                drops = {} if requested == 2 else {'minecraft:sugar': 3}
                awards = {'sugar': requested}
                craft.validate_outcome(expected, expected, drops, drops, zero, {**zero, **awards}, awards)
                with self.assertRaises(AssertionError):
                    craft.validate_outcome(expected, expected, drops, drops, zero, {**zero, 'sugar': 3}, awards)

    def test_exact_outcome_rejects_partial_output_missing_remainder_and_false_rejection(self):
        expected = craft.expected_view('player', {4: craft.stack('glass_bottle')}, cursor=craft.stack('sugar', 3))
        zero = {'sugar': 0, 'honey_block': 0, 'cake': 0}
        craft.validate_outcome(expected, expected, {}, {}, zero, {**zero, 'sugar': 3}, {'sugar': 3})
        for defect in ('output', 'bottle', 'cursor', 'award', 'spill'):
            actual = deepcopy(expected)
            after, drops = {**zero, 'sugar': 3}, {}
            if defect == 'output':
                actual['cursor']['count'] = 2
            elif defect == 'bottle':
                actual['grid'][3] = None
            elif defect == 'cursor':
                actual['main'][0] = craft.stack('sugar', 3)
            elif defect == 'award':
                after['sugar'] = 6
            else:
                drops = {'minecraft:glass_bottle': 1}
            with self.subTest(defect=defect), self.assertRaises(AssertionError):
                craft.validate_outcome(actual, expected, drops, {}, zero, after, {'sugar': 3})
        rejected = craft.expected_view('player', {4: craft.stack('honey_bottle')}, result=craft.stack('sugar', 3), cursor=craft.stack('sugar', 64))
        with self.assertRaises(AssertionError):
            craft.validate_outcome(rejected, rejected, {}, {}, zero, {**zero, 'sugar': 3}, {})

    def test_saved_default_stacks_keep_typed_counts_and_separate_equipment_offhand(self):
        player = {'Inventory': {'nbt_type': 'List', 'element_type': 10, 'value': [
            {'Slot': craft.atom('Byte', 9), 'id': 'minecraft:glass_bottle', 'count': craft.atom('Int', 63)}]},
            'equipment': {'offhand': {'id': 'minecraft:glass_bottle', 'count': craft.atom('Int', 64), 'components': {}}}}
        main, offhand = craft.player_items(player)
        self.assertEqual(main[9], craft.stack('glass_bottle', 63))
        self.assertEqual(offhand, craft.stack('glass_bottle', 64))
        self.assertEqual(sum(value['count'] for value in main if value), 63)
        for change in ({'components': {'minecraft:custom_data': {}}}, {'count': craft.atom('Byte', 64)},
                       {'count': craft.atom('Int', 0)}, {'count': craft.atom('Int', 65)}, {'unknown': 0}):
            with self.subTest(change=change), self.assertRaises(AssertionError):
                craft.saved_stack({**player['equipment']['offhand'], **change})
        legacy = deepcopy(player)
        legacy['Inventory']['value'][0]['Slot'] = craft.atom('Byte', -106)
        with self.assertRaises(AssertionError):
            craft.player_items(legacy)

    def test_drop_totals_require_typed_items_unique_uuid_and_the_retained_chunk(self):
        first, second = drop(), drop('glass_bottle', 1, (5, 6, 7, 8))
        totals, identities = craft.validate_drop_entities([first, second])
        self.assertEqual(totals, {'minecraft:glass_bottle': 1, 'minecraft:sugar': 3})
        self.assertEqual(len(set(identities)), 2)
        for defect in ('duplicate', 'uuid', 'component', 'count', 'outside', 'coordinate_type'):
            bad = deepcopy(second)
            if defect == 'duplicate':
                bad['UUID'] = first['UUID']
            elif defect == 'uuid':
                bad['UUID']['value'] = [1, 2, 3]
            elif defect == 'component':
                bad['Item']['components'] = {'minecraft:custom_name': 'tagged'}
            elif defect == 'count':
                bad['Item']['count'] = craft.atom('Byte', 1)
            elif defect == 'outside':
                bad['Pos']['value'][0] = craft.atom('Double', 4112.0)
            else:
                bad['Pos']['value'][1] = craft.atom('Float', 201.5)
            with self.subTest(defect=defect), self.assertRaises(AssertionError):
                craft.validate_drop_entities([first, bad])

    def test_fence_is_a_later_raw_awardstats_not_a_queued_write_or_an_omitted_counter(self):
        fence = sample_case()['phases'][0]['fence']
        craft.validate_fence(fence, 10)
        for defect in ('queued_only', 'stale', 'wrong_request', 'wrong_hash'):
            bad = deepcopy(fence)
            if defect == 'queued_only':
                bad['response'] = {'written': True}
            elif defect == 'stale':
                bad['response']['sequence'] = 10
            elif defect == 'wrong_request':
                bad['request']['action_id'] = 0
            else:
                bad['response']['payload_sha256'] = '0' * 64
            with self.subTest(defect=defect), self.assertRaises(AssertionError):
                craft.validate_fence(bad, 10)
        # The empty AwardStats is a fence only. It cannot prove a sugar award.
        value = craft.expected_view('player', {})
        zero = {'sugar': 0, 'honey_block': 0, 'cake': 0}
        with self.assertRaises(AssertionError):
            craft.validate_outcome(value, value, {}, {}, zero, zero, {'sugar': 3})

    def test_click_validator_rejects_wrong_preaction_hash_missing_updates_and_cross_connection_fences(self):
        good = sample_case()['phases'][0]
        craft.validate_click(good)
        for defect in ('revision', 'cursor', 'predictions', 'missing_raw', 'wrong_order', 'other_client', 'lost_updates'):
            bad = deepcopy(good)
            if defect == 'revision':
                bad['request']['before']['player']['state_id'] = 8
            elif defect == 'cursor':
                bad['request']['before']['cursor'] = {'item_id': 1217, 'count': 64}
            elif defect == 'predictions':
                bad['request']['changed_slots'] = {'4': None}
            elif defect == 'missing_raw':
                bad['wire'].pop()
            elif defect == 'wrong_order':
                bad['wire'][0], bad['wire'][1] = bad['wire'][1], bad['wire'][0]
            elif defect == 'other_client':
                bad['wire'][-1]['pid'] = 2
            else:
                bad['observations'] = []
            with self.subTest(defect=defect), self.assertRaises(AssertionError):
                craft.validate_click(bad)

    def test_case_requires_both_actual_setup_and_extraction_with_all_outcome_phases(self):
        good = sample_case()
        craft.validate_case(good)
        for defect in ('no_setup', 'no_extraction', 'no_fence', 'no_outcome', 'uncompleted', 'unretained'):
            bad = deepcopy(good)
            if defect == 'no_setup':
                bad['phases'].pop(0)
            elif defect == 'no_extraction':
                bad['phases'].pop()
            elif defect == 'no_fence':
                bad['phases'][-1]['fence']['response']['sequence'] = 20
            elif defect == 'no_outcome':
                bad['checks'].pop()
            elif defect == 'uncompleted':
                bad['phases'][-1]['complete'] = False
            else:
                bad['evidence_retained'] = False
            with self.subTest(defect=defect), self.assertRaises(AssertionError):
                craft.validate_case(bad)

    def test_cleanup_cannot_precede_evidence_or_follow_failed_expectations(self):
        good = sample_case()
        good['timeline'] += ['cleanup_started', 'cleanup_complete']
        craft.validate_case(good)
        for timeline in (['assertions_passed', 'evidence_retained', 'cleanup_started', 'cleanup_complete'],
                         ['evidence_retained', 'cleanup_started', 'assertions_passed', 'cleanup_complete'],
                         ['evidence_retained', 'assertions_passed', 'failure', 'cleanup_started', 'cleanup_complete'],
                         ['evidence_retained', 'assertions_passed', 'cleanup_started']):
            with self.subTest(timeline=timeline), self.assertRaises(AssertionError):
                craft.validate_case({**good, 'timeline': timeline})

    def test_campaign_never_publishes_partial_rows_without_both_boots_pins_and_clean_lifecycle(self):
        # Deliberately incomplete evidence. No synthetic success report is used
        # as runtime gold; each omission must keep every composite failed.
        source = {'commit': 'a' * 40, 'dirty': False}
        report = {'completed_boots': list(craft.BOOTS), 'boots': {name: {} for name in craft.BOOTS},
                  'provenance_checked_before': True, 'provenance_checked_after': True,
                  'source': source, 'binary': {'source_commit': 'b' * 40, 'sha256': 'c' * 64},
                  'official': {'inner_sha256': craft.INNER_JAR_SHA256}, 'java': {'sha256': craft.JDK_SHA256},
                  'contract': {'primary': {name: {'sha256': pin} for name, pin in craft.PRIMARY.items()},
                               'reports': craft.REPORT_PINS, 'wire_original_result': 'failed', 'wire_post_validation_result': 'passed'},
                  'manifest': {'SYNTHETIC': 'd' * 64}, 'cleanup_complete': False, 'runtime_errors': []}
        with self.assertRaisesRegex(AssertionError, 'Unclean campaign lifecycle'):
            craft.validate_campaign(report)
        first = {'result': 'passed', 'engine': 'vanilla',
                 'clients': [{'name': craft.NAME, 'pid': 2, 'argv': ['node', '--crafting']}],
                 'teardown': {'boot': craft.BOOTS[0], 'pid': 1, 'argv': ['SYNTHETIC'],
                              'started': '2026-01-01T00:00:00Z', 'stopped': '2026-01-01T00:01:00Z',
                              'exit_code': 0, 'forced_stop': True,
                              'clients': [{'pid': 2, 'exit_code': 0, 'forced_close': False, 'failure': None}]}}
        lifecycle = {**report, 'cleanup_complete': True, 'boots': {craft.BOOTS[0]: first, craft.BOOTS[1]: {}}}
        with self.assertRaisesRegex(AssertionError, 'Unclean server teardown'):
            craft.validate_campaign(lifecycle)
        first['teardown']['forced_stop'] = False
        first['teardown']['clients'][0]['forced_close'] = True
        with self.assertRaisesRegex(AssertionError, 'Unclean client teardown'):
            craft.validate_campaign(lifecycle)
        for change in ({}, {'completed_boots': [craft.BOOTS[0]]}, {'completed_boots': list(reversed(craft.BOOTS))},
                       {'provenance_checked_after': False}, {'source': {**source, 'dirty': True}},
                       {'runtime_errors': ['forced shutdown']}, {'cleanup_complete': True}):
            rows = craft.campaign_rows({**report, **change})
            self.assertEqual(len(rows), len(craft.CASES))
            self.assertEqual({row['result'] for row in rows}, {'failed'})


if __name__ == '__main__':
    unittest.main()
