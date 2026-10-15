#!/usr/bin/env python3
"""Validate Ferraloom's feature ledger and optionally enforce completion gates."""

from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import sys

STATUSES = frozenset({'unverified', 'missing', 'partial', 'verified'})
SCOPES = frozenset({'vanilla', 'implementation', 'deferred_plugin'})
RESULTS = frozenset({'passed', 'failed', 'skipped'})
ROOT = Path(__file__).resolve().parents[1]


def _nonempty(value: object) -> bool:
    return isinstance(value, str) and bool(value.strip())


def validate_target(document: dict, pinned: object) -> list[str]:
    """Keep the behavioral ledger on the same edition and artifact metadata pin."""
    if not isinstance(pinned, dict):
        return ['pinned target must be an object']
    target = document.get('target', {})
    if not isinstance(target, dict):
        return ['inventory target must be an object']
    errors = [
        f'target.{ledger_key} differs from pinned target {pin_key}'
        for ledger_key, pin_key in (
            ('edition', 'edition'), ('version', 'version'),
            ('version_metadata_sha1', 'metadata_sha1'),
        )
        if not _nonempty(pinned.get(pin_key)) or target.get(ledger_key) != pinned[pin_key]
    ]
    features = document.get('features', [])
    if not isinstance(features, list):
        return errors
    for feature in features:
        if isinstance(feature, dict) and feature.get('rust_status') == 'verified':
            evidence = feature.get('evidence', [])
            if not isinstance(evidence, list):
                continue
            for record in evidence:
                if isinstance(record, dict) and isinstance(record.get('vanilla'), dict) and record['vanilla'].get('result') == 'passed':
                    if record['vanilla'].get('build_id') != f'sha1:{pinned.get("server_sha1")}':
                        errors.append(f'{feature.get("id")}: verified reference build differs from pinned server SHA-1')
    return errors


def _snapshot_errors(report: object, record: dict, feature_id: str, mapping: object) -> list[str]:
    """Bind a verified claim to its actual paired observation, rather than a filename."""
    if not isinstance(report, dict) or type(report.get('schema_version')) is not int or report['schema_version'] != 1:
        return ['verified artifact must be a paired report with schema_version 1']
    errors = []
    if report.get('target_version') != record['target_version']:
        errors.append('artifact target version differs from evidence')
    if report.get('test_suite_sha256') != record.get('test_suite_sha256'):
        errors.append('artifact suite hash differs from evidence')
    if not isinstance(mapping, dict) or mapping.get('target_version') != record['target_version']:
        errors.append('verified evidence needs a matching scenario map')
    else:
        cases = mapping.get('cases', {})
        expected = {'feature_id': feature_id, 'scenario': record['scenario']}
        if not isinstance(cases, dict) or cases.get(record['test_id']) != expected:
            errors.append('artifact test ID does not map to this feature and required scenario')
    servers = report.get('servers')
    if not isinstance(servers, dict) or set(servers) != {'vanilla', 'rust'}:
        return errors + ['verified artifact must contain both server result arrays']
    for implementation, field, prefix, length in (
        ('vanilla', 'server_sha1', 'sha1:', 40),
        ('rust', 'rust_binary_sha256', 'sha256:', 64),
    ):
        digest = report.get(field)
        if not isinstance(digest, str) or not re.fullmatch(f'[a-f0-9]{{{length}}}', digest):
            errors.append(f'artifact is missing valid {field}')
        elif record[implementation]['build_id'] != prefix + digest:
            errors.append(f'artifact {implementation} build differs from evidence')
        entries = servers[implementation]
        if not isinstance(entries, list):
            errors.append(f'artifact {implementation} results must be an array')
            continue
        matches = [entry for entry in entries if isinstance(entry, dict) and entry.get('id') == record['test_id']]
        if len(matches) != 1:
            errors.append(f'artifact must contain exactly one {implementation} result for test ID')
            continue
        observed = matches[0]
        if observed.get('result') != record[implementation]['result']:
            errors.append(f'artifact {implementation} result contradicts evidence')
        for field in ('detail', 'error'):
            if field in record[implementation] and record[implementation][field] != observed.get(field):
                errors.append(f'artifact {implementation} {field} contradicts evidence')
    return errors


