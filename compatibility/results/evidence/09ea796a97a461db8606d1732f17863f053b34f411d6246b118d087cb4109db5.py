"""Independently verify the original, closed fourth native run and retained binary.

Reads archived evidence only. It never runs a project module or a validation.
"""
import argparse
import ast
import copy
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import socket
import tarfile

ROOT = Path('/home/doc/code/Ferraloom')
E = 'a951d03e175831d5220fbd741ad488bfc9b6745e'
PREFIX = 'review29-all16-builder-fourth'
HELPER = ROOT / '.cache/reviews/r29-native-validation-prepare/run-review29-integrated-all-sixteen.py'
HELPER_SHA = '769cddec92e87f39a133ad9e1c55fa65ebd97b8c204269adc5dc559f4b1b2c1c'
DEST = ROOT / '.cache/reviews/r29-all16-builder-fourth-success'


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def unique(pairs):
    out = {}
    for key, value in pairs:
        assert key not in out, key
        out[key] = value
    return out


def json_bytes(raw):
    return json.loads(raw, object_pairs_hook=unique)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--job-id', required=True, help='Actual completed builder job ID, supplied by ROOT')
    parser.add_argument('--submission-json', required=True, type=Path)
    args = parser.parse_args()
    assert re.fullmatch(r'\d{8}T\d{6}Z-[0-9a-f]{8}', args.job_id), 'Actual job ID required'
    JOB = args.job_id
    submission_path = args.submission_json
    assert submission_path.is_absolute() and submission_path.is_file() and not submission_path.is_symlink()
    assert submission_path.resolve(strict=True) == submission_path
    assert submission_path.is_relative_to(ROOT / '.cache/builder-submissions') and submission_path.name == 'submission.json'
    helper = HELPER.read_bytes()
    assert sha(helper) == HELPER_SHA
    tree = ast.parse(helper)
    assignments = {node.targets[0].id: node.value for node in tree.body
                   if isinstance(node, ast.Assign) and len(node.targets) == 1
                   and isinstance(node.targets[0], ast.Name)}
    common = ast.literal_eval(assignments['common'])
    step_node = copy.deepcopy(assignments['steps'])
    for pair in step_node.elts:
        argv = pair.elts[1]
        expanded = []
        for value in argv.elts:
            if isinstance(value, ast.Starred):
                assert isinstance(value.value, ast.Name) and value.value.id == 'common'
                expanded.extend(ast.Constant(item) for item in common)
            else:
                expanded.append(value)
        argv.elts = expanded
    expected_steps = ast.literal_eval(step_node)
    expected_counts = ast.literal_eval(assignments['expected'])
    assert len(expected_steps) == 16 and sum(map(sum, expected_counts.values())) == 1637
    archive = (DEST / 'raw.tar').read_bytes()
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        members = tar.getmembers()
        assert all(member.isfile() for member in members)
        assert len({member.name for member in members}) == len(members)
        files = {member.name: tar.extractfile(member).read() for member in members}
    native_name = f'.cache/{PREFIX}-native.json'
    outer_name = f'.cache/remote-build-jobs/{JOB}.json'
    native, outer = json_bytes(files[native_name]), json_bytes(files[outer_name])
    submission_raw = submission_path.read_bytes()
    submission = json_bytes(submission_raw)
    assert submission['source_commit'] == E and submission['cwd'] == str(ROOT)
    assert submission['host'] == 'devbox' and submission['command'][:2] == ['/home/doc/.local/bin/ferraloom-build', '--']
    expected_command = ['timeout', '--signal=TERM', '--kill-after=30s', '7200',
                        'python3', '-B', str(HELPER.relative_to(ROOT)), E, PREFIX]
    assert outer['command'] == expected_command == submission['command'][2:]
    assert outer['host'] == native['host'] == 'ferraloom-builder'
    assert outer['source_commit'] == native['source_commit'] == E
    assert outer['cwd'] == str(ROOT) and outer['exit_code'] == 0 and outer['source_status_after'] == ''
    assert outer['resource_limits'] == {'memory': '6G', 'swap': '0', 'cpu': '200%'}
    assert native['resource_limits'] == {'memory.max': str(6 * 1024**3), 'memory.swap.max': '0', 'cpu.max': '200000 100000'}
    assert Path(native['cgroup']).name == f'ferraloom-build-{JOB}.scope'
    assert native['helper_sha256'] == HELPER_SHA and native['full_source_clean'] is True
    assert native['build_environment'] == {'CARGO_INCREMENTAL': '0', 'CARGO_BUILD_JOBS': '1'}
    assert len(native['steps']) == len(expected_steps)
    header_re = re.compile(r'^\s*Running .+ \(([^()]+)\)\s*$', re.M)
    summary_re = re.compile(r'test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;')
    actual_counts, isolated_summaries, required_names = {}, {}, {native_name, outer_name}
    for record, (name, command) in zip(native['steps'], expected_steps, strict=True):
        assert record['step'] == name and record['command'] == command
        assert record['exit_code'] == 0 and record['execution_gate_passed'] is True
        log_name = f'.cache/{PREFIX}-{name}.log'
        required_names.add(log_name)
        raw = files[log_name]
        assert record['log'] == str(ROOT / log_name) and record['log_sha256'] == sha(raw)
        text = raw.decode()
        if name in expected_counts:
            headers = list(header_re.finditer(text))
            assert len(headers) == len(expected_counts[name])
            observed = []
            for index, (header, count) in enumerate(zip(headers, expected_counts[name], strict=True)):
                segment = text[header.end():headers[index+1].start() if index+1 < len(headers) else len(text)]
                summaries = summary_re.findall(segment)
                assert summaries and summaries[-1] == ('ok', str(count), '0', '0', '0', '0')
                # Four inherited core controls re-execute this test binary to
                # isolate process limits/state. Their filtered child results
                # are retained separately, never added to the full-suite count.
                children = summaries[:-1]
                assert len(children) == (4 if name == 'core' else 0)
                assert all(row == ('ok', '1', '0', '0', '0', str(count-1)) for row in children)
                assert re.findall(r'^running (\d+) tests?$', segment, re.M) == [str(count), *(['1'] * len(children))]
                if children:
                    isolated_summaries[name] = children
                binary = PurePosixPath(header.group(1))
                assert '..' not in binary.parts and not binary.is_absolute()
                binary = PurePosixPath(str(ROOT)) / binary
                assert binary.parent == PurePosixPath(str(ROOT / 'target/debug/deps'))
                assert native['execution_checks'][name][index] == {'binary': str(binary), 'expected_passed': count}
                observed.append(count)
            actual_counts[name] = observed
        elif name == 'gametest-compile':
            assert not summary_re.search(text)
        elif name == 'python':
            assert re.search(r'Ran 128 tests in ', text) and re.search(r'^OK$', text, re.M)
        elif name == 'node-protocol':
            counts = {'tests': 70, 'suites': 0, 'pass': 70, 'fail': 0, 'cancelled': 0, 'skipped': 0, 'todo': 0}
            assert any(all(re.search('^'+re.escape(marker)+' '+field+' '+str(value)+'$', text, re.M)
                           for field, value in counts.items()) for marker in ('#', 'ℹ'))
    assert actual_counts == expected_counts and set(native['execution_checks']) == set(expected_counts)
    artifact = native.get('preserved_generated_test_artifact')
    if artifact:
        name = f'.cache/{PREFIX}-generated-data/usercache.json'
        required_names.add(name)
        assert artifact['path'] == str(ROOT / name) and artifact['sha256'] == sha(files[name])
        value = json_bytes(files[name])
        assert len(value) == 1 and value[0]['name'] == 'QueuedPlayer' and value[0]['uuid'] == '00000000-0000-0000-0000-000000000066'
    assert set(files) == required_names
    binary = json_bytes((DEST / 'binary-preservation.json').read_bytes())
    assert binary['host'] == 'ferraloom-builder' and binary['source_commit'] == E and binary['mode'] == '0555'
    assert binary['path'] == str(ROOT / '.cache/validated-binaries' / E / 'ferraloom')
    assert re.fullmatch('[a-f0-9]{64}', native['binary_sha256']) and binary['sha256'] == native['binary_sha256']
    result = {'kind': 'root-original-evidence-native-verification', 'result': 'all16-passed',
              'actual_host': 'ferraloom-builder', 'verification_host': socket.gethostname(),
              'engine_source': E, 'job_id': JOB, 'prefix': PREFIX, 'rust_tests': 1637,
              'submission_json': str(submission_path), 'submission_sha256': sha(submission_raw),
              'python_tests': 128, 'node_tests': 70, 'per_binary': actual_counts,
              'isolated_subprocess_summaries_not_added_to_total': isolated_summaries,
              'helper_sha256': HELPER_SHA, 'verifier_sha256': sha(Path(__file__).read_bytes()),
              'archive_sha256': sha(archive), 'native_sha256': sha(files[native_name]),
              'outer_sha256': sha(files[outer_name]), 'binary': binary,
              'full_source_clean': True, 'all_original_raw_log_hashes_checked': True,
              'files': {name: {'bytes': len(raw), 'sha256': sha(raw)} for name, raw in files.items()},
              'bounds': 'Native validation only. No paired runtime, REA capture or compatibility promotion.'}
    target = DEST / 'root-verification.json'
    with target.open('x') as stream:
        stream.write(json.dumps(result, indent=2)+'\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
