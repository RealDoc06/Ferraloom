"""Synthetic false-positive controls only. No runtime goldens; all tests UNRUN."""
from copy import deepcopy
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
sys.path.insert(0, str(Path(__file__).resolve().parent))
import rea_crafting_case as bridge
from test_crafting import drop, envelope, phase

craft = bridge.craft
ITEMS = {'honey_bottle': 1, 'sugar': 1217, 'glass_bottle': 1270, 'honey_block': 2, 'cake': 3}
ZERO = dict.fromkeys(craft.CRAFTED, 0)
# Only the admitted source ID is real; these receipts/bytes are hand-authored.
SYNTHETIC_SOURCE = bridge.ENGINE_SOURCE_COMMIT
SYNTHETIC_BINARY = '2' * 64
OLD_SOURCES = ('a951d03e175831d5220fbd741ad488bfc9b6745e',
               '67ffe67deb49dbb3719d4f3974fc1c8a92cd2980',
               'ae26989de759558a2d1cca77f83610de75f220b1')


def state(active: str, revision: int, *, grid=None, result=None, cursor=None, main=None) -> dict:
    player = [None] * 46
    if main:
        player[36:45], player[9:36] = main[:9], main[9:]
    menu = {'container_id': 0, 'kind': 'player', 'state_id': revision,
            'baseline_sequence': 1, 'revision_sequence': 2, 'slots': player}
    table = None
    if active == 'table':
        slots = [None] * 46
        slots[0], slots[9] = result, grid
        slots[37:46], slots[10:37] = player[36:45], player[9:36]
        table = {**menu, 'container_id': 7, 'kind': 'table', 'slots': slots}
    return {'active': active, 'player': menu, 'table': table, 'cursor': cursor, 'cursor_sequence': 2}


def saved(view: dict, sugar: int) -> dict:
    inventory = [{'Slot': craft.atom('Byte', index), 'id': value['item'], 'count': craft.atom('Int', value['count'])}
                 for index, value in enumerate(view['main']) if value is not None]
    return {'player': {'typed': {'Inventory': {'nbt_type': 'List', 'element_type': 10, 'value': inventory},
        'DataVersion': craft.atom('Int', 5023), 'SelectedItemSlot': craft.atom('Int', 0),
        'UUID': {'nbt_type': 'IntArray', 'value': list(craft.UUID_INTS)},
        'Pos': {'nbt_type': 'List', 'element_type': 6, 'value': [craft.atom('Double', n) for n in (4104.5, 200.0, 8.5)]},
        'Rotation': {'nbt_type': 'List', 'element_type': 5, 'value': [craft.atom('Float', 0.0)] * 2},
        'Dimension': 'minecraft:overworld', 'playerGameType': craft.atom('Int', 0)}},
        'statistics': {'data': {'DataVersion': 5023, 'stats': {'minecraft:crafted': {'minecraft:sugar': sugar}}}}}


def fence(received: dict, mark: int, sugar: int) -> dict:
    raw = b'\x03\x01\x01\xc1\x09' + craft.protocol.varint(sugar) if sugar else b'\x03\x00'
    response = envelope(raw.hex(), packet_id=3, sequence=mark + 10,
                        payload_bytes=len(raw), decoded_bytes=len(raw),
                        entries=[{'category_id': 1, 'statistic_id': 1217, 'value': sugar}] if sugar else [])
    return {'request': {'written': True, 'packet_id': 12, 'action_id': 1, 'after_sequence': mark},
            'response': response, 'received': received, 'after_sequence': mark + 10}


def snapshot(label: str, received: dict, sugar: int) -> dict:
    actual = craft.view(received, ITEMS)
    return {'label': label, 'fence': fence(received, 100, sugar), 'actual': actual, 'saved': saved(actual, sugar),
            'drops': [], 'drop_totals': {}, 'live_count': {'predicate': 'if score CraftDrops pdprobe matches 0',
                'commands_log': 'SYNTHETIC/commands.jsonl'}, 'live_count_command': [], 'live_items': [], 'presence': [],
            'crafted_counts': {**ZERO, 'sugar': sugar}, 'wire_counts': {**ZERO, 'sugar': sugar},
            'time_before': 100, 'time_after': 100}