def validate_inventory(document: object, *, artifact_root: Path | None = None) -> list[str]:
    """Return actionable schema and evidence errors without trusting status claims.

    Artifact existence is optional so a checked-in ledger can reference CI artifacts
    materialized later. Passing artifact_root also checks repository-relative files.
    """
    errors: list[str] = []
    artifact_digests: dict[Path, str] = {}
    artifact_reports: dict[Path, object] = {}
    scenario_map: object = None
    mapping_loaded = False
    if not isinstance(document, dict):
        return ['inventory must be an object']
    if type(document.get('schema_version')) is not int or document['schema_version'] != 1:
        errors.append('schema_version must be 1')
    target = document.get('target')
    if not isinstance(target, dict):
        errors.append('target must be an object')
        target = {}
    if target.get('edition') != 'java':
        errors.append('target.edition must be java')
    if target.get('release_type') != 'release':
        errors.append('target.release_type must be release')
    for key in ('version', 'manifest_url', 'version_metadata_url', 'release_time', 'checked_on'):
        if not _nonempty(target.get(key)):
            errors.append(f'target.{key} must be a nonempty string')
    if not isinstance(target.get('version_metadata_sha1'), str) or not re.fullmatch(
        r'[a-f0-9]{40}', target['version_metadata_sha1']
    ):
        errors.append('target.version_metadata_sha1 must be a lowercase SHA-1 digest')
    features = document.get('features')
    if not isinstance(features, list) or not features:
        return errors + ['features must be a nonempty array']
    seen: set[str] = set()
    for index, feature in enumerate(features):
        label = f'features[{index}]'
        if not isinstance(feature, dict):
            errors.append(f'{label} must be an object')
            continue
        feature_id = feature.get('id')
        if not isinstance(feature_id, str) or not re.fullmatch(r'[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*', feature_id):
            errors.append(f'{label}.id must be category.feature in snake_case')
        else:
            label = feature_id
            if feature_id in seen:
                errors.append(f'{label}: duplicate feature ID')
            seen.add(feature_id)
            if feature.get('category') != feature_id.split('.')[0]:
                errors.append(f'{label}: category must match ID prefix')
        for key in ('category', 'name', 'vanilla_expectation'):
            if not _nonempty(feature.get(key)):
                errors.append(f'{label}.{key} must be a nonempty string')
        scope = feature.get('scope')
        if not isinstance(scope, str) or scope not in SCOPES:
            errors.append(f'{label}: invalid scope')
        expected_scope = {'plugins': 'deferred_plugin', 'concurrency': 'implementation'}.get(
            feature.get('category') if isinstance(feature.get('category'), str) else '', 'vanilla'
        )
        if scope != expected_scope:
            errors.append(f'{label}: category requires scope {expected_scope}')
        if type(feature.get('required')) is not bool:
            errors.append(f'{label}.required must be boolean')
        elif scope in ('vanilla', 'implementation') and not feature['required']:
            errors.append(f'{label}: vanilla and implementation features must be required')
        elif scope == 'deferred_plugin' and feature['required']:
            errors.append(f'{label}: deferred plugin features cannot gate vanilla completion')
        status = feature.get('rust_status')
        if not isinstance(status, str) or status not in STATUSES:
            errors.append(f'{label}: invalid rust_status')
        scenarios = feature.get('required_scenarios')
        if not isinstance(scenarios, list) or not scenarios or any(not _nonempty(s) for s in scenarios):
            errors.append(f'{label}.required_scenarios must be a nonempty string array')
            scenarios = []
        elif len(set(scenarios)) != len(scenarios):
            errors.append(f'{label}: duplicate required scenario')
        evidence = feature.get('evidence')
        if not isinstance(evidence, list):
            errors.append(f'{label}.evidence must be an array')
            evidence = []
        passing_by_build: dict[str, set[str]] = {}
        failing_by_build: dict[str, set[str]] = {}
        for evidence_index, record in enumerate(evidence):
            record_label = f'{label}.evidence[{evidence_index}]'
            if not isinstance(record, dict):
                errors.append(f'{record_label} must be an object')
                continue
            start_errors = len(errors)
            verified_paths = []
            report_digest = record.get('report_sha256')
            if report_digest is not None and (
                not isinstance(report_digest, str) or not re.fullmatch(r'[a-f0-9]{64}', report_digest)
            ):
                errors.append(f'{record_label}.report_sha256 must be a lowercase SHA-256 digest')
            for key in ('test_id', 'environment'):
                if not _nonempty(record.get(key)):
                    errors.append(f'{record_label}.{key} must be a nonempty string')
            scenario = record.get('scenario')
            if not isinstance(scenario, str) or scenario not in scenarios:
                errors.append(f'{record_label}.scenario must name a required scenario')
            if record.get('target_version') != target.get('version'):
                errors.append(f'{record_label}: target_version must match pinned version')
            paired_pass = True
            for implementation in ('vanilla', 'rust'):
                result = record.get(implementation)
                if not isinstance(result, dict):
                    errors.append(f'{record_label}.{implementation} must be an object')
                    paired_pass = False
                    continue
                outcome = result.get('result')
                if not isinstance(outcome, str) or outcome not in RESULTS:
                    errors.append(f'{record_label}.{implementation}: invalid result')
                paired_pass = paired_pass and outcome == 'passed'
                for key in ('build_id', 'artifact'):
                    if not _nonempty(result.get(key)):
                        errors.append(f'{record_label}.{implementation}.{key} must be a nonempty string')
                artifact = result.get('artifact')
                if _nonempty(artifact):
                    relative = PurePosixPath(artifact)
                    if relative.is_absolute() or '..' in relative.parts or '\\' in artifact or '://' in artifact:
                        errors.append(f'{record_label}.{implementation}.artifact must be a repository-relative path')
                    elif artifact_root is not None:
                        path = artifact_root / artifact
                        if not path.is_file():
                            errors.append(f'{record_label}.{implementation}: artifact does not exist: {artifact}')
                        elif not path.resolve().is_relative_to(artifact_root.resolve()):
                            errors.append(f'{record_label}.{implementation}: artifact escapes repository')
                        elif isinstance(report_digest, str):
                            if path not in artifact_digests:
                                artifact_digests[path] = hashlib.sha256(path.read_bytes()).hexdigest()
                            if artifact_digests[path] != report_digest:
                                errors.append(f'{record_label}.{implementation}: artifact SHA-256 mismatch')
                            elif status == 'verified':
                                verified_paths.append(path)
            if paired_pass and status == 'verified':
                for key in ('report_sha256', 'test_suite_sha256'):
                    if not isinstance(record.get(key), str) or not re.fullmatch(r'[a-f0-9]{64}', record[key]):
                        errors.append(f'{record_label}: verified evidence requires {key}')
                for implementation, pattern in (('vanilla', r'sha1:[a-f0-9]{40}'), ('rust', r'sha256:[a-f0-9]{64}')):
                    result = record.get(implementation)
                    if isinstance(result, dict) and (
                        not isinstance(result.get('build_id'), str) or not re.fullmatch(pattern, result['build_id'])
                    ):
                        errors.append(f'{record_label}: verified {implementation} build_id must identify its binary hash')
                if artifact_root is not None and len(errors) == start_errors:
                    if not mapping_loaded:
                        try:
                            scenario_map = json.loads((artifact_root / 'compatibility/scenario-map.json').read_text())
                        except (OSError, ValueError) as error:
                            errors.append(f'verified scenario map read failed: {error}')
                        mapping_loaded = True
                    for path in set(verified_paths):
                        try:
                            if path not in artifact_reports:
                                artifact_reports[path] = json.loads(path.read_text())
                            errors.extend(f'{record_label}: {error}' for error in _snapshot_errors(
                                artifact_reports[path], record, feature_id, scenario_map
                            ))
                        except (OSError, ValueError) as error:
                            errors.append(f'{record_label}: verified artifact read failed: {error}')
            if paired_pass and len(errors) == start_errors and isinstance(scenario, str):
                passing_by_build.setdefault(record['rust']['build_id'], set()).add(scenario)
            elif not paired_pass and len(errors) == start_errors and isinstance(scenario, str):
                rust = record.get('rust')
                if isinstance(rust, dict) and _nonempty(rust.get('build_id')):
                    failing_by_build.setdefault(rust['build_id'], set()).add(scenario)
        if status == 'verified':
            if not scenarios or not any(set(scenarios) <= passing - failing_by_build.get(build, set()) for build, passing in passing_by_build.items()):
                errors.append(f'{label}: verified requires paired passing evidence for every required scenario on the same Rust build')
    return errors


