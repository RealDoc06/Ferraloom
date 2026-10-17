"""Synthetic source-derived validator witnesses, never real acceptance receipts.

No server, client, JVM or process is started by these focused units.
"""
import base64
from copy import deepcopy
import hashlib
from pathlib import Path
import queue
import struct
import sys
import unittest
from unittest.mock import Mock, patch
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
import check_dragon_parts as dragon


def envelope(raw: bytes, **fields) -> dict:
    return {'payload_base64': base64.b64encode(raw).decode(),
            'payload_sha256': hashlib.sha256(raw).hexdigest(), **fields}


def typed(kind, value):
    if kind == 'Float':
        value = struct.unpack('>f', struct.pack('>f', value))[0]
    return {'nbt_type': kind, 'value': value, 'literal': str(value) + {'Float': 'f', 'Double': 'd', 'Int': ''}[kind]}


def field_read(identity, field, actual):
    command = f'data get entity {identity} {field}'
    result = {'uuid': identity, 'field': field, 'command': command, 'actual': deepcopy(actual),
              'output': ['Dragon has the following entity data: ' + actual['literal']],
              'typed_predicate': {'predicate': f'if data entity {identity} {{{field}:{actual["literal"]}}}'}}
    if field == 'Health':
        expected = int(actual['value'] * 1000000)
        result['scaled_read'] = {'command': command + ' 1000000', 'mode': 'result', 'expected': expected,
                                 'predicate': f'if score Observation pdprobe matches {expected}'}
    return result


def position(identity, expected=(0.0, 0.0, 0.0)):
    return {'uuid': identity, 'expected': list(expected), 'verified': True,
            'reads': [{'command': f'data get entity {identity} Pos[{index}]', 'field': f'Pos[{index}]',
                       'expected': value, 'actual': typed('Double', value),
                       'output': ['Dragon has the following entity data: ' + str(value) + 'd']}
                      for index, value in enumerate(expected)]}


def subject(label='default_hover', identifier=100, sequence=10):
    identity = dragon.supplied_identity(label)
    raw = (b'\x01' + dragon.protocol.varint(identifier) + uuid.UUID(identity['uuid']).bytes + b'\x2c'
           + struct.pack('>3d', 0, 0, 0) + b'opaque')
    spawn = envelope(raw, kind='spawn_header', packet_id=1, entity_id=identifier, uuid=identity['uuid'], entity_type=44,
        x=0, y=0, z=0, sequence=sequence, complete=False, decoded_bytes=len(raw) - 6, payload_bytes=len(raw),
        undecoded_remainder_base64=base64.b64encode(b'opaque').decode())
    phase = 10 if label == 'default_hover' else int(label[6:]) if label.startswith('phase_') else 0 if label.startswith('invalid_') else 9 if label == 'nbt_fields' else 10
    result = {'label': label, **identity, 'entity_id': identifier, 'spawn': spawn,
              'initial': {'Health': typed('Float', 0.01 if label == 'death_200' else 200), 'DragonPhase': typed('Int', phase),
                          'DragonDeathTime': typed('Int', 37 if label == 'nbt_fields' else 0),
                          'sitting_damage_received': typed('Float', 2.5 if label == 'nbt_fields' else 0)},
              'gametime_before': 100, 'gametime_after': 100, 'position': position(identity['uuid'])}
    result['identity_predicate'] = {'predicate': dragon.origin_predicate(result)}
    result['initial_reads'] = {field: field_read(identity['uuid'], field, actual) for field, actual in result['initial'].items()}
    return result


def resident(victim):
    return {'uuid': victim['uuid'], 't0': 100, 'gametime_before': 100, 'gametime_after': 100, 'frozen_after': True,
            'identity_predicate': {'predicate': dragon.origin_predicate(victim)}, 'position': position(victim['uuid'])}


def non_target(identity, actual):
    command = f'if data entity {identity} {{Health:{actual["literal"]}}}'
    return {'uuid': identity, 'nbt_type': 'Float', 'literal': actual['literal'], 'command': command,
            'predicate': {'predicate': command}}


def target_health(identity, actual):
    return {**actual, 'full_read': field_read(identity, 'Health', actual)}


def attack(*, negative=False, victim=None, health=None, observations=None, offset=1, lethal=False,
           after_health=199.8, counter=0, after_counter=0.1999969482421875):
    victim = subject() if victim is None else victim
    identity = victim['uuid']
    health = ({identity: victim['initial']['Health'], subject('phase_0')['uuid']: typed('Float', 200)}
              if health is None else health)
    target = -2147483000 if negative else victim['entity_id'] + offset
    raw = b'\x01' + dragon.protocol.varint(target)
    request = envelope(raw, entity_id=target, packet_id=1, written=True, after_sequence=100)
    response = envelope(b'\x03\x00', packet_id=3, sequence=200, payload_bytes=2, decoded_bytes=2)
    before = {key: target_health(key, actual) if key == identity else non_target(key, actual) for key, actual in health.items()}
    after = deepcopy(before)
    after[identity] = target_health(identity, typed('Float', health[identity]['value'] if negative else 0 if lethal else after_health))
    return {'subject': deepcopy(victim), 'target_id': target, 'request': request, 'allocation_assumption': 'synthetic fresh allocator premise',
        'fence': {'request': {'packet_id': 12, 'action_id': 1, 'written': True, 'after_sequence': 101},
                  'response': response, 'observations': deepcopy(observations if observations is not None else [victim['spawn']])},
        'wire': [envelope(raw, packet_id=1, direction='serverbound', pid=7),
                 envelope(b'\x0c\x01', packet_id=12, direction='serverbound', pid=7),
                 {**response, 'direction': 'clientbound', 'pid': 7}],
        't0': 100, 'gametime_before': 100, 'gametime_after': 100, 'frozen_after': True,
        'before': before, 'after': after, 'retained_health': deepcopy(health),
        'retained_sitting': typed('Float', counter),
        'sitting_before': field_read(identity, 'sitting_damage_received', typed('Float', counter)),
        'sitting_after': field_read(identity, 'sitting_damage_received', typed('Float',
            counter if negative else health[identity]['value'] if lethal else after_counter)),
        'inventory': {'slots': [None] * 46},
        'player': {'predicate': f'if entity @a[name={dragon.NAME},nbt={{playerGameType:0,Pos:[0.0d,0.0d,2.0d]}}]'},
        'player_position': position(str(dragon.PLAYER_ID), (0.0, 0.0, 2.0)),
        'healthy_connection': {'phase': 'play', 'login': True}}


def state_case(victim=None, role='java_frozen24'):
    victim = subject() if victim is None else victim
    return {'name': victim['label'], 'boot': role, 't0': 100, 'result': 'passed', 'subject': victim,
        'values': deepcopy(victim['initial']), 'observations': [victim['spawn']],
        'gametime_before': 100, 'gametime_after': 100, 'evidence_retained': True,
        'timeline': ['started', 'evidence_retained', 'assertions_passed', 'cleanup_started', 'cleanup_pending', 'cleanup_complete'],
        'cleanup': {'verified': True, 'status': 'complete', 'owned_boot': role, 'boundary': role, 'resident': resident(victim)}}


