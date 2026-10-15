"""Evidence import must preserve provenance without upgrading selected tests to parity."""
from __future__ import annotations

import contextlib
import copy
import hashlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools'))
import record_evidence as importer
from validate_features import validate_inventory


class RecordEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.inventory = json.loads((ROOT / 'compatibility/features.json').read_text())
        for feature in self.inventory['features']:
            feature.update(rust_status='unverified', evidence=[])
        self.target = json.loads((ROOT / 'compatibility/target.json').read_text())
        self.mapping = json.loads((ROOT / 'compatibility/scenario-map.json').read_text())
        cases = [{'id': name, 'result': 'passed', 'detail': {'observed': True}} for name in (
            'network.status_fields', 'network.fragmented', 'network.concurrent_status',
        )]
        self.report = {
            'schema_version': 1, 'target_version': self.target['version'],
            'protocol': self.target['protocol'], 'server_sha1': self.target['server_sha1'],
            'rust_binary_sha256': 'a' * 64, 'test_suite_sha256': 'b' * 64,
            'environment': {'host': 'test-host', 'platform': 'linux', 'auth': 'offline loopback', 'seed': '12345678'},
            'servers': {'vanilla': copy.deepcopy(cases), 'rust': copy.deepcopy(cases)},
        }

    def ingest(self, *, report=None, mapping=None, inventory=None):
        return importer.ingest_report(
            inventory or self.inventory, report or self.report, mapping or self.mapping, self.target,
            artifact='compatibility/results/evidence/test.json', artifact_sha256='c' * 64,
        )

    def test_passing_selection_is_partial_and_idempotent(self):
        updated, counts = self.ingest()
        feature = next(feature for feature in updated['features'] if feature['id'] == 'network.status')
        self.assertEqual(feature['rust_status'], 'partial')
        evidence = next(record for record in feature['evidence'] if record['test_id'] == 'network.status_fields')
        self.assertEqual(evidence['vanilla']['build_id'], 'sha1:' + self.target['server_sha1'])
        self.assertEqual(evidence['rust']['build_id'], 'sha256:' + 'a' * 64)
        self.assertEqual(evidence['report_sha256'], 'c' * 64)
        self.assertEqual(counts, {'observations': 2, 'paired_passes': 2, 'failed_or_skipped': 0, 'unmapped': 1})
        rerun, _ = self.ingest(inventory=updated)
        self.assertEqual(rerun, updated)
        self.assertFalse(any(feature['rust_status'] == 'verified' for feature in updated['features']))
        self.assertEqual(validate_inventory(updated), [])

    def test_failed_reference_or_rust_remains_visible(self):
        self.report['servers']['rust'][0].update(result='failed', error='unsupported behavior')
        updated, counts = self.ingest()
        feature = next(feature for feature in updated['features'] if feature['id'] == 'network.status')
        record = next(record for record in feature['evidence'] if record['test_id'] == 'network.status_fields')
        self.assertEqual(record['rust']['result'], 'failed')
        self.assertEqual(record['rust']['error'], 'unsupported behavior')
        self.assertEqual(counts['failed_or_skipped'], 1)
        self.assertNotEqual(feature['rust_status'], 'verified')
        self.report['servers']['vanilla'][0]['result'] = 'failed'
        updated, _ = self.ingest()
        feature = next(feature for feature in updated['features'] if feature['id'] == 'network.status')
        self.assertEqual(feature['evidence'][-1]['vanilla']['result'], 'failed')

    def test_changed_inputs_reject_report_but_ordinary_failures_and_dirty_source_remain_partial(self):
        for failure in ('harness.changed_during_run', 'rust_binary.changed_during_run'):
            with self.subTest(failure=failure):
                report = {**self.report, 'failed': [failure]}
                with self.assertRaisesRegex(ValueError, 'integrity failure'):
                    self.ingest(report=report)
        self.report['failed'] = ['rust:network.status_fields']
        self.report['source_state'] = {'commit': 'd' * 40, 'dirty': True}
        self.report['servers']['rust'][0].update(result='failed', error='behavior mismatch')
        updated, counts = self.ingest()
        self.assertEqual(counts['failed_or_skipped'], 1)
        self.assertFalse(any(feature['rust_status'] == 'verified' for feature in updated['features']))
        framing = next(feature for feature in updated['features'] if feature['id'] == 'network.framing')
        self.assertEqual(framing['rust_status'], 'partial')

    def test_rejects_wrong_target_or_missing_provenance(self):
        for key, value in (
            ('target_version', 'wrong'), ('protocol', -1), ('server_sha1', '0' * 40),
            ('rust_binary_sha256', None), ('test_suite_sha256', 'not-a-hash'),
        ):
            with self.subTest(key=key):
                changed = {**self.report, key: value}
                with self.assertRaises(ValueError):
                    self.ingest(report=changed)

    def test_rejects_missing_server_unknown_duplicate_and_unpaired_cases(self):
        for mutation in ('missing_server', 'unknown', 'duplicate', 'unpaired'):
            with self.subTest(mutation=mutation):
                changed = copy.deepcopy(self.report)
                if mutation == 'missing_server':
                    changed['servers'].pop('vanilla')
                elif mutation == 'unknown':
                    changed['servers']['rust'][0]['id'] = 'unknown.case'
                elif mutation == 'duplicate':
                    changed['servers']['rust'].append(changed['servers']['rust'][0])
                else:
                    changed['servers']['vanilla'].pop()
                with self.assertRaises(ValueError):
                    self.ingest(report=changed)

    def test_rejects_unknown_feature_and_required_scenario_in_mapping(self):
        for key, value in (('feature_id', 'network.does_not_exist'), ('scenario', 'not a required scenario')):
            with self.subTest(key=key):
                changed = copy.deepcopy(self.mapping)
                changed['cases']['network.status_fields'][key] = value
                with self.assertRaises(ValueError):
                    self.ingest(mapping=changed)

    def test_rejects_nonrelative_artifact(self):
        with self.assertRaises(ValueError):
            importer.ingest_report(self.inventory, self.report, self.mapping, self.target,
                                   artifact='../outside.json', artifact_sha256='c' * 64)

    def test_cli_snapshots_exact_report_and_ledger_passes_artifact_check(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, value in (('report', self.report), ('inventory', self.inventory), ('mapping', self.mapping), ('target', self.target)):
                (root / f'{name}.json').write_text(json.dumps(value))
            args = [str(root / 'report.json'), '--inventory', str(root / 'inventory.json'),
                    '--mapping', str(root / 'mapping.json'), '--target', str(root / 'target.json'),
                    '--output', str(root / 'imported.json')]
            with patch.object(importer, 'ROOT', root), contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(importer.main([*args, '--dry-run']), 0)
                self.assertFalse((root / 'imported.json').exists())
                self.assertEqual(importer.main(args), 0)
            updated = json.loads((root / 'imported.json').read_text())
            self.assertEqual(validate_inventory(updated, artifact_root=root), [])
            digest = hashlib.sha256((root / 'report.json').read_bytes()).hexdigest()
            snapshot = root / f'compatibility/results/evidence/{digest}.json'
            self.assertEqual(snapshot.read_bytes(), (root / 'report.json').read_bytes())
            snapshot.write_text('{}')
            self.assertTrue(any('SHA-256 mismatch' in error for error in validate_inventory(updated, artifact_root=root)))


if __name__ == '__main__':
    unittest.main()
