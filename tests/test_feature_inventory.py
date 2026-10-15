"""Exercise evidence and completion gates rather than duplicating the feature list."""

from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('validate_features', ROOT / 'tools/validate_features.py')
assert SPEC and SPEC.loader
validator = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(validator)


class FeatureInventoryTests(unittest.TestCase):
    def setUp(self):
        self.document = json.loads((ROOT / 'compatibility/features.json').read_text())
        self.document['features'] = [copy.deepcopy(self.document['features'][0])]
        self.feature = self.document['features'][0]
        self.feature['required_scenarios'] = ['happy path', 'failure path']
        self.feature['evidence'] = []
        self.feature['rust_status'] = 'unverified'

    def add_passing_evidence(self):
        self.feature['evidence'] = [
            {
                'test_id': f'protocol.handshake.{index}',
                'scenario': scenario,
                'target_version': self.document['target']['version'],
                'environment': 'isolated test server; Java and Rust build details in artifacts',
                'vanilla': {'result': 'passed', 'build_id': 'official-server-sha256', 'artifact': 'artifacts/vanilla.json'},
                'rust': {'result': 'passed', 'build_id': 'rust-git-commit', 'artifact': 'artifacts/rust.json'},
            }
            for index, scenario in enumerate(self.feature['required_scenarios'])
        ]
        if self.feature['rust_status'] == 'verified':
            for record in self.feature['evidence']:
                record.update(report_sha256='e' * 64, test_suite_sha256='d' * 64)
                record['vanilla']['build_id'] = 'sha1:' + 'c' * 40
                record['rust']['build_id'] = 'sha256:' + 'a' * 64

    def write_verified_snapshot(self, root: Path, *, report_changes=None):
        self.feature['rust_status'] = 'verified'
        self.add_passing_evidence()
        report = {
            'schema_version': 1, 'target_version': self.document['target']['version'],
            'server_sha1': 'c' * 40, 'rust_binary_sha256': 'a' * 64,
            'test_suite_sha256': 'd' * 64,
            'source_state': {'commit': 'f' * 40, 'dirty': False},
            'servers': {implementation: [{'id': record['test_id'], 'result': 'passed'}
                                        for record in self.feature['evidence']]
                        for implementation in ('vanilla', 'rust')},
        }
        if report_changes:
            report_changes(report)
        raw = json.dumps(report).encode()
        (root / 'artifacts').mkdir()
        (root / 'artifacts/report.json').write_bytes(raw)
        for record in self.feature['evidence']:
            record['report_sha256'] = hashlib.sha256(raw).hexdigest()
            for implementation in ('vanilla', 'rust'):
                record[implementation]['artifact'] = 'artifacts/report.json'
        (root / 'compatibility').mkdir()
        (root / 'compatibility/scenario-map.json').write_text(json.dumps({
            'target_version': self.document['target']['version'],
            'cases': {record['test_id']: {'feature_id': self.feature['id'], 'scenario': record['scenario']}
                      for record in self.feature['evidence']},
        }))

    def test_checked_in_inventory_is_valid(self):
        document = json.loads((ROOT / 'compatibility/features.json').read_text())
        self.assertEqual(validator.validate_inventory(document), [])

    def test_unverified_is_valid_but_fails_completion(self):
        self.assertEqual(validator.validate_inventory(self.document), [])
        self.assertTrue(validator.completion_errors(self.document))

    def test_verified_requires_all_scenarios_and_paired_results(self):
        self.feature['rust_status'] = 'verified'
        self.assertTrue(validator.validate_inventory(self.document))
        self.add_passing_evidence()
        self.assertEqual(validator.validate_inventory(self.document), [])
        self.assertEqual(validator.completion_errors(self.document), [])
        for mutation in ('missing_scenario', 'failed_rust', 'skipped_vanilla', 'wrong_version', 'missing_test'):
            with self.subTest(mutation=mutation):
                mutated = copy.deepcopy(self.document)
                evidence = mutated['features'][0]['evidence']
                if mutation == 'missing_scenario':
                    evidence.pop()
                elif mutation == 'failed_rust':
                    evidence[0]['rust']['result'] = 'failed'
                elif mutation == 'skipped_vanilla':
                    evidence[0]['vanilla']['result'] = 'skipped'
                elif mutation == 'wrong_version':
                    evidence[0]['target_version'] = 'different-version'
                else:
                    evidence[0].pop('test_id')
                self.assertTrue(validator.validate_inventory(mutated))

    def test_verified_cannot_combine_scenarios_from_different_rust_builds(self):
        self.feature['rust_status'] = 'verified'
        self.add_passing_evidence()
        self.feature['evidence'][1]['rust']['build_id'] = 'sha256:' + 'b' * 64
        self.assertTrue(any('same Rust build' in error for error in validator.validate_inventory(self.document)))

    def test_failure_on_same_build_cannot_be_masked_by_another_passing_test(self):
        self.feature['rust_status'] = 'verified'
        self.add_passing_evidence()
        failed = copy.deepcopy(self.feature['evidence'][0])
        failed['test_id'] = 'another.required.check'
        failed['rust']['result'] = 'failed'
        self.feature['evidence'].append(failed)
        self.assertTrue(validator.validate_inventory(self.document))
        self.assertTrue(validator.completion_errors(self.document))

    def test_target_pin_drift_is_rejected(self):
        pin = json.loads((ROOT / 'compatibility/target.json').read_text())
        self.assertEqual(validator.validate_target(self.document, pin), [])
        for key in ('edition', 'version', 'metadata_sha1'):
            with self.subTest(key=key):
                changed = {**pin, key: 'different'}
                self.assertTrue(validator.validate_target(self.document, changed))

    def test_completion_requires_one_build_across_all_required_features(self):
        self.feature['rust_status'] = 'verified'
        self.add_passing_evidence()
        second = copy.deepcopy(self.feature)
        second['id'] = 'network.other'
        for record in second['evidence']:
            record['rust']['build_id'] = 'sha256:' + 'b' * 64
        self.document['features'].append(second)
        self.assertEqual(validator.validate_inventory(self.document), [])
        self.assertTrue(any('single verified Rust build' in error for error in validator.completion_errors(self.document)))

    def test_duplicate_ids_and_optional_vanilla_are_rejected(self):
        self.document['features'].append(copy.deepcopy(self.feature))
        self.assertTrue(any('duplicate feature ID' in error for error in validator.validate_inventory(self.document)))
        self.document['features'].pop()
        self.feature['required'] = False
        self.assertTrue(any('must be required' in error for error in validator.validate_inventory(self.document)))

    def test_plugin_deferral_and_separate_multicore_gate(self):
        self.feature.update(id='plugins.compatibility', category='plugins', scope='deferred_plugin', required=False)
        self.assertEqual(validator.validate_inventory(self.document), [])
        self.assertEqual(validator.completion_errors(self.document, include_implementation=True), [])
        self.feature.update(id='concurrency.ownership', category='concurrency', scope='implementation', required=True)
        self.assertEqual(validator.completion_errors(self.document), [])
        self.assertTrue(validator.completion_errors(self.document, include_implementation=True))

    def test_scope_cannot_hide_a_vanilla_feature_from_the_gate(self):
        self.feature.update(scope='deferred_plugin', required=False)
        self.assertTrue(any('category requires scope vanilla' in error for error in validator.validate_inventory(self.document)))

    def test_artifact_paths_and_materialized_evidence(self):
        self.add_passing_evidence()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.assertTrue(validator.validate_inventory(self.document, artifact_root=root))
            (root / 'artifacts').mkdir()
            for filename in ('vanilla.json', 'rust.json'):
                (root / 'artifacts' / filename).write_text('{}')
            self.assertEqual(validator.validate_inventory(self.document, artifact_root=root), [])
        for path in ('/etc/passwd', '../outside.json', 'https://example.com/run', 'artifacts\\run.json'):
            with self.subTest(path=path):
                self.feature['evidence'][0]['rust']['artifact'] = path
                self.assertTrue(validator.validate_inventory(self.document))

    def test_report_artifact_hash_detects_overwritten_evidence(self):
        self.add_passing_evidence()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'artifacts').mkdir()
            for filename in ('vanilla.json', 'rust.json'):
                (root / 'artifacts' / filename).write_text('{}')
            self.feature['evidence'][0]['report_sha256'] = '0' * 64
            self.assertTrue(any('SHA-256 mismatch' in error for error in validator.validate_inventory(self.document, artifact_root=root)))

    def test_verified_cannot_use_unrelated_existing_json_without_report_hash(self):
        self.feature['rust_status'] = 'verified'
        self.add_passing_evidence()
        for record in self.feature['evidence']:
            record.pop('report_sha256')
            record['vanilla']['artifact'] = record['rust']['artifact'] = 'compatibility/target.json'
        self.assertTrue(any('requires report_sha256' in error for error in validator.validate_inventory(self.document, artifact_root=ROOT)))

    def test_verified_snapshot_is_bound_to_actual_test_results_target_and_builds(self):
        for mutation in ('none', 'unrelated', 'missing_test', 'failed_result', 'wrong_version', 'wrong_build', 'wrong_suite'):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)

                def mutate(report):
                    if mutation == 'unrelated':
                        report.clear()
                    elif mutation == 'missing_test':
                        report['servers']['rust'][0]['id'] = 'another.test'
                    elif mutation == 'failed_result':
                        report['servers']['rust'][0]['result'] = 'failed'
                    elif mutation == 'wrong_version':
                        report['target_version'] = 'wrong'
                    elif mutation == 'wrong_build':
                        report['rust_binary_sha256'] = 'f' * 64
                    elif mutation == 'wrong_suite':
                        report['test_suite_sha256'] = 'f' * 64

                self.write_verified_snapshot(root, report_changes=mutate)
                errors = validator.validate_inventory(self.document, artifact_root=root)
                self.assertEqual(bool(errors), mutation != 'none', errors)

    def test_verified_snapshot_cannot_claim_a_different_scenario(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_verified_snapshot(root)
            self.feature['evidence'][0]['scenario'] = 'failure path'
            errors = validator.validate_inventory(self.document, artifact_root=root)
            self.assertTrue(any('does not map' in error for error in errors))

    def test_verified_snapshot_rejects_changed_inputs_or_unreproducible_source(self):
        for mutation in ('changed_harness', 'changed_binary', 'dirty_source', 'missing_source', 'bad_commit', 'nonboolean_dirty'):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)

                def mutate(report):
                    if mutation == 'changed_harness':
                        report['failed'] = ['harness.changed_during_run']
                    elif mutation == 'changed_binary':
                        report['failed'] = ['rust_binary.changed_during_run']
                    elif mutation == 'dirty_source':
                        report['source_state']['dirty'] = True
                    elif mutation == 'missing_source':
                        report.pop('source_state')
                    elif mutation == 'bad_commit':
                        report['source_state']['commit'] = 'unknown'
                    else:
                        report['source_state']['dirty'] = 0

                self.write_verified_snapshot(root, report_changes=mutate)
                self.assertTrue(validator.validate_inventory(self.document, artifact_root=root))

    def test_strict_cli_checks_artifacts_without_optional_flag(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_verified_snapshot(root)
            self.feature['evidence'][0]['rust']['artifact'] = 'missing.json'
            (root / 'inventory.json').write_text(json.dumps(self.document))
            target = {'edition': 'java', 'version': self.document['target']['version'],
                      'metadata_sha1': self.document['target']['version_metadata_sha1'], 'server_sha1': 'c' * 40}
            (root / 'target.json').write_text(json.dumps(target))
            with patch.object(validator, 'ROOT', root):
                self.assertEqual(validator.main([str(root / 'inventory.json'), '--target', str(root / 'target.json'), '--strict']), 2)

    def test_malformed_shapes_return_errors_instead_of_crashing(self):
        for value in (None, [], {}, {'schema_version': True, 'features': [None]},
                      {**self.document, 'features': [{**self.feature, 'scope': [], 'rust_status': {}}]}):
            with self.subTest(value=value):
                self.assertTrue(validator.validate_inventory(value))


if __name__ == '__main__':
    unittest.main()