def synthetic_entry() -> dict:
    """Hand-authored pure inputs; synthetic honey ID1 and process identities."""
    honey, sugar, bottle = {'item_id': 1, 'count': 1}, {'item_id': 1217, 'count': 3}, {'item_id': 1270, 'count': 1}
    start = state('table', 7, main=[honey] + [None] * 35)
    prepared = state('table', 10, grid=honey, result=sugar)
    extracted = state('table', 12, grid=bottle, cursor=sugar)
    setup = phase('grid_setup', '120707000900020000', start, prepared, 10,
                  [('140708000901010000', 20), ('140709002500', 20), ('14070a000003c1090000', 20)], 'swap', 9)
    take = phase('extract', '12070a000001000000', prepared, extracted, 20,
                 [('14070b000000', 20), ('14070c000901f6090000', 20), ('6203c1090000', 98)], 'pickup', 0)
    take['request']['button'] = 1
    take['fence'] = fence(extracted, 20, 2)
    take['wire'][-1] = {**take['fence']['response'], 'direction': 'clientbound', 'pid': 1}
    fresh = state('table', 1)
    opened = {'kind': 'table_open', 'complete': True, 'after_sequence': 1,
              'request': {'sent_action': 'use_item_on', 'written': True, 'interaction_sequence': 1},
              'fence': fence(fresh, 3, 0),
              'packets': [{'kind': 'open', 'sequence': 2, 'menu_type': 12, 'container_id': 7},
                          {'kind': 'content', 'container_id': 7, 'slots': [None] * 46}],
              'wire': [envelope('4200', direction='serverbound', packet_id=66, pid=1),
                       envelope('3c070c', direction='clientbound', packet_id=60, sequence=2, pid=1)]}
    # The synthetic open envelope tests selection, not a runtime use-item codec.
    # Existing shared tests cover the real encoders; this is never capture gold.
    empty = state('player', 15)
    evacuated_main = [sugar, bottle] + [None] * 34
    snapshots = [snapshot('baseline', empty, 0), snapshot('prepared', prepared, 0),
                 snapshot('extracted', extracted, 2),
                 snapshot('evacuated', state('player', 14, main=evacuated_main), 2), snapshot('before_stop', empty, 2)]
    checks = [{'label': value['label'], 'actual': deepcopy(value['actual']), 'expected': deepcopy(value['actual']),
               'drops': {}, 'expected_drops': {}, 'before_stats': ZERO, 'after_stats': {**ZERO, 'sugar': number},
               'awards': {'sugar': number} if number else {}, 'validated': True}
              for value, number in zip(snapshots[1:3], (0, 2), strict=True)]
    case = {'name': bridge.CASE, 'result': 'passed', 'evidence_retained': True, 'placements': [(9, 0)],
            'phases': [opened, setup, take], 'checks': checks, 'snapshots': snapshots,
            'timeline': ['evidence_retained', 'assertions_passed', 'cleanup_started', 'cleanup_complete']}
    for action in case['phases']:
        for row in action.get('wire', []):
            row['pid'] = 2
    return {'role': 'SYNTHETIC_fresh', 'cases': {bridge.CASE: case}, 'saved': deepcopy(snapshots[-1]['saved']),
            'status': {'protocol': 777}, 'table_predicate': {'predicate': 'if block 4104 200 6 minecraft:crafting_table'},
            'world': {'level_data_version': craft.atom('Int', 5023), 'chunk_data_version': 5023, 'chunk': [256, 0],
                      'status': 'minecraft:full', 'world_time': 100,
                      'table': {'name': 'minecraft:crafting_table', 'properties': {}},
                      'floor': {'name': 'minecraft:stone', 'properties': {}}},
            'cleanup_errors': [], 'watchdogs': [],
            'clients': [{'name': craft.NAME, 'pid': 2, 'argv': ['SYNTHETIC-node', '--crafting', '--raw-packets']}],
            'teardown': {'boot': 'SYNTHETIC_fresh', 'pid': 1, 'argv': ['SYNTHETIC-server'],
                         'started': '2026-01-01T00:00:00Z', 'stopped': '2026-01-01T00:01:00Z',
                         'exit_code': 0, 'forced_stop': False,
                         'clients': [{'pid': 2, 'exit_code': 0, 'forced_close': False, 'failure': None,
                                      'expected_disconnect': None}]}}


