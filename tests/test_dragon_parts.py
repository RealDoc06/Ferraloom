"""Synthetic source-derived validator witnesses, never real acceptance receipts.

No server, client, JVM or process is started by these focused units.
"""
import base64
from copy import deepcopy
import hashlib
from pathlib import Path
import struct
import sys
import unittest
from unittest.mock import patch
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
import check_dragon_parts as dragon


def envelope(raw: bytes, **fields) -> dict:
    return {'payload_base64': base64.b64encode(raw).decode(),
            'payload_sha256': hashlib.sha256(raw).hexdigest(), **fields}


def typed(kind, value):
    return {'nbt_type': kind, 'value': value}


def subject():
    identity = '13572468-2468-1357-0000-000000000001'
    raw = b'\x01\x64' + uuid.UUID(identity).bytes + b'\x2c' + struct.pack('>3d', 0, 0, 0) + b'opaque'
    spawn = envelope(raw, kind='spawn_header', packet_id=1, entity_id=100, uuid=identity, entity_type=44,
        x=0, y=0, z=0, sequence=10, complete=False, decoded_bytes=len(raw) - 6, payload_bytes=len(raw),
        undecoded_remainder_base64=base64.b64encode(b'opaque').decode())
    return {'uuid': identity, 'entity_id': 100, 'spawn': spawn,
            'initial': {'Health': typed('Float', 200), 'DragonPhase': typed('Int', 10),
                        'DragonDeathTime': typed('Int', 0), 'sitting_damage_received': typed('Float', 0)}}


def attack(*, negative=False):
    victim = subject()
    target = -2147483000 if negative else 101
    raw = b'\x01' + dragon.protocol.varint(target)
    request = envelope(raw, entity_id=target, packet_id=1, written=True, after_sequence=10)
    response = envelope(b'\x03\x00', packet_id=3, sequence=20, payload_bytes=2, decoded_bytes=2)
    return {'subject': victim, 'target_id': target, 'request': request, 'allocation_assumption': 'synthetic fresh allocator premise',
        'fence': {'request': {'packet_id': 12, 'action_id': 1, 'written': True, 'after_sequence': 11},
                  'response': response, 'observations': [victim['spawn']]},
        'wire': [envelope(raw, packet_id=1, direction='serverbound', pid=7),
                 envelope(b'\x0c\x01', packet_id=12, direction='serverbound', pid=7),
                 {**response, 'direction': 'clientbound', 'pid': 7}],
        'gametime_before': 100, 'gametime_after': 100, 'frozen_after': True,
        'before': {victim['uuid']: typed('Float', 200), 'another_current_victim': typed('Float', 200)},
        'after': {victim['uuid']: typed('Float', 200 if negative else 199.8), 'another_current_victim': typed('Float', 200)},
        'healthy_connection': {'phase': 'play', 'login': True}}


def state_case():
    victim = subject()
    return {'name': 'default_hover', 'result': 'passed', 'subject': victim,
        'values': deepcopy(victim['initial']), 'observations': [victim['spawn']],
        'gametime_before': 100, 'gametime_after': 100, 'evidence_retained': True,
        'timeline': ['started', 'evidence_retained', 'assertions_passed', 'cleanup_started', 'cleanup_complete'],
        'cleanup': {'verified': True}}


def death_case():
    case = state_case()
    lethal = attack()
    lethal['subject']['initial']['Health']['value'] = 0.01
    lethal['before'][lethal['subject']['uuid']]['value'] = 0.01
    lethal['after'][lethal['subject']['uuid']]['value'] = 0
    case.update(name='death_200', subject=lethal['subject'], attacks=[lethal],
                prelethal=deepcopy(lethal['subject']['initial']),
                before_step={'Health': typed('Float', 0), 'DragonDeathTime': typed('Int', 0)}, steps=[])
    elapsed, gametime = 0, 100
    for mark in (1, 20, 155, 199, 200):
        row = {'elapsed': mark, 'chunks': [], 'observations': [lethal['subject']['spawn']]}
        while elapsed < mark:
            delta = min(100, mark - elapsed)
            row['chunks'].append({'before': gametime, 'after': gametime + delta, 'delta': delta, 'frozen_after': True})
            elapsed += delta
            gametime += delta
        if mark < 200:
            row['values'] = {'DragonDeathTime': typed('Int', mark), 'Health': typed('Float', 0)}
        else:
            row['absent'] = {'predicate': 'unless entity ' + lethal['subject']['uuid']}
            row['natural_lookup_failure'] = {'success_zero': True}
            row['observations'].append(envelope(b'\x4e\x01\x64', kind='remove', packet_id=78, sequence=200,
                                               entity_ids=[100], complete=True, decoded_bytes=3, payload_bytes=3))
        case['steps'].append(row)
    return case


class DragonValidatorTests(unittest.TestCase):
    def test_authoritative_numeric_reads_reject_errors_wrong_types_and_ambiguity(self):
        self.assertEqual(dragon.parse_numeric(['Dragon has the following entity data: 200.0f'], 'Float')['value'], 200)
        self.assertEqual(dragon.parse_numeric(['Dragon has the following entity data: 37'], 'Int')['value'], 37)
        for lines, kind in [(['No entity was found'], 'Float'), (['Dragon has the following entity data: 200'], 'Float'),
                (['Dragon has the following entity data: 37f'], 'Int'), (['Dragon has the following entity data: NaNf'], 'Float'),
                (['Dragon has the following entity data: 10'] * 2, 'Int')]:
            with self.subTest(lines=lines), self.assertRaises(AssertionError):
                dragon.parse_numeric(lines, kind)

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
                    broken['after']['another_current_victim']['value'] = 199
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

    def test_partial_two_boot_reports_and_forced_cleanup_stay_failed(self):
        report = {'completed_boots': ['java_fresh'], 'cleanup_complete': False, 'runtime_errors': ['timeout'],
                  'provenance_checked_before': True, 'provenance_checked_after': False,
                  'boots': {'java_fresh': {'cases': {'default_hover': state_case()}}}}
        self.assertEqual(len(dragon.campaign_rows(report)), len(dragon.CASES))
        self.assertTrue(all(row['result'] == 'failed' for row in dragon.campaign_rows(report)))
        with self.assertRaises(AssertionError):
            dragon.validate_campaign(report)
        report.update(completed_boots=list(dragon.BOOTS), cleanup_complete=True, runtime_errors=[], provenance_checked_after=True)
        boot = {'result': 'passed', 'cases': {name: state_case() for name in dragon.CASES}, 'clients': [{}],
                'input_pins_checked_before_boot': True, 'teardown': {'exit_code': 0, 'forced_stop': True}}
        report['boots'] = {role: deepcopy(boot) for role in dragon.BOOTS}
        with self.assertRaisesRegex(AssertionError, 'server stop'):
            dragon.validate_campaign(report)


if __name__ == '__main__':
    unittest.main()