def completion_errors(document: dict, *, include_implementation: bool = False) -> list[str]:
    """Apply only after schema validation; unverified and partial are never passes."""
    scopes = {'vanilla', 'implementation'} if include_implementation else {'vanilla'}
    required = [feature for feature in document['features'] if feature['required'] and feature['scope'] in scopes]
    errors = [
        f'{feature["id"]}: required feature is {feature["rust_status"]}'
        for feature in required if feature['rust_status'] != 'verified'
    ]
    if errors or not required:
        return errors
    common_builds: set[str] | None = None
    for feature in required:
        coverage: dict[str, set[str]] = {}
        failures: dict[str, set[str]] = {}
        for record in feature['evidence']:
            if record['vanilla']['result'] == record['rust']['result'] == 'passed':
                coverage.setdefault(record['rust']['build_id'], set()).add(record['scenario'])
            else:
                failures.setdefault(record['rust']['build_id'], set()).add(record['scenario'])
        builds = {build for build, scenarios in coverage.items() if set(feature['required_scenarios']) <= scenarios - failures.get(build, set())}
        common_builds = builds if common_builds is None else common_builds & builds
    if not common_builds:
        errors.append('required features do not share a single verified Rust build')
    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('inventory', nargs='?', type=Path, default=ROOT / 'compatibility/features.json')
    gates = parser.add_mutually_exclusive_group()
    gates.add_argument('--strict', action='store_true', help='require every required vanilla feature to be verified')
    gates.add_argument('--strict-all', action='store_true', help='also require multicore and performance features to be verified')
    parser.add_argument('--check-artifacts', action='store_true', help='require all evidence files to exist below the repository root')
    parser.add_argument('--target', type=Path, default=ROOT / 'compatibility/target.json', help='canonical target pin to compare against')
    args = parser.parse_args(argv)
    try:
        document = json.loads(args.inventory.read_text())
        pinned = json.loads(args.target.read_text())
    except (OSError, ValueError) as error:
        print(f'inventory read failed: {error}', file=sys.stderr)
        return 2
    errors = validate_inventory(document, artifact_root=ROOT if args.check_artifacts or args.strict or args.strict_all else None)
    if isinstance(document, dict):
        errors.extend(validate_target(document, pinned))
    if errors:
        print('\n'.join(errors), file=sys.stderr)
        return 2
    if args.strict or args.strict_all:
        errors = completion_errors(document, include_implementation=args.strict_all)
        if errors:
            print('\n'.join(errors), file=sys.stderr)
            print(f'Completion gate failed: {len(errors)} required feature families remain.', file=sys.stderr)
            return 1
    counts = Counter(feature['rust_status'] for feature in document['features'])
    summary = ', '.join(f'{status}={counts[status]}' for status in sorted(STATUSES))
    print(f'{document["target"]["edition"]} {document["target"]["version"]}: {len(document["features"])} feature families; {summary}')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