def synthetic_receipts() -> tuple[dict, dict]:
    """Synthetic identities/bytes in the exact retained all-sixteen helper schema.

    Commands/counts model the r30 helper source, not a completed native pass.
    This is a hand-authored acceptance control, not a runtime golden or pass.
    """
    source = SYNTHETIC_SOURCE
    root, prefix = '/home/doc/code/Ferraloom', 'review30-corrected-engine-builder-synthetic'
    commands = [
        ('fmt-root', ['cargo', 'fmt', '--all', '--', '--check']),
        ('fmt-vendor', ['cargo', 'fmt', '--manifest-path', 'vendor/pumpkin/Cargo.toml', '--all', '--', '--check']),
        ('python', ['python3', '-B', '-m', 'unittest', 'discover', '-s', 'tests']),
        ('vendor-clippy', ['cargo', 'clippy', '--manifest-path', 'vendor/pumpkin/Cargo.toml', '--locked', '--offline', '--jobs', '1', '--keep-going', '--workspace', '--all-targets', '--target-dir', 'target', '--', '-D', 'warnings']),
        ('data', ['cargo', 'test', '--manifest-path', 'vendor/pumpkin/Cargo.toml', '--locked', '--offline', '--jobs', '1', '-p', 'pumpkin-data', '--lib', '--target-dir', 'target']),
        ('protocol', ['cargo', 'test', '--manifest-path', 'vendor/pumpkin/Cargo.toml', '--locked', '--offline', '--jobs', '1', '-p', 'pumpkin-protocol', '--lib', '--tests', '--target-dir', 'target']),
        ('command', ['cargo', 'test', '--locked', '--offline', '--jobs', '1', '-p', 'pumpkin-command', '--lib']),
        ('world', ['cargo', 'test', '--manifest-path', 'vendor/pumpkin/Cargo.toml', '--locked', '--offline', '--jobs', '1', '-p', 'pumpkin-world', '--lib', '--target-dir', 'target', '--', '--test-threads=2']),
        ('inventory', ['cargo', 'test', '--manifest-path', 'vendor/pumpkin/Cargo.toml', '--locked', '--offline', '--jobs', '1', '-p', 'pumpkin-inventory', '--lib', '--target-dir', 'target']),
        ('core', ['cargo', 'test', '--manifest-path', 'vendor/pumpkin/Cargo.toml', '--locked', '--offline', '--jobs', '1', '-p', 'pumpkin-core', '--lib', '--target-dir', 'target', '--', '--test-threads=2']),
        ('gametest-compile', ['cargo', 'check', '--manifest-path', 'vendor/pumpkin/Cargo.toml', '--locked', '--offline', '--jobs', '1', '-p', 'pumpkin-gametest', '--all-targets', '--target-dir', 'target']),
        ('clippy', ['cargo', 'clippy', '--locked', '--offline', '--jobs', '1', '--all-targets', '--', '-D', 'warnings']),
        ('wrapper', ['cargo', 'test', '--locked', '--offline', '--jobs', '1']),
        ('node-protocol', ['npm', 'run', 'test:protocol']),
        ('typescript', ['npm', 'run', 'check:protocol']),
        ('build', ['cargo', 'build', '--locked', '--offline', '--jobs', '1']),
    ]
    steps = [{'step': name, 'command': argv, 'exit_code': 0, 'elapsed_seconds': 1.0,
              'log': f'{root}/.cache/{prefix}-{name}.log', 'log_sha256': '1' * 64,
              'execution_gate_passed': True} for name, argv in commands]
    counts = {'data': [96], 'protocol': [129, 12], 'command': [92], 'world': [416],
              'inventory': [24], 'core': [866], 'wrapper': [12]}
    native = {'source_commit': source, 'host': 'ferraloom-builder', 'helper_sha256': bridge.NATIVE_HELPER_SHA256,
              'resource_limits': {'memory.max': '6442450944', 'memory.swap.max': '0', 'cpu.max': '200000 100000'},
              'cgroup': '/user.slice/user-1000.slice/user@1000.service/app.slice/ferraloom-build-20261017T040257Z-deadbeef.scope',
              'build_environment': {'CARGO_INCREMENTAL': '0', 'CARGO_BUILD_JOBS': '1'}, 'steps': steps,
              'execution_checks': {name: [{'binary': f'{root}/target/debug/deps/synthetic_{name}_{index}',
                                          'expected_passed': count} for index, count in enumerate(values)]
                                   for name, values in counts.items()},
              'scope': 'All seven vendored crates plus wrapper, Python, Node and TypeScript. GameTest compile only, zero standalone behavior tests. No parity promotion.',
              'elapsed_seconds': 16.0,
              'preserved_generated_test_artifact': {'path': f'{root}/.cache/{prefix}-generated-data/usercache.json',
                                                   'sha256': '2' * 64},
              'binary_sha256': SYNTHETIC_BINARY,
              'full_source_clean': True}
    build = {'host': 'ferraloom-builder', 'source_commit': source,
             'command': ['timeout', '--signal=TERM', '--kill-after=30s', '7200', 'python3', '-B',
                         bridge.NATIVE_HELPER, source, prefix],
             'cwd': root, 'resource_limits': {'memory': '6G', 'swap': '0', 'cpu': '200%'},
             'exit_code': 0, 'elapsed_seconds': 16.1, 'source_status_after': ''}
    return build, native