def death_case(victim=None, role='java_death1'):
    victim = subject('death_200') if victim is None else victim
    case = state_case(victim, role)
    lethal = attack(victim=victim, health={victim['uuid']: victim['initial']['Health']}, lethal=True)
    case.update(attacks=[lethal], prelethal=deepcopy(victim['initial']),
                before_step={'Health': typed('Float', 0), 'DragonDeathTime': typed('Int', 0)}, steps=[])
    elapsed, gametime = 0, 100
    for mark in (1, 20, 155, 199, 200):
        row = {'elapsed': mark, 'chunks': [], 'observations': [victim['spawn']]}
        while elapsed < mark:
            delta = min(100, mark - elapsed)
            row['chunks'].append({'before': gametime, 'after': gametime + delta, 'delta': delta, 'frozen_after': True})
            elapsed += delta
            gametime += delta
        if mark < 200:
            row['values'] = {'DragonDeathTime': typed('Int', mark), 'Health': typed('Float', 0)}
        else:
            row['absent'] = {'predicate': 'unless entity ' + victim['uuid']}
            row['natural_lookup_failure'] = {'success_zero': True, 'command': f'data get entity {victim["uuid"]} DragonDeathTime',
                                           'mode': 'success', 'expected': 0, 'predicate': 'if score Observation pdprobe matches 0'}
            raw = b'\x4e\x01' + dragon.protocol.varint(victim['entity_id'])
            row['observations'].append(envelope(raw, kind='remove', packet_id=78, sequence=300,
                entity_ids=[victim['entity_id']], complete=True, decoded_bytes=len(raw), payload_bytes=len(raw)))
        case['steps'].append(row)
    case['cleanup']['already_removed_by_actual_tick200'] = deepcopy(case['steps'][-1]['absent'])
    return case


def campaign():
    report = {'completed_boots': list(dragon.BOOTS), 'cleanup_complete': True, 'runtime_errors': [],
              'provenance_checked_before': True, 'provenance_checked_after': True, 'result': 'passed',
              'comparison': 'equal_bounded_contracts', 'boots': {}, 'processes': {'vanilla': [], 'rust': []}}
    for index, role in enumerate(dragon.BOOTS):
        subjects = [subject(name, 100 + offset * 10, 10 + offset) for offset, name in enumerate(dragon.boot_cases(role))]
        health = {victim['uuid']: deepcopy(victim['initial']['Health']) for victim in subjects}
        sitting = {victim['uuid']: deepcopy(victim['initial']['sitting_damage_received']) for victim in subjects}
        observations = [victim['spawn'] for victim in subjects]
        cases = {}
        for victim in subjects:
            name = victim['label']
            case = cases[name] = death_case(victim, role) if name == 'death_200' else state_case(victim, role)
            if name.startswith('part_') or name == 'unknown_then_positive':
                cohort = {key: value for key, value in health.items()
                          if name == 'unknown_then_positive' or key != dragon.supplied_identity('unknown_then_positive')['uuid']}
                case['attacks'] = []
                cohort_observations = [packet for packet in observations if packet['uuid'] in cohort]
                if name == 'unknown_then_positive':
                    case['attacks'].append(attack(victim=victim, health=cohort, observations=cohort_observations, negative=True))
                case['attacks'].append(attack(victim=victim, health=cohort, observations=cohort_observations,
                                             offset=dragon.PARTS.index(name[5:]) + 1 if name.startswith('part_') else 1))
                health[victim['uuid']] = {key: case['attacks'][-1]['after'][victim['uuid']][key] for key in ('nbt_type', 'value', 'literal')}
                sitting[victim['uuid']] = deepcopy(case['attacks'][-1]['sitting_after']['actual'])
        if role.endswith('_death1'):
            observations = deepcopy(cases['death_200']['steps'][-1]['observations'])
        engine = 'vanilla' if role.startswith('java_') else 'rust'
        pid = 1000 + index
        client = {'pid': 7, 'exit_code': 0, 'forced_close': False, 'failure': None}
        teardown = {'boot': role, 'pid': pid, 'exit_code': 0, 'forced_stop': False, 'unexpected_exit': False,
                    'started': f'2026-10-17T10:0{index}:00Z', 'stopped': f'2026-10-17T10:0{index}:30Z', 'clients': [client]}
        boot = report['boots'][role] = {'role': role, 'engine': engine, 'result': 'passed', 'directory': '/synthetic/' + role,
            'cases': cases, 'subjects': subjects, 't0': 100, 'clients': [{'pid': 7}], 'status': {'protocol': 777},
            'input_pins_checked_before_boot': True, 'teardown': teardown, 'world_absent_before_configure': True,
            'fresh_arena': {'predicate': 'unless entity @e[type=minecraft:ender_dragon]', 'roster': [], 'gametime': 100},
            'observations': observations, 'wire': [{**packet, 'pid': 7, 'direction': 'clientbound'} for packet in observations]}
        report['processes'][engine].append(deepcopy(teardown))
        if role.endswith('_frozen24'):
            boot['after_client_close_gametime'] = 100
            boot['final_sweep'] = {}
            for victim in subjects:
                facts = {**victim['initial'], 'Health': health[victim['uuid']], 'sitting_damage_received': sitting[victim['uuid']]}
                reads = {field: field_read(victim['uuid'], field, actual) for field, actual in facts.items()}
                boot['final_sweep'][victim['uuid']] = {**resident(victim), 'facts': facts,
                                                       'health_read': reads['Health'], 'reads_by_field': reads}
        else:
            boot['sole_subject'] = {'roster': [subjects[0]['uuid']],
                'predicate': 'unless entity @e[type=minecraft:ender_dragon,nbt=!{UUID:[I;324478056,610800471,0,25]}]'}
    return report


