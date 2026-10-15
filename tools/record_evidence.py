#!/usr/bin/env python3
"""Import paired suite observations into the ledger without asserting full parity."""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import sys

from validate_features import ROOT, RESULTS, report_integrity_errors, validate_inventory, validate_target


def _require(condition: object, message: str) -> None:
    if not condition:
        raise ValueError(message)


def _digest(value: object, length: int) -> bool:
    return isinstance(value, str) and re.fullmatch(f'[a-f0-9]{{{length}}}', value) is not None


def validate_mapping(mapping: object, inventory: dict, target: dict) -> dict:
    """Reject misspelled features/scenarios even when absent from this run."""
    _require(isinstance(mapping, dict) and mapping.get('schema_version') == 1, 'mapping schema_version must be 1')
    _require(mapping.get('target_version') == target['version'], 'mapping target_version differs from target')
    cases = mapping.get('cases')
    _require(isinstance(cases, dict) and cases, 'mapping.cases must be a nonempty object')
    features = {feature['id']: feature for feature in inventory['features']}
    for test_id, entry in cases.items():
        _require(isinstance(test_id, str) and test_id.strip(), 'mapping test IDs must be nonempty strings')
        _require(isinstance(entry, dict), f'{test_id}: mapping must be an object')
        if 'skip_reason' in entry:
            _require(set(entry) == {'skip_reason'} and isinstance(entry['skip_reason'], str) and entry['skip_reason'].strip(), f'{test_id}: invalid skip_reason mapping')
            continue
        _require(set(entry) == {'feature_id', 'scenario'}, f'{test_id}: mapping needs feature_id and scenario')
        feature_id = entry['feature_id']
        _require(isinstance(feature_id, str) and feature_id in features, f'{test_id}: unknown feature ID')
        _require(entry['scenario'] in features[feature_id]['required_scenarios'], f'{test_id}: unknown required scenario')
        _require(features[feature_id]['scope'] == 'vanilla', f'{test_id}: suite importer only accepts vanilla mappings')
    return cases