class ReaCraftingCaseTests(unittest.TestCase):
    def projection(self, value: dict) -> dict:
        return bridge.project(value, {'items': ITEMS, 'crafted_category': 1})

    def test_projection_keeps_whole_output_empty_next_result_and_actual_delta(self):
        value = synthetic_entry()
        projected = self.projection(value)
        observed = projected['checkpoints'][2]
        self.assertEqual(observed['view']['cursor'], craft.stack('sugar', 3))
        self.assertIsNone(observed['view']['result'])
        self.assertEqual(observed['crafted_delta']['sugar'], 2)
        self.assertEqual(observed['saved']['crafted']['sugar'], 2)
        self.assertEqual(projected['observed_crafted_values'], [{'item': 'sugar', 'absolute': 2}])
        self.assertEqual(projected['stopped']['crafted']['sugar'], 2)
        self.assertEqual(craft.payload(value['cases'][bridge.CASE]['snapshots'][2]['fence']['response']).hex(),
                         '030101c10902')
        self.assertEqual(projected['checkpoints'][1]['view']['result'], craft.stack('sugar', 3))
        self.assertEqual(projected['checkpoints'][3]['view']['main'][:2],
                         [craft.stack('sugar', 3), craft.stack('glass_bottle')])
        value['cases'][bridge.CASE]['snapshots'][2]['fence']['received']['cursor']['count'] = 1
        with self.assertRaises(AssertionError):
            self.projection(value)

    def test_wrong_missing_or_opaque_remainder_is_never_erased(self):
        for remainder in (None, {'item_id': 1270, 'count': 2},
                          {'item_id': 1270, 'count': 1, 'components': {'minecraft:custom_data': {'changed': 1}}}):
            value = synthetic_entry()
            value['cases'][bridge.CASE]['snapshots'][2]['fence']['received']['table']['slots'][9] = remainder
            with self.subTest(remainder=remainder), self.assertRaises(AssertionError):
                self.projection(value)

    def test_missing_wrong_stats_or_drop_observations_fail_closed(self):
        for defect in ('missing_stats', 'wrong_stats', 'missing_census', 'drop', 'missing_raw_award'):
            value = synthetic_entry()
            case = value['cases'][bridge.CASE]
            snap = case['snapshots'][2]
            if defect == 'missing_stats':
                del snap['wire_counts']
            elif defect == 'wrong_stats':
                snap['wire_counts']['sugar'] = 3
            elif defect == 'missing_census':
                del snap['live_count']
            elif defect == 'drop':
                snap['drops'] = [drop()]
            else:
                for row in case['snapshots']:
                    row['fence'] = fence(row['fence']['received'], 100, 0)
                take = case['phases'][-1]
                take['fence'] = fence(take['received'], 20, 0)
                take['wire'][-1] = {**take['fence']['response'], 'direction': 'clientbound', 'pid': 2}
            with self.subTest(defect=defect), self.assertRaises((AssertionError, KeyError)):
                self.projection(value)

    def test_missing_or_changed_stopped_continuation_cannot_match(self):
        for defect in ('missing_player', 'missing_world', 'missing_before_stop', 'changed_stats', 'changed_table'):
            value = synthetic_entry()
            if defect == 'missing_player':
                del value['saved']
            elif defect == 'missing_world':
                del value['world']
            elif defect == 'missing_before_stop':
                value['cases'][bridge.CASE]['snapshots'].pop()
            elif defect == 'changed_stats':
                value['saved']['statistics']['data']['stats']['minecraft:crafted']['minecraft:sugar'] = 6
            else:
                value['world']['table']['name'] = 'minecraft:air'
            with self.subTest(defect=defect), self.assertRaises((AssertionError, KeyError)):
                self.projection(value)

    def test_unclean_or_unowned_child_exit_is_not_a_semantic_success(self):
        for defect in ('server_forced', 'client_forced', 'client_failed', 'wrong_owner', 'missing_cleanup'):
            value = synthetic_entry()
            stop = value['teardown']
            if defect == 'server_forced':
                stop['forced_stop'] = True
            elif defect == 'client_forced':
                stop['clients'][0]['forced_close'] = True
            elif defect == 'client_failed':
                stop['clients'][0]['exit_code'] = 1
            elif defect == 'wrong_owner':
                stop['clients'][0]['pid'] = 3
            else:
                del value['cleanup_errors']
            with self.subTest(defect=defect), self.assertRaises(AssertionError):
                bridge.lifecycle_projection(value)

    def test_decoded_statistic_cannot_disagree_with_its_raw_packet(self):
        value = synthetic_entry()
        snap = value['cases'][bridge.CASE]['snapshots'][2]
        snap['fence']['response']['entries'][0]['value'] = 6
        with self.assertRaisesRegex(AssertionError, 'Decoded statistics'):
            self.projection(value)

    def associate(self, build: dict, native: dict, **overrides) -> None:
        bridge.validate_build_association(bridge.canonical(build), bridge.canonical(native), **({
            'build_name': '20261017T040257Z-deadbeef.json',
            'native_name': 'review30-corrected-engine-builder-synthetic-native.json',
            'source': SYNTHETIC_SOURCE,
            'binary_sha256': SYNTHETIC_BINARY,
        } | overrides))

    def source_paths(self, changed: list[str], statuses: list[str]) -> None:
        """Mock filesystem/Git inputs and stop after the real pin_inputs path checks."""
        args = SimpleNamespace(rust_source_commit=SYNTHETIC_SOURCE, current_clean_commit='3' * 40,
                               client_source_commit=SYNTHETIC_SOURCE,
                               java='SYNTHETIC-java', rust_binary=Path('/SYNTHETIC/binary'),
                               reference_cache=Path('/SYNTHETIC/reference'), artifacts=Path('/SYNTHETIC/artifacts'),
                               build_receipt=Path('/SYNTHETIC/build.json'), native_receipt=Path('/SYNTHETIC/native.json'))
        with (patch.object(bridge, 'git', side_effect=[
                  '\n'.join(changed), '\n'.join(statuses), RuntimeError('SYNTHETIC ancestry boundary')]),
              patch.object(craft.suite, 'git_state', return_value={'commit': args.current_clean_commit, 'dirty': False}),
              patch.object(craft, 'manifest', return_value={}),
              patch.object(craft, 'digest', return_value='0' * 64),
              patch.object(craft, 'read_bytes', return_value=b'{}'),
              patch.object(craft, 'write_json'), patch.object(bridge, 'retain'),
              patch.object(bridge.shutil, 'which', return_value='/SYNTHETIC/executable')):
            bridge.pin_inputs(args, {})

    def test_synthetic_source_boundaries_require_exact_observed_engine_client_bridge_chain(self):
        """Mocked Git metadata only, never runtime goldens or a completed native pass."""
        engine, client, harness = SYNTHETIC_SOURCE, SYNTHETIC_SOURCE, '3' * 40
        bridge_names = ('diff', '--no-renames', '--name-only', client, harness)
        bridge_status = ('diff', '--no-renames', '--name-status', client, harness)
        harness_parent = ('rev-list', '--parents', '-n', '1', harness)
        client_status = ('diff', '--no-renames', '--name-status', engine, client)
        full_status = ('diff', '--no-renames', '--name-status', engine, harness)
        paths = dict.fromkeys(bridge.OWNED, 'M')
        replies = {
            bridge_names: '\n'.join(sorted(bridge.OWNED)),
            bridge_status: '\n'.join('M\t' + path for path in sorted(bridge.OWNED)),
            harness_parent: harness + ' ' + client,
            ('merge-base', '--is-ancestor', engine, harness): '',
            client_status: '',
            full_status: '\n'.join(status + '\t' + path for path, status in sorted(paths.items())),
        }
        outside_queries = []
        for before, after, excluded in ((engine, client, ()),
                                         (client, harness, bridge.OWNED),
                                         (engine, harness, tuple(paths))):
            query = ('diff', '--no-renames', '--name-status', before, after, '--', '.',
                     *(':(exclude)' + path for path in excluded))
            replies[query] = ''
            outside_queries.append(query)
        with patch.object(bridge, 'git', side_effect=lambda *query: replies[query]):
            observed = bridge.source_boundaries(engine, client, harness)
        self.assertEqual(observed['engine_to_client']['to_commit'], client)
        self.assertEqual(observed['engine_to_client']['name_status'], [])
        self.assertEqual(observed['client_to_harness']['from_commit'], client)
        self.assertEqual(observed['engine_to_harness']['from_commit'], engine)
        self.assertEqual(observed['engine_to_harness']['name_status'], replies[full_status].splitlines())
        self.assertTrue(all(boundary['outside_declared_paths'] == [] for boundary in observed.values()))
        defects = [
            ('wrong_client_status', client_status, 'M\t' + bridge.CLIENT_PATHS[0]),
            ('wrong_bridge_status', bridge_status, replies[bridge_status].replace('M\t', 'A\t', 1)),
            ('extra_engine_file', client_status, replies[client_status] + '\nM\tvendor/pumpkin/src/main.rs'),
            ('missing_bridge_path', bridge_names, replies[bridge_names].splitlines()[0]),
            ('multicommit_harness', harness_parent, harness + ' ' + '4' * 40),
            ('unobserved_harness_commit', harness_parent, '5' * 40 + ' ' + engine),
            ('merge_parent', harness_parent, harness + ' ' + engine + ' ' + '4' * 40),
            ('wrong_combined_diff', full_status, replies[full_status] + '\nM\tCargo.toml'),
        ]
        defects += [('outside_boundary_' + str(index), query, 'M\tCargo.toml')
                    for index, query in enumerate(outside_queries)]
        for name, query, value in defects:
            altered = replies | {query: value}
            with (self.subTest(defect=name), self.assertRaises(AssertionError),
                  patch.object(bridge, 'git', side_effect=lambda *query: altered[query])):
                bridge.source_boundaries(engine, client, harness)
        for old in OLD_SOURCES:
            for sources in ((old, old, harness), (engine, old, harness),
                            (old, client, harness)):
                with (self.subTest(sources=sources), self.assertRaises(AssertionError),
                      patch.object(bridge, 'git', side_effect=lambda *query: replies[query])):
                    bridge.source_boundaries(*sources)
            with (self.subTest(old_harness=old), self.assertRaises(AssertionError),
                  patch.object(bridge, 'git', side_effect=[replies[bridge_names], replies[bridge_status],
                                                          old + ' ' + '4' * 40])):
                bridge.source_boundaries(engine, client, old)
            build, native = synthetic_receipts()
            build['source_commit'] = native['source_commit'] = old
            build['command'][-2] = old
            for source in (engine, old):
                with self.subTest(old_native_source=old, supplied_source=source), self.assertRaises(AssertionError):
                    self.associate(build, native, source=source)

    def test_completed_native_helper_receipt_shape_binds_supplied_binary(self):
        build, native = synthetic_receipts()
        self.assertIsNone(self.associate(build, native))
        self.assertEqual(native['steps'][-1]['command'], ['cargo', 'build', '--locked', '--offline', '--jobs', '1'])
        self.assertEqual(build['command'][4:7], ['python3', '-B', bridge.NATIVE_HELPER])
        self.assertEqual(native['host'], build['host'])
        self.assertEqual(native['helper_sha256'], bridge.NATIVE_HELPER_SHA256)
        with self.assertRaisesRegex(RuntimeError, 'SYNTHETIC ancestry boundary'):
            self.source_paths(['tests/test_rea_crafting_case.py', 'tools/rea_crafting_case.py'],
                              ['M\ttests/test_rea_crafting_case.py', 'M\ttools/rea_crafting_case.py'])

    def test_missing_native_or_old_binary_cannot_match_source(self):
        build, native = synthetic_receipts()
        with self.assertRaises(AssertionError):
            self.associate(build, {})
        with self.assertRaisesRegex(AssertionError, 'supplied binary'):
            self.associate(build, native, binary_sha256='0' * 64)
        for change in ({'source_commit': '0' * 40}, {'binary_sha256': '0' * 64},
                       {'full_source_clean': False}):
            with self.subTest(change=change), self.assertRaises(AssertionError):
                self.associate(build, native | change)

    def test_incomplete_or_unrelated_native_helper_job_is_rejected(self):
        defects = (
            ('missing_step', lambda b, n: n['steps'].pop()),
            ('reordered_steps', lambda b, n: n['steps'].reverse()),
            ('failed_gate', lambda b, n: n['steps'][2].update(execution_gate_passed=False)),
            ('missing_gate', lambda b, n: n['steps'][2].pop('execution_gate_passed')),
            ('failed_step', lambda b, n: n['steps'][2].update(exit_code=1)),
            ('boolean_exit', lambda b, n: n['steps'][2].update(exit_code=False)),
            ('check_instead_of_build', lambda b, n: n['steps'][-1].update(command=['cargo', 'check', '--locked', '--offline', '--jobs', '1'])),
            ('unrelated_wrapper_job', lambda b, n: b.update(command=['cargo', 'build', '--locked', '--offline', '--jobs', '1'])),
            ('wrong_prefix', lambda b, n: b['command'].__setitem__(-1, 'review30-corrected-engine-builder-other')),
            ('wrong_helper', lambda b, n: b['command'].__setitem__(6, '.cache/other.py')),
            ('old_helper', lambda b, n: b['command'].__setitem__(6, '.cache/reviews/r29-native-validation-prepare/run-review29-integrated-all-sixteen.py')),
            ('wrong_helper_hash', lambda b, n: n.update(helper_sha256='0' * 64)),
            ('old_helper_hash', lambda b, n: n.update(helper_sha256='769cddec92e87f39a133ad9e1c55fa65ebd97b8c204269adc5dc559f4b1b2c1c')),
            ('missing_helper_hash', lambda b, n: n.pop('helper_sha256')),
            ('wrong_native_host', lambda b, n: n.update(host='devbox')),
            ('missing_native_host', lambda b, n: n.pop('host')),
            ('wrong_source', lambda b, n: b.update(source_commit='0' * 40)),
            ('wrong_cwd', lambda b, n: b.update(cwd='/tmp/other')),
            ('wrong_host', lambda b, n: b.update(host='devbox')),
            ('failed_wrapper', lambda b, n: b.update(exit_code=1)),
            ('dirty_wrapper', lambda b, n: b.update(source_status_after=' M src/main.rs')),
            ('wrong_limits', lambda b, n: b['resource_limits'].update(memory='12G')),
            ('wrong_job', lambda b, n: n.update(cgroup=n['cgroup'].replace('deadbeef', 'feedbeef'))),
            ('wrong_log_attempt', lambda b, n: n['steps'][-1].update(log='/tmp/other-build.log')),
            ('wrong_environment', lambda b, n: n['build_environment'].update(CARGO_INCREMENTAL='1')),
        )
        for name, mutate in defects:
            build, native = synthetic_receipts()
            mutate(build, native)
            with self.subTest(defect=name), self.assertRaises(AssertionError):
                self.associate(build, native)
        for names in ({'build_name': 'renamed.json'},
                      {'native_name': 'review30-corrected-engine-builder-other-native.json'},
                      {'native_name': 'review29-all16-builder-synthetic-native.json'},
                      {'native_name': 'review28-all16-builder-synthetic-native.json'}):
            build, native = synthetic_receipts()
            with self.subTest(names=names), self.assertRaises(AssertionError):
                self.associate(build, native, **names)
        paths = ['tests/test_rea_crafting_case.py', 'tools/rea_crafting_case.py']
        modifications = ['M\t' + path for path in paths]
        for changed, statuses in (([], []), (paths[:1], modifications[:1]),
                                  (paths + ['src/main.rs'], modifications + ['M\tsrc/main.rs']),
                                  (paths, ['A\t' + path for path in paths]),
                                  (paths, ['A\t' + paths[0], 'M\t' + paths[1]])):
            with self.subTest(changed=changed, statuses=statuses), self.assertRaises(AssertionError):
                self.source_paths(changed, statuses)

    def test_buffered_reference_profile_appends_only_to_fresh_java_properties(self):
        initial = b'# Fresh reference\nserver-port=25565\nmotd=Fixture\n'
        configured = bridge.buffered_reference_configuration('java', initial)
        self.assertEqual(configured, initial + b'sync-chunk-writes=false\n')
        # Plain generated-key shapes from retained 26.3 server.properties,
        # SHA256 cd2dc94567e5294d2f542e1d580391a7b586b56ea6c52f4d2d4ce3f68086c7f0.
        # This subset is a parser control, never a gameplay/capture golden.
        rewritten = (b'#Minecraft server properties\n# Server rewrite\nquery.port=25565\n'
                     b'rcon.password=\nrcon.port=25575\nlevel-type=minecraft\\:normal\n'
                     b'sync-chunk-writes=false\nmotd=Fixture\n')
        self.assertEqual(bridge.buffered_reference_configuration('java', rewritten, started=True), rewritten)

    def test_buffered_reference_profile_leaves_rust_configuration_unchanged(self):
        configuration = b'seed = "12345678"\n[world.chunk]\ntype = "anvil"\n'
        self.assertEqual(bridge.buffered_reference_configuration('rust', configuration), configuration)
        self.assertEqual(bridge.buffered_reference_configuration('rust', configuration, started=True), configuration)

    def test_buffered_reference_profile_rejects_existing_ambiguous_or_changed_java_option(self):
        for raw in (b'sync-chunk-writes=false\n', b'sync-chunk-writes=true\n',
                    b'sync-chunk-writes=false\nsync-chunk-writes=false\n',
                    b'sync-chunk-writes:false\n', b'sync-chunk-writes false\n',
                    b'sync-chunk-\\u0077rites=false\n', b'motd=Fixture\\\nsync-chunk-writes=false\n',
                    b'motd=Fixture'):
            with self.subTest(initial=raw), self.assertRaises(AssertionError):
                bridge.buffered_reference_configuration('java', raw)
        for raw in (b'motd=Fixture\n', b'sync-chunk-writes=true\n',
                    b'sync-chunk-writes=false\nsync-chunk-writes=true\n',
                    b'sync-chunk-writes=false\nsync-chunk-writes=false\n',
                    b'sync-chunk-writes=false \n', b'sync-chunk-writes:false\n',
                    b'sync-chunk-\\u0077rites=false\n', b'motd=Fixture\\\nsync-chunk-writes=false\n'):
            with self.subTest(after_start=raw), self.assertRaises(AssertionError):
                bridge.buffered_reference_configuration('java', raw, started=True)


if __name__ == '__main__':
    unittest.main()