class DragonValidatorTests(unittest.TestCase):
    def test_authoritative_numeric_reads_reject_errors_wrong_types_and_ambiguity(self):
        self.assertEqual(dragon.parse_numeric(['Dragon has the following entity data: 200.0f'], 'Float')['value'], 200)
        self.assertEqual(dragon.parse_numeric(['Dragon has the following entity data: 37'], 'Int')['value'], 37)
        for lines, kind in [(['No entity was found'], 'Float'), (['Dragon has the following entity data: 200'], 'Float'),
                (['Dragon has the following entity data: 37f'], 'Int'), (['Dragon has the following entity data: NaNf'], 'Float'),
                (['Dragon has the following entity data: 10'] * 2, 'Int')]:
            with self.subTest(lines=lines), self.assertRaises(AssertionError):
                dragon.parse_numeric(lines, kind)
        for literal, value in [('0.0d', 0.0), ('-0.0D', -0.0), ('0.5d', 0.5),
                               ('5e-324d', 5e-324), ('1.7976931348623157e308d', 1.7976931348623157e308)]:
            self.assertEqual(dragon.parse_numeric(['Dragon has the following entity data: ' + literal], 'Double'),
                             {'nbt_type': 'Double', 'value': value, 'literal': literal})
        for literal in ('0', '0.0', '0.0f', '0b', '0s', '0L', 'NaNd', 'Infinityd', '-Infinityd', '1e309d'):
            with self.subTest(literal=literal), self.assertRaises(AssertionError):
                dragon.parse_numeric(['Dragon has the following entity data: ' + literal], 'Double')
        for lines in ([], ['No entity was found'], ['Dragon has the following entity data: 0.0d'] * 2):
            with self.subTest(lines=lines), self.assertRaises(AssertionError):
                dragon.parse_numeric(lines, 'Double')
        for kind in ('Float', 'Int'):
            with self.subTest(kind=kind), self.assertRaises(AssertionError):
                dragon.parse_numeric(['Dragon has the following entity data: 0.0d'], kind)

        probe = dragon.Probe(Mock(), Mock(), {'setup': {}, 'role': 'java_frozen24', 't0': 100})
        identity = subject()['uuid']
        with patch.object(probe, 'retain'):
            for expected in ((0.0, 0.0, 0.0), (8192.0, 200.0, 8192.0), (0.0, 0.0, 2.0)):
                probe.server.command.reset_mock()
                probe.server.command.side_effect = [['Dragon has the following entity data: ' + str(value) + 'd'] for value in expected]
                evidence = probe.position(identity, expected)
                self.assertTrue(evidence['verified'])
                self.assertEqual([call.args[0] for call in probe.server.command.call_args_list],
                                 [f'data get entity {identity} Pos[{index}]' for index in range(3)])
                self.assertEqual([read['actual']['value'] for read in evidence['reads']], list(expected))
                self.assertTrue(all(read['actual']['nbt_type'] == 'Double' for read in evidence['reads']))
            for expected, literals in [((0.0, 0.0, 0.0), ('0.5d', '0.0d', '0.5d')),
                    ((0.0, 0.0, 0.0), ('5e-324d', '0.0d', '0.0d')),
                    ((0.0, 0.0, 2.0), ('0.0d', '2.0d', '0.0d')),
                    ((0.0, 0.0, 2.0), ('0.0d', '0.0d', None)),
                    ((0.0, 0.0, 2.0), ('0.0d', '0.0d', '2.0f')),
                    ((0.0, 0.0, 2.0), ('0.0d', '0.0d', '2')),
                    ((0.0, 0.0, 2.0), ('0.0d', '0.0d', '1e309d')),
                    ((8192.0, 200.0, 8192.0), ('8192.5d', '200.0d', '8192.5d'))]:
                probe.server.command.side_effect = [([] if literal is None else ['Dragon has the following entity data: ' + literal])
                                                    for literal in literals]
                with self.subTest(expected=expected, literals=literals), self.assertRaises(AssertionError):
                    probe.position(identity, expected)
                self.assertNotIn('verified', probe.case['positions'][-1])
                self.assertIn('output', probe.case['positions'][-1]['reads'][-1])

    def test_all_eight_part_routes_require_fresh_committed_health_and_unchanged_other_victims(self):
        for offset in range(1, 9):
            evidence = attack()
            target = 100 + offset
            raw = b'\x01' + dragon.protocol.varint(target)
            evidence['target_id'] = evidence['request']['entity_id'] = target
            evidence['request'].update(envelope(raw))
            evidence['wire'][0].update(envelope(raw))
            dragon.validate_attack(evidence, offset=offset)
            for defect in ('callback_only', 'other_victim', 'not_fresh', 'not_hover', 'wrong_part', 'no_fence', 'different_client', 'ticks'):
                broken = deepcopy(evidence)
                if defect == 'callback_only':
                    broken['after'][broken['subject']['uuid']]['value'] = 200
                elif defect == 'other_victim':
                    broken['after'][subject('phase_0')['uuid']]['literal'] = '199.0f'
                elif defect == 'not_fresh':
                    broken['subject']['initial']['Health']['value'] = 199
                elif defect == 'not_hover':
                    broken['subject']['initial']['DragonPhase']['value'] = 0
                elif defect == 'wrong_part':
                    broken['target_id'] += 1
                elif defect == 'no_fence':
                    broken['wire'].pop()
                elif defect == 'different_client':
                    broken['wire'][-1]['pid'] = 8
                else:
                    broken['gametime_after'] += 1
                with self.subTest(offset=offset, defect=defect), self.assertRaises(AssertionError):
                    dragon.validate_attack(broken, offset=offset)

        probe = dragon.Probe(Mock(), Mock(), {'setup': {}, 'role': 'java_frozen24', 't0': 100})
        victim = subject()
        probe.subjects = [victim]
        probe.health = {victim['uuid']: victim['initial']['Health']}
        probe.server.command.side_effect = [['Player has the following entity data: ' + value] for value in ('0.0d', '0.0d', '2.5d')]
        with patch.object(probe, 'retain'), patch.object(probe, 'health_map', return_value={}), \
                patch.object(dragon, 'require_frozen'), patch.object(dragon.suite, 'game_time', return_value=100), \
                patch.object(dragon, 'typed_predicate') as player:
            with self.assertRaisesRegex(AssertionError, 'wrong_exact_position'):
                probe.attack(victim, 101, offset=1)
            player.assert_called_once_with(probe.server,
                f'if entity @a[name={dragon.NAME},nbt={{playerGameType:0,Pos:[0.0d,0.0d,2.0d]}}]')
        probe.client.request.assert_not_called()
        self.assertEqual([call.args[0] for call in probe.server.command.call_args_list],
                         [f'data get entity {dragon.PLAYER_ID} Pos[{index}]' for index in range(3)])

    def test_unknown_signed_id_requires_unchanged_health_and_a_following_real_positive(self):
        negative = attack(negative=True)
        dragon.validate_attack(negative, offset=None)
        broken = deepcopy(negative)
        broken['after'][broken['subject']['uuid']]['value'] = 199
        with self.assertRaises(AssertionError):
            dragon.validate_attack(broken, offset=None)
        case = state_case()
        case.update(name='unknown_then_positive', attacks=[negative])
        with self.assertRaises(AssertionError):
            dragon.validate_evidence(case)
        case['attacks'].append(attack())
        dragon.validate_evidence(case)

    def test_source_derived_hover_counter_accepts_committed_one_and_one_quarter(self):
        # Synthetic source-derived controls, not B/D or E runtime acceptance.
        # Java phase10 isSitting; parent bci134 fadd then bci139 fsub.
        for remaining, counter in ((199, 1), (198.75, 1.25)):
            evidence = attack(after_health=remaining, after_counter=counter)
            dragon.validate_attack(evidence, offset=1)
            self.assertEqual(evidence['sitting_after']['actual'], typed('Float', counter))
            literal = str(counter) + 'f'
            actual = {**typed('Float', counter), 'literal': literal}
            evidence['sitting_after'] = field_read(evidence['subject']['uuid'], 'sitting_damage_received', actual)
            dragon.validate_attack(evidence, offset=1)
            self.assertEqual(evidence['sitting_after']['actual']['literal'], literal)
        victim = subject()
        evidence = attack(victim=victim, health={victim['uuid']: typed('Float', 199)}, counter=1,
                          after_health=198.75, after_counter=1.25)
        dragon.validate_attack(evidence, offset=1)
        self.assertEqual(evidence['retained_sitting'], typed('Float', 1))
        self.assertEqual(evidence['sitting_after']['actual'], typed('Float', 1.25))

    def test_source_derived_counter_arithmetic_rounds_fadd_before_fsub(self):
        # Isolated arithmetic control, not a claimed reachable campaign baseline.
        # f32(0.1) + 200 rounds to 200.100006103515625 before subtracting 199.
        actual = dragon.sitting_counter_after(0.10000000149011612, 200.0, 199.0)
        self.assertEqual(actual, 1.100006103515625)
        self.assertNotEqual(actual, 1.100000023841858)  # f32(counter + committed_loss) groups the operations wrongly.

    def test_sitting_counter_requires_committed_loss_and_raw_typed_target_witnesses(self):
        good = attack(after_health=199, after_counter=1)
        for defect in ('wrong', 'reset', 'uncommitted', 'negative', 'threshold_reset', 'wrong_type', 'bool_value',
                       'int_value', 'string_value', 'nan', 'literal', 'missing_before', 'missing_after', 'missing_output',
                       'ambiguous_output', 'raw_type', 'overflow', 'uuid', 'field', 'command', 'predicate', 'retained'):
            broken = deepcopy(good)
            read = broken['sitting_after']
            if defect in ('wrong', 'reset', 'negative'):
                value = {'wrong': 1.25, 'reset': 0, 'negative': -1}[defect]
                broken['sitting_after'] = field_read(broken['subject']['uuid'], 'sitting_damage_received', typed('Float', value))
            elif defect == 'uncommitted':
                broken['after'] = deepcopy(broken['before'])
            elif defect == 'threshold_reset':
                broken = attack(after_health=149, after_counter=0)
            elif defect == 'wrong_type':
                read['actual'] = typed('Double', 1)
            elif defect in ('bool_value', 'int_value', 'string_value', 'nan'):
                read['actual']['value'] = {'bool_value': True, 'int_value': 1, 'string_value': '1.0', 'nan': float('nan')}[defect]
            elif defect == 'literal':
                read['actual']['literal'] = '1.25f'
            elif defect in ('missing_before', 'missing_after'):
                broken.pop('sitting_before' if defect == 'missing_before' else 'sitting_after')
            elif defect == 'missing_output':
                read.pop('output')
            elif defect == 'ambiguous_output':
                read['output'] *= 2
            elif defect in ('raw_type', 'overflow'):
                read['output'] = ['Dragon has the following entity data: ' + ('1.0d' if defect == 'raw_type' else '1e40f')]
            elif defect == 'uuid':
                read['uuid'] = subject('phase_0')['uuid']
            elif defect == 'field':
                read['field'] = 'Health'
            elif defect == 'command':
                read['command'] = f'data get entity {subject("phase_0")["uuid"]} sitting_damage_received'
            elif defect == 'predicate':
                read['typed_predicate']['predicate'] = f'if data entity {broken["subject"]["uuid"]} {{sitting_damage_received:0.0f}}'
            else:
                broken['retained_sitting'] = typed('Float', 1)
            with self.subTest(defect=defect), self.assertRaises(AssertionError):
                dragon.validate_attack(broken, offset=1)

    def test_unknown_id_counter_and_boot_counter_lineage_cannot_be_rebased(self):
        for actual in (typed('Float', 1), {'nbt_type': 'Float', 'value': 0.0, 'literal': '0f'}):
            negative = attack(negative=True)
            identity = negative['subject']['uuid']
            negative['sitting_after'] = field_read(identity, 'sitting_damage_received', actual)
            with self.subTest(actual=actual), self.assertRaisesRegex(AssertionError, 'Unknown-ID attack changed sitting counter'):
                dragon.validate_attack(negative, offset=None)
        report = campaign()
        boot = report['boots']['java_frozen24']
        hit = boot['cases']['part_neck']['attacks'][0]
        identity = hit['subject']['uuid']
        hit['retained_sitting'] = typed('Float', 1)
        hit['sitting_before'] = field_read(identity, 'sitting_damage_received', typed('Float', 1))
        hit['sitting_after'] = field_read(identity, 'sitting_damage_received', typed('Float', 1.1999969482421875))
        dragon.validate_attack(hit, offset=2)  # Arithmetic alone cannot establish the retained baseline.
        with self.assertRaisesRegex(AssertionError, 'last full-read literal'):
            dragon.validate_boot(boot, 'java_frozen24')
        self.assertTrue(all(row['result'] == 'failed' for row in dragon.campaign_rows(report)))

    def test_final_sweep_requires_last_verified_counter_literal_and_untouched_state(self):
        for defect in ('reset', 'wrong_last', 'same_value_new_literal', 'untouched_state'):
            boot = campaign()['boots']['java_frozen24']
            victim = boot['cases']['default_hover' if defect == 'untouched_state' else 'part_head']['subject']
            sweep = boot['final_sweep'][victim['uuid']]
            actual = typed('Float', 0 if defect == 'reset' else 1)
            if defect == 'same_value_new_literal':
                actual = {**sweep['facts']['sitting_damage_received'], 'literal': '0.1999969482421875F'}
            sweep['facts']['sitting_damage_received'] = actual
            sweep['reads_by_field']['sitting_damage_received'] = field_read(victim['uuid'], 'sitting_damage_received', actual)
            with self.subTest(defect=defect), self.assertRaisesRegex(AssertionError, 'Final facts differ'):
                dragon.validate_boot(boot, 'java_frozen24')

        victim = subject('part_head')
        probe = dragon.Probe(Mock(), Mock(), {'setup': {}, 'role': 'java_frozen24', 't0': 100})
        probe.subjects = [victim]
        probe.health = {victim['uuid']: typed('Float', 199)}
        probe.sitting = {victim['uuid']: typed('Float', 1)}
        facts = {**victim['initial'], 'Health': typed('Float', 199), 'sitting_damage_received': typed('Float', 1)}
        probe.last_reads = {(victim['uuid'], field): field_read(victim['uuid'], field, actual) for field, actual in facts.items()}
        with patch.object(probe, 'retain'), patch.object(probe, 'resident', return_value=resident(victim)), \
                patch.object(probe, 'facts', return_value=facts), patch.object(probe, 'frozen_time', return_value=100), \
                patch.object(probe, 'observations', return_value=[victim['spawn']]), patch.object(dragon, 'interaction_deadline'):
            probe.final_sweep()
            self.assertEqual(probe.entry['final_sweep'][victim['uuid']]['facts']['sitting_damage_received'], typed('Float', 1))
            facts['sitting_damage_received'] = typed('Float', 0)
            with self.assertRaisesRegex(AssertionError, 'Final fact mismatch'):
                probe.final_sweep()
            self.assertEqual(probe.sitting[victim['uuid']], typed('Float', 1))

    def test_probe_reads_counters_around_actual_attack_and_commits_only_verified_literals(self):
        for actual_counter in (1, 0):
            witness = attack(after_health=199, after_counter=actual_counter)
            victim, identity = witness['subject'], witness['subject']['uuid']
            probe = dragon.Probe(Mock(), Mock(), {'setup': {}, 'role': 'java_frozen24', 't0': 100})
            probe.subjects = [victim]
            probe.health = {key: deepcopy(value) for key, value in witness['retained_health'].items()}
            probe.sitting = {identity: typed('Float', 0)}
            probe.client.events = []
            probe.client.state.return_value = {'player_inventory': {'slots': [None] * 46}, 'phase': 'play', 'login': True, 'packets': 201}
            order = []
            def read_counter(command):
                order.append('counter')
                value = '0.0f' if order.count('counter') == 1 else str(actual_counter) + '.0f'
                return ['Dragon has the following entity data: ' + value]
            def send_attack(*args, **kwargs):
                order.append('attack')
                probe.client.events.extend({**packet, 'event': 'wire_packet'} for packet in witness['wire'])
                return witness['request']
            def fence():
                order.append('fence')
                return witness['fence']
            probe.server.command.side_effect = read_counter
            probe.client.request.side_effect = send_attack
            probe.client.dragon_fence.side_effect = fence
            with patch.object(probe, 'retain'), patch.object(probe, 'frozen_time', return_value=100), \
                    patch.object(probe, 'health_map', side_effect=[witness['before'], witness['after']]), \
                    patch.object(probe, 'position', return_value=position(str(dragon.PLAYER_ID), (0.0, 0.0, 2.0))), \
                    patch.object(dragon, 'typed_predicate', side_effect=lambda server, text: {'predicate': text}):
                if actual_counter == 1:
                    evidence = probe.attack(victim, 101, offset=1)
                    self.assertEqual(probe.sitting[identity], typed('Float', 1))
                    self.assertEqual(probe.health[identity], typed('Float', 199))
                    self.assertEqual(evidence['sitting_after']['output'], ['Dragon has the following entity data: 1.0f'])
                else:
                    with self.assertRaisesRegex(AssertionError, 'committed health loss'):
                        probe.attack(victim, 101, offset=1)
                    self.assertEqual(probe.sitting[identity], typed('Float', 0))
                    self.assertEqual(probe.health[identity], typed('Float', 200))
                    self.assertEqual(probe.case['attacks'][0]['sitting_after']['actual'], typed('Float', 0))
            self.assertEqual(order, ['counter', 'attack', 'fence', 'counter'])
            self.assertEqual([call.args[0] for call in probe.server.command.call_args_list],
                             [f'data get entity {identity} sitting_damage_received'] * 2)

    def test_header_only_identity_and_complete_removals_reject_part_entities_or_changed_raw(self):
        victim = subject()
        dragon.validate_parent(victim, [victim['spawn']])
        remove = envelope(b'\x4e\x01\x64', packet_id=78, kind='remove', entity_ids=[100], sequence=30,
                          complete=True, decoded_bytes=3, payload_bytes=3)
        dragon.validate_parent(victim, [victim['spawn'], remove], removed=True)
        for defect in ('part_remove', 'duplicate_parent', 'full_claim', 'lost_tail', 'hash', 'truncated_remove'):
            packets = deepcopy([victim['spawn'], remove])
            if defect == 'part_remove':
                packets[1].update(envelope(b'\x4e\x02\x64\x65'), entity_ids=[100, 101], decoded_bytes=4, payload_bytes=4)
            elif defect == 'duplicate_parent':
                packets.append({**deepcopy(remove), 'sequence': 31})
            elif defect == 'full_claim':
                packets[0]['complete'] = True
            elif defect == 'lost_tail':
                packets[0]['undecoded_remainder_base64'] = ''
            elif defect == 'hash':
                packets[0]['payload_sha256'] = '0' * 64
            else:
                packets[1].update(envelope(b'\x4e\x02\x64'))
            with self.subTest(defect=defect), self.assertRaises(AssertionError):
                dragon.validate_parent(victim, packets, removed=True)

        # Retained first Java attempt: valid bounded header, wrong literal-origin position.
        raw = base64.b64decode('AQYTVyRoJGgTVwAAAAAAAAABLD/gAAAAAAAAAAAAAAAAAAA/4AAAAAAAAAAAAAAA')
        self.assertEqual(hashlib.sha256(raw).hexdigest(), 'dc959c5f39a2416e4eadd58aca2300ab4b50983911e539074ffc937dc8a33769')
        self.assertEqual(struct.unpack('>3d', raw[19:43]), (0.5, 0.0, 0.5))
        half_spawn = envelope(raw, kind='spawn_header', packet_id=1, entity_id=6, uuid=victim['uuid'], entity_type=44,
            x=0.5, y=0.0, z=0.5, sequence=142, complete=False, decoded_bytes=43, payload_bytes=len(raw),
            undecoded_remainder_base64=base64.b64encode(raw[43:]).decode())
        dragon.validate_packets([half_spawn])
        with self.assertRaisesRegex(AssertionError, 'origin parent spawn'):
            dragon.validate_parent({'uuid': victim['uuid'], 'entity_id': 6, 'spawn': half_spawn}, [half_spawn])

        probe = dragon.Probe(Mock(), Mock(), {'setup': {}, 'role': 'java_frozen24', 't0': 100})
        probe.client.state.return_value = {'packets': 9}
        probe.client.wait_state.return_value = {'dragon_packets': [victim['spawn']]}
        probe.server.command.side_effect = [['Summoned new Ender Dragon'],
            *[['Dragon has the following entity data: 0.0d'] for _ in range(3)]]
        probe.last_reads = {(victim['uuid'], field): read for field, read in victim['initial_reads'].items()}
        with patch.object(probe, 'retain'), patch.object(probe, 'facts', return_value=victim['initial']), \
                patch.object(dragon, 'typed_predicate', side_effect=lambda server, text: {'predicate': text}) as predicate, \
                patch.object(probe, 'frozen_time', return_value=100):
            summoned = probe.summon('default_hover', phase=None)
            self.assertTrue(summoned['command'].startswith('summon minecraft:ender_dragon 0.0 0.0 0.0 {'))
            self.assertEqual(probe.server.command.call_args_list[0].args, (summoned['command'],))
            predicate.assert_called_once_with(probe.server,
                f'if data entity {victim["uuid"]} {{UUID:[I;324478056,610800471,0,1],Pos:[0.0d,0.0d,0.0d],NoAI:1b}}')
            self.assertTrue(summoned['position']['verified'])
            self.assertEqual([read['actual']['value'] for read in summoned['position']['reads']], [0.0, 0.0, 0.0])
            probe.server.command.reset_mock()
            probe.server.command.side_effect = [['Dragon has the following entity data: 0.0d'] for _ in range(3)]
            probe.case = {}
            probe.resident_cleanup(summoned)
            cleanup = probe.case['cleanup']
            self.assertEqual(cleanup['status'], 'pending')
            self.assertFalse(cleanup['verified'])
            self.assertEqual(cleanup['resident']['identity_predicate']['predicate'], dragon.origin_predicate(summoned))
            self.assertEqual([read['actual']['value'] for read in cleanup['resident']['position']['reads']], [0.0, 0.0, 0.0])
            self.assertEqual([call.args[0] for call in probe.server.command.call_args_list],
                             [f'data get entity {victim["uuid"]} Pos[{index}]' for index in range(3)])

    def test_missing_stages_failed_expectations_and_cleanup_cannot_be_promoted(self):
        case = state_case()
        dragon.validate_case(case)
        for defect in ('stage', 'failed', 'failure_then_cleanup', 'not_retained', 'cleanup'):
            broken = deepcopy(case)
            if defect == 'stage':
                broken['timeline'].remove('evidence_retained')
            elif defect == 'failed':
                broken['result'] = 'failed'
            elif defect == 'failure_then_cleanup':
                broken['timeline'].insert(2, 'failure')
            elif defect == 'not_retained':
                broken['evidence_retained'] = False
            else:
                broken['cleanup']['verified'] = False
            with self.subTest(defect=defect), self.assertRaises(AssertionError):
                dragon.validate_case(broken)

    def test_death_requires_all_milestones_and_cannot_use_a_data_health_zero_fixture(self):
        case = death_case()
        dragon.validate_evidence(case)
        case['steps'] = []
        with self.assertRaisesRegex(AssertionError, 'milestones'):
            dragon.validate_evidence(case)
        case['prelethal']['Health']['value'] = 0
        with self.assertRaisesRegex(AssertionError, 'positive-health'):
            dragon.validate_evidence(case)

    def test_death_rejects_generic_twenty_tick_removal_double_counter_and_oversized_steps(self):
        for defect in ('generic20', 'double_counter', 'timer199', 'oversized_step', 'no_terminal_remove'):
            case = death_case()
            if defect == 'generic20':
                case['steps'][1]['observations'].append(deepcopy(case['steps'][-1]['observations'][-1]))
            elif defect == 'double_counter':
                case['steps'][2]['values']['DragonDeathTime']['value'] = 310
            elif defect == 'timer199':
                case['before_step']['DragonDeathTime']['value'] = 199
            elif defect == 'oversized_step':
                case['steps'][2]['chunks'] = [{'before': 120, 'after': 255, 'delta': 135, 'frozen_after': True}]
            else:
                case['steps'][-1]['observations'].pop()
            with self.subTest(defect=defect), self.assertRaises(AssertionError):
                dragon.validate_evidence(case)

    def test_engine_equivalence_allows_reporting_but_refuses_every_unlisted_build_input(self):
        for path in ('README.md', 'docs/report.md', 'compatibility/results/dragon.json', *dragon.OWNED):
            self.assertTrue(dragon.harness_path(path))
        for path in ('src/main.rs', 'vendor/pumpkin/src/a.rs', 'Cargo.toml', 'Cargo.lock', '.cargo/config.toml',
                     'build.rs', 'rust-toolchain.toml', 'compatibility/target.json', 'assets/new.dat', 'tools/build.py'):
            self.assertFalse(dragon.harness_path(path))
        engine, harness = 'a' * 40, 'b' * 40
        def git(*args):
            if args[0] == 'rev-parse':
                return args[-1][:-9] + '\n'
            return 'README.md\0tools/parity/client.ts\0' if '--name-only' in args else ''
        with patch.object(dragon, 'git', side_effect=git):
            provenance = dragon.engine_equivalence(engine, harness)
            self.assertEqual(provenance['build_label'], engine)
            self.assertEqual(provenance['production_diff'], '')
        with patch.object(dragon, 'git', side_effect=[engine + '\n', harness + '\n', 'vendor/changed.rs\0']):
            with self.assertRaises(AssertionError):
                dragon.engine_equivalence(engine, harness)

    def test_exact_four_boot_reports_aggregate_25_composites_and_50_actual_rows(self):
        report = campaign()
        dragon.validate_campaign(report)
        self.assertEqual(sum(len(boot['cases']) for boot in report['boots'].values()), 50)
        self.assertEqual([len(report['boots'][role]['cases']) for role in dragon.BOOTS], [24, 1, 24, 1])
        self.assertEqual(dragon.comparable(report['boots'], 'java'), dragon.comparable(report['boots'], 'rust'))
        rows = dragon.campaign_rows(report)
        self.assertEqual(len(rows), 25)
        self.assertTrue(all(row['result'] == 'passed' and len(row['boots']) == 2 for row in rows))
        self.assertEqual(set(rows[-1]['boots']), {'java_death1', 'rust_death1'})
        self.assertEqual(set(rows[0]['boots']), {'java_frozen24', 'rust_frozen24'})
        # Numeric IDs restart in the fresh death process; supplied UUIDs do not.
        self.assertEqual(report['boots']['java_frozen24']['subjects'][0]['entity_id'],
                         report['boots']['java_death1']['subjects'][0]['entity_id'])

    def test_partial_misowned_duplicated_or_out_of_order_four_boot_reports_stay_failed(self):
        for defect in ('missing_boot', 'order', 'missing_case', 'extra_case', 'misowned', 'duplicate_subject', 'stale_two_boot',
                       'frozen_parent_removed', 'death_boot_not_removed', 'state_attacks'):
            report = campaign()
            frozen = report['boots']['java_frozen24']
            if defect == 'missing_boot':
                report['boots'].pop('rust_death1')
            elif defect == 'order':
                report['completed_boots'][0:2] = reversed(report['completed_boots'][0:2])
            elif defect == 'missing_case':
                frozen['cases'].pop('phase_0')
            elif defect == 'extra_case':
                frozen['cases']['death_200'] = deepcopy(report['boots']['java_death1']['cases']['death_200'])
            elif defect == 'misowned':
                frozen['cases']['default_hover']['boot'] = 'java_death1'
            elif defect == 'duplicate_subject':
                frozen['subjects'][1] = deepcopy(frozen['subjects'][0])
            elif defect == 'frozen_parent_removed':
                raw = b'\x4e\x01' + dragon.protocol.varint(frozen['subjects'][0]['entity_id'])
                removal = envelope(raw, kind='remove', packet_id=78, sequence=300,
                    entity_ids=[frozen['subjects'][0]['entity_id']], complete=True, decoded_bytes=len(raw), payload_bytes=len(raw))
                frozen['observations'].append(removal)
                frozen['wire'].append({**removal, 'pid': frozen['clients'][0]['pid'], 'direction': 'clientbound'})
            elif defect == 'death_boot_not_removed':
                death = report['boots']['java_death1']
                death['observations'] = [packet for packet in death['observations'] if packet['kind'] != 'remove']
                death['wire'] = [packet for packet in death['wire'] if packet['packet_id'] != 78]
            elif defect == 'state_attacks':
                frozen['cases']['default_hover']['attacks'] = []
            else:
                report['completed_boots'] = ['java_fresh', 'rust_fresh']
            with self.subTest(defect=defect), self.assertRaises(AssertionError) as rejected:
                dragon.validate_campaign(report)
            if defect in ('frozen_parent_removed', 'death_boot_not_removed'):
                self.assertEqual(str(rejected.exception), 'Wrong parent removal count')
            elif defect == 'state_attacks':
                self.assertEqual(str(rejected.exception), 'Wrong attack evidence presence for case')
            self.assertTrue(all(row['result'] == 'failed' for row in dragon.campaign_rows(report)))

    def test_each_boot_requires_clean_owned_nonoverlapping_process_boundaries(self):
        for role in dragon.BOOTS:
            for defect in ('forced_stop', 'forced_close', 'unexpected_exit', 'server_exit', 'client_exit', 'client_failure', 'two_clients',
                           'wrong_owner', 'wrong_pid', 'reused_world', 'overlap', 'unregistered_process'):
                report = campaign()
                boot = report['boots'][role]
                teardown = boot['teardown']
                if defect == 'forced_stop':
                    teardown['forced_stop'] = True
                elif defect == 'forced_close':
                    teardown['clients'][0]['forced_close'] = True
                elif defect == 'unexpected_exit':
                    teardown['unexpected_exit'] = True
                elif defect == 'server_exit':
                    teardown['exit_code'] = 1
                elif defect == 'client_exit':
                    teardown['clients'][0]['exit_code'] = 1
                elif defect == 'client_failure':
                    teardown['clients'][0]['failure'] = 'synthetic failure'
                elif defect == 'two_clients':
                    boot['clients'].append({'pid': 8})
                elif defect == 'wrong_owner':
                    teardown['boot'] = 'unowned'
                elif defect == 'wrong_pid':
                    teardown['clients'][0]['pid'] = 8
                elif defect == 'reused_world':
                    boot['directory'] = report['boots'][dragon.BOOTS[(dragon.BOOTS.index(role) + 1) % 4]]['directory']
                elif defect == 'overlap':
                    teardown['started'] = '2026-10-17T09:00:00Z'
                    teardown['stopped'] = '2026-10-17T11:00:00Z'
                    report['processes'][boot['engine']][0 if role.endswith('_frozen24') else 1] = deepcopy(teardown)
                else:
                    report['processes'][boot['engine']].pop()
                with self.subTest(role=role, defect=defect), self.assertRaises(AssertionError):
                    dragon.validate_campaign(report)

    def test_pending_cleanup_needs_final_sweep_and_actual_clean_stop_before_promotion(self):
        for role in dragon.BOOTS:
            boot = campaign()['boots'][role]
            for case in boot['cases'].values():
                case['result'] = 'pending'
                case['timeline'].pop()
                case['cleanup'].update(status='pending', verified=False)
                case['cleanup'].pop('boundary')
            boot['result'] = 'failed'
            pending = deepcopy(boot)
            for defect in ('no_stop', 'forced_stop', 'wrong_owner', 'failed_row', 'early_cleanup'):
                broken = deepcopy(pending)
                if defect == 'no_stop':
                    broken['teardown']['exit_code'] = None
                elif defect == 'forced_stop':
                    broken['teardown']['forced_stop'] = True
                elif defect == 'wrong_owner':
                    broken['teardown']['boot'] = 'unowned'
                elif defect == 'failed_row':
                    next(iter(broken['cases'].values()))['result'] = 'failed'
                else:
                    next(iter(broken['cases'].values()))['cleanup']['status'] = 'complete'
                with self.subTest(role=role, defect=defect), self.assertRaises(AssertionError):
                    dragon.finalize_cases(broken, role)
                self.assertNotEqual(broken['result'], 'passed')
            dragon.finalize_cases(boot, role)
            self.assertTrue(all(case['result'] == 'passed' and case['cleanup']['verified'] for case in boot['cases'].values()))

    def test_death_uses_stable_ordinal_25_and_refuses_old_world_subjects(self):
        probe = dragon.Probe(Mock(), Mock(), {'setup': {}, 'role': 'java_death1', 't0': 100})
        victim = subject('death_200')
        probe.client.state.return_value = {'packets': 9}
        probe.client.wait_state.return_value = {'dragon_packets': [victim['spawn']]}
        probe.server.command.side_effect = [['Summoned new Ender Dragon'],
            *[['Dragon has the following entity data: 0.0d'] for _ in range(3)]]
        probe.last_reads = {(victim['uuid'], field): read for field, read in victim['initial_reads'].items()}
        with patch.object(probe, 'retain'), patch.object(probe, 'facts', return_value=victim['initial']), \
                patch.object(probe, 'frozen_time', return_value=100), \
                patch.object(dragon, 'typed_predicate', side_effect=lambda server, text: {'predicate': text}):
            actual = probe.summon('death_200', health='0.01f')
            self.assertEqual(actual['uuid'], '13572468-2468-1357-0000-000000000019')
            self.assertEqual(actual['uuid_ints'][-1], 25)
            with self.assertRaises(AssertionError):
                probe.summon('default_hover')
            with self.assertRaises(AssertionError):
                probe.summon('death_200')
        for defect in ('old_roster', 'no_absence', 'wrong_uuid', 'overlapping_ids'):
            report = campaign()
            death = report['boots']['java_death1']
            if defect == 'old_roster':
                death['fresh_arena']['roster'].append(subject()['uuid'])
            elif defect == 'no_absence':
                death['sole_subject']['predicate'] = 'unless loaded 8192 200 8192'
            elif defect == 'wrong_uuid':
                death['subjects'][0]['uuid_ints'][-1] = 1
            else:
                frozen = report['boots']['java_frozen24']
                frozen['subjects'][1]['entity_id'] = frozen['subjects'][0]['entity_id'] + 8
            with self.subTest(defect=defect), self.assertRaises(AssertionError):
                dragon.validate_campaign(report)

    def test_all_frozen_read_points_and_final_facts_must_equal_actual_t0(self):
        for defect in ('row_time', 'attack_time', 'resident_time', 'baseline_time', 'after_close', 'sweep_time',
                       'real_step', 'position', 'health', 'phase', 'noai', 'uuid', 'missing_sweep', 'bare_verified'):
            report = campaign()
            boot = report['boots']['java_frozen24']
            case = boot['cases']['default_hover']
            sweep = boot['final_sweep'][case['subject']['uuid']]
            if defect == 'row_time':
                case['gametime_after'] += 1
            elif defect == 'attack_time':
                boot['cases']['part_head']['attacks'][0]['t0'] = 0
            elif defect == 'resident_time':
                case['cleanup']['resident']['gametime_before'] += 1
            elif defect == 'baseline_time':
                case['subject']['gametime_before'] = 0
            elif defect == 'after_close':
                boot['after_client_close_gametime'] += 1
            elif defect == 'sweep_time':
                sweep['gametime_after'] += 1
            elif defect == 'real_step':
                boot['steps'] = [{'delta': 1}]
            elif defect == 'position':
                sweep['position']['reads'][0]['actual'] = typed('Double', 0.5)
            elif defect == 'health':
                sweep['facts']['Health'] = typed('Float', 199)
            elif defect == 'phase':
                sweep['facts']['DragonPhase'] = typed('Int', 0)
            elif defect == 'noai':
                sweep['identity_predicate']['predicate'] = sweep['identity_predicate']['predicate'].replace('NoAI:1b', 'NoAI:0b')
            elif defect == 'uuid':
                sweep['uuid'] = subject('phase_0')['uuid']
            elif defect == 'missing_sweep':
                boot['final_sweep'].pop(case['subject']['uuid'])
            else:
                case['cleanup']['resident']['position']['verified'] = False
            with self.subTest(defect=defect), self.assertRaises(AssertionError):
                dragon.validate_campaign(report)

    def test_non_target_health_needs_exact_live_witness_and_fixed_last_literal_cohort(self):
        for defect in ('omit_state', 'omit_old_part', 'wrong_uuid', 'wrong_type', 'copied_value', 'changed_literal',
                       'wrong_command', 'predicate_damage', 'last_committed', 'unscaled_target'):
            report = campaign()
            case = report['boots']['java_frozen24']['cases']['part_neck']
            hit = case['attacks'][0]
            identity = subject()['uuid']
            old_part = dragon.supplied_identity('part_head')['uuid']
            if defect in ('omit_state', 'omit_old_part'):
                key = identity if defect == 'omit_state' else old_part
                for field in ('before', 'after', 'retained_health'):
                    hit[field].pop(key)
            elif defect == 'wrong_uuid':
                hit['after'][identity]['uuid'] = old_part
            elif defect == 'wrong_type':
                hit['after'][identity]['nbt_type'] = 'Double'
            elif defect == 'copied_value':
                hit['after'][identity] = typed('Float', 200)
                hit['after'][identity].update(uuid=identity, command='', predicate={'predicate': ''})
            elif defect == 'changed_literal':
                hit['after'][identity]['literal'] = '199.0f'
            elif defect == 'wrong_command':
                hit['after'][identity]['command'] = f'if data entity {old_part} {{Health:200.0f}}'
            elif defect == 'predicate_damage':
                hit['after'][identity]['predicate']['predicate'] = f'if data entity {identity} {{Health:199.0f}}'
            elif defect == 'last_committed':
                hit['retained_health'][old_part] = typed('Float', 200)
                hit['before'][old_part] = non_target(old_part, typed('Float', 200))
                hit['after'][old_part] = non_target(old_part, typed('Float', 200))
            else:
                hit['after'][case['subject']['uuid']]['full_read']['scaled_read']['expected'] = 0
            with self.subTest(defect=defect), self.assertRaises(AssertionError):
                dragon.validate_campaign(report)

    def test_non_target_probe_uses_live_predicate_and_retains_failed_damage_witness(self):
        probe = dragon.Probe(Mock(), Mock(), {'setup': {}, 'role': 'java_frozen24', 't0': 100})
        victim, other = subject('part_head'), subject('phase_0', 110)
        probe.subjects = [victim, other]
        probe.health = {entry['uuid']: entry['initial']['Health'] for entry in probe.subjects}
        probe.last_reads[(victim['uuid'], 'Health')] = victim['initial_reads']['Health']
        command = f'if data entity {other["uuid"]} {{Health:200.0f}}'
        with patch.object(probe, 'retain'), patch.object(probe, 'field', return_value=victim['initial']['Health']) as full, \
                patch.object(dragon, 'typed_predicate', side_effect=lambda server, text: {'predicate': text}) as live:
            values = probe.health_map(victim)
            full.assert_called_once_with(victim, 'Health', 'Float')
            live.assert_called_once_with(probe.server, command)
            self.assertEqual(values[other['uuid']], non_target(other['uuid'], other['initial']['Health']))
        retained = []
        with patch.object(probe, 'retain', side_effect=lambda: retained.append(deepcopy(probe.entry))), \
                patch.object(probe, 'field', return_value=victim['initial']['Health']), \
                patch.object(dragon, 'typed_predicate', side_effect=AssertionError('actual predicate failed: Health changed')):
            with self.assertRaisesRegex(AssertionError, 'Health changed'):
                probe.health_map(victim)
            witness = probe.case['non_target_witnesses'][-1]
            self.assertEqual(witness['command'], command)
            self.assertNotIn('predicate', witness)
            probe.entry['cases'] = {victim['label']: {}}
            cleanup = Mock()
            with patch.object(dragon, 'interaction_deadline'):
                self.assertFalse(probe.row(victim['label'], lambda: probe.health_map(victim), cleanup))
            failed = retained[-1]['cases'][victim['label']]
            self.assertEqual(failed['result'], 'failed')
            self.assertEqual(failed['non_target_witnesses'][-1], witness)
            self.assertIn('Health changed', failed['error'])
            cleanup.assert_not_called()