def ingest_report(inventory: dict, report: object, mapping: object, target: dict, *, artifact: str, artifact_sha256: str) -> tuple[dict, dict]:
    """Return a new ledger plus counts; retain failures and replace repeated observations.

    Every imported result is a selected observation, so affected features can only
    become partial. Explicitly ignored cases remain visible in the source report.
    """
    errors = validate_inventory(inventory) + validate_target(inventory, target)
    _require(not errors, '; '.join(errors))
    cases = validate_mapping(mapping, inventory, target)
    _require(isinstance(report, dict) and report.get('schema_version') == 1, 'report schema_version must be 1')
    integrity_errors = report_integrity_errors(report)
    _require(not integrity_errors, '; '.join(integrity_errors))
    _require(report.get('target_version') == target['version'], 'report target_version differs from target')
    _require(report.get('protocol') == target['protocol'], 'report protocol differs from target')
    _require(_digest(report.get('server_sha1'), 40) and report['server_sha1'] == target['server_sha1'], 'report official server SHA-1 differs from target')
    _require(_digest(report.get('rust_binary_sha256'), 64), 'report needs exact Rust binary SHA-256')
    _require(_digest(report.get('test_suite_sha256'), 64), 'report needs exact suite SHA-256')
    _require(_digest(artifact_sha256, 64), 'artifact needs SHA-256')
    _require(isinstance(artifact, str) and artifact.strip(), 'artifact path must be nonempty')
    path = PurePosixPath(artifact)
    _require(not path.is_absolute() and '..' not in path.parts and '\\' not in artifact and '://' not in artifact, 'artifact path must be repository-relative')
    environment = report.get('environment')
    _require(isinstance(environment, dict) and environment, 'report needs environment details')
    for key in ('host', 'platform', 'auth', 'seed'):
        _require(isinstance(environment.get(key), str) and environment[key].strip(), f'report environment needs {key}')
    servers = report.get('servers')
    _require(isinstance(servers, dict) and set(servers) == {'vanilla', 'rust'}, 'report must contain both vanilla and Rust server results')
    paired: dict[str, dict] = {}
    for server, entries in servers.items():
        _require(isinstance(entries, list) and entries, f'{server}: cases must be a nonempty array')
        indexed = {}
        for entry in entries:
            _require(isinstance(entry, dict), f'{server}: case must be an object')
            test_id = entry.get('id')
            _require(isinstance(test_id, str) and test_id in cases, f'{server}: unknown case {test_id!r}')
            _require(test_id not in indexed, f'{server}: duplicate case {test_id}')
            _require(isinstance(entry.get('result'), str) and entry['result'] in RESULTS, f'{server}:{test_id}: invalid result')
            indexed[test_id] = entry
        paired[server] = indexed
    _require(paired['vanilla'].keys() == paired['rust'].keys(), 'vanilla and Rust must report identical case IDs')
    updated = copy.deepcopy(inventory)
    features = {feature['id']: feature for feature in updated['features']}
    counts = {'observations': 0, 'paired_passes': 0, 'failed_or_skipped': 0, 'unmapped': 0}
    for test_id in sorted(paired['vanilla']):
        entry = cases[test_id]
        if 'skip_reason' in entry:
            counts['unmapped'] += 1
            continue
        feature = features[entry['feature_id']]
        record = {
            'test_id': test_id, 'scenario': entry['scenario'], 'target_version': target['version'],
            'environment': json.dumps(environment, sort_keys=True),
            'report_sha256': artifact_sha256, 'test_suite_sha256': report['test_suite_sha256'],
        }
        for server, build_id in (
            ('vanilla', 'sha1:' + report['server_sha1']),
            ('rust', 'sha256:' + report['rust_binary_sha256']),
        ):
            result = paired[server][test_id]
            record[server] = {'result': result['result'], 'build_id': build_id, 'artifact': artifact}
            for field in ('detail', 'error'):
                if field in result:
                    record[server][field] = result[field]
        feature['evidence'] = [existing for existing in feature['evidence'] if (existing['test_id'], existing['scenario']) != (test_id, entry['scenario'])]
        feature['evidence'].append(record)
        passed = record['vanilla']['result'] == record['rust']['result'] == 'passed'
        if passed or feature['rust_status'] == 'verified':
            feature['rust_status'] = 'partial'
        counts['observations'] += 1
        counts['paired_passes' if passed else 'failed_or_skipped'] += 1
    errors = validate_inventory(updated)
    _require(not errors, '; '.join(errors))
    return updated, counts


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path, help='paired report produced by tools/parity/suite.py')
    parser.add_argument('--inventory', type=Path, default=ROOT / 'compatibility/features.json')
    parser.add_argument('--mapping', type=Path, default=ROOT / 'compatibility/scenario-map.json')
    parser.add_argument('--target', type=Path, default=ROOT / 'compatibility/target.json')
    parser.add_argument('--output', type=Path, help='write ledger here instead of replacing --inventory')
    parser.add_argument('--dry-run', action='store_true', help='validate and summarize without writing ledger or snapshots')
    args = parser.parse_args(argv)
    try:
        raw = args.report.read_bytes()
        digest = hashlib.sha256(raw).hexdigest()
        artifact = f'compatibility/results/evidence/{digest}.json'
        updated, counts = ingest_report(
            json.loads(args.inventory.read_text()), json.loads(raw), json.loads(args.mapping.read_text()),
            json.loads(args.target.read_text()), artifact=artifact, artifact_sha256=digest,
        )
        if not args.dry_run:
            snapshot = ROOT / artifact
            snapshot.parent.mkdir(parents=True, exist_ok=True)
            if snapshot.exists() and snapshot.read_bytes() != raw:
                raise ValueError('immutable evidence snapshot differs from its content hash')
            snapshot.write_bytes(raw)
            output = args.output or args.inventory
            output.parent.mkdir(parents=True, exist_ok=True)
            temporary = output.with_suffix(output.suffix + '.tmp')
            temporary.write_text(json.dumps(updated, indent=2) + '\n')
            temporary.replace(output)
        print(('Validated only: ' if args.dry_run else 'Recorded: ') + json.dumps(counts, sort_keys=True))
        return 0
    except (OSError, ValueError, TypeError, KeyError) as error:
        print(f'evidence import rejected: {error}', file=sys.stderr)
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