def queued_client(*, dragon_parts=True, jukeboxes=False, crafting=False) -> dragon.Client:
    """Use the real receiver with an in-memory queue, without starting a client."""
    observed = dragon.Client.__new__(dragon.Client)
    observed.messages = queue.Queue()
    observed.failure = None
    observed.pending = {}
    observed.events = []
    observed.dragon_wire_events = 0
    observed.statistics_values = {}
    observed.dragon_parts = dragon_parts
    observed.jukeboxes = jukeboxes
    observed.crafting = crafting
    return observed


class DragonReceiveTests(unittest.TestCase):
    def receive_all(self, observed, messages):
        for message in messages:
            observed.messages.put(message)
            self.assertIs(observed.receive(1), message)

    def test_mixed_exact_boundaries_preserve_all_records_in_order(self):
        observed = queued_client()
        wire = envelope(b'\x21\x00', event='wire_packet', packet_id=33, direction='clientbound', pid=7)
        messages = [message for sequence in range(4096) for message in (
            {**wire, 'sequence': sequence, 'time': 'synthetic'},
            {'event': 'chat', 'sequence': sequence, 'text': f'synthetic-{sequence}'})]
        expected = deepcopy(messages)
        self.receive_all(observed, messages)
        self.assertEqual(len(observed.events), 4096 + 4096)
        self.assertEqual(observed.events, expected)

    def test_wire_4097_fails_independently_and_preserves_offender(self):
        observed = queued_client()
        messages = [{'event': 'ready'}, *({'event': 'wire_packet', 'sequence': index} for index in range(4096))]
        self.receive_all(observed, messages)
        offender = envelope(b'\x21\x00', event='wire_packet', packet_id=33, sequence=4096)
        observed.messages.put(offender)
        with self.assertRaisesRegex(AssertionError, '^Dragon wire IPC evidence budget exceeded$'):
            observed.receive(1)
        self.assertEqual(observed.events, messages + [offender])
        self.assertIs(observed.events[-1], offender)

    def test_nonwire_4097_fails_independently_and_preserves_offender(self):
        observed = queued_client()
        kinds = ('chat', 'dragon_packet', 'statistics_packet', 'menu', 'position', 'command_tree', 'ready')
        messages = [{'event': 'wire_packet'}, *({'event': kinds[index % len(kinds)], 'sequence': index, 'entries': []}
                                              for index in range(4096))]
        self.receive_all(observed, messages)
        offender = {'event': 'chat', 'text': 'synthetic overflow'}
        observed.messages.put(offender)
        with self.assertRaisesRegex(AssertionError, '^Dragon IPC evidence budget exceeded$'):
            observed.receive(1)
        self.assertEqual(observed.events, messages + [offender])
        self.assertIs(observed.events[-1], offender)

    def test_replies_do_not_charge_unsolicited_categories(self):
        observed = queued_client()
        messages = [message for index in range(4096) for message in (
            {'event': 'wire_packet', 'sequence': index}, {'event': 'chat', 'text': f'synthetic-{index}'})]
        snapshot = {'packets': 4096, 'dragon_packets': [{'kind': 'spawn_header'}], 'chat_messages': messages[1::2]}
        replies = [{'id': index, 'ok': True, 'result': deepcopy(snapshot)} for index in range(1, 4)]
        replies += [{'id': 4, 'ok': True, 'result': {'written': True, 'entity_id': -2147483000}},
                    {'id': 5, 'ok': True, 'result': {'written': True, 'after_sequence': 4096}}]
        self.receive_all(observed, replies)
        self.receive_all(observed, messages)
        self.receive_all(observed, replies)
        self.assertEqual(observed.pending, {reply['id']: reply for reply in replies})
        self.assertEqual(observed.events, messages)

    def test_non_dragon_modes_and_fatal_events_keep_existing_contracts(self):
        for mode, label in (('jukeboxes', 'Jukebox'), ('crafting', 'Crafting'), (None, '')):
            for dragon_parts in ((False, True) if mode else (False,)):
                with self.subTest(mode=mode, dragon_parts=dragon_parts):
                    observed = queued_client(dragon_parts=dragon_parts, **({mode: True} if mode else {}))
                    messages = [{'event': 'wire_packet' if index % 2 else 'chat', 'sequence': index}
                                for index in range(16384 if mode else 101)]
                    self.receive_all(observed, messages)
                    self.assertEqual(observed.events, messages if mode else messages[-100:])
                    if mode:
                        offender = {'event': 'chat', 'text': 'synthetic overflow'}
                        observed.messages.put(offender)
                        with self.assertRaisesRegex(AssertionError, f'^{label} IPC evidence budget exceeded$'):
                            observed.receive(1)
                        self.assertEqual(observed.events, messages + [offender])
        for dragon_parts in (False, True):
            for fatal in ({'event': 'error', 'message': 'synthetic error'}, {'event': 'eof'}, {'event': 'end'}):
                with self.subTest(dragon_parts=dragon_parts, fatal=fatal):
                    observed = queued_client(dragon_parts=dragon_parts)
                    observed.messages.put(fatal)
                    with self.assertRaises(RuntimeError) as failed:
                        observed.receive(1)
                    failure = fatal.get('message', f'Client ended: {fatal}')
                    self.assertEqual(str(failed.exception), failure)
                    self.assertEqual(observed.failure, failure)
                    self.assertEqual(observed.events, [])
                    observed.messages.put({'event': 'ready'})
                    with self.assertRaises(RuntimeError) as repeated:
                        observed.receive(1)
                    self.assertEqual(str(repeated.exception), failure)
                    self.assertEqual(observed.messages.qsize(), 1)
            observed = queued_client(dragon_parts=dragon_parts)
            expected_end = {'event': 'end', 'expected': True}
            self.receive_all(observed, [expected_end])
            self.assertEqual(observed.events, [expected_end])


if __name__ == '__main__':
    unittest.main()
