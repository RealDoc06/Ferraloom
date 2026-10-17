"""Proposed post-run evidence preservation on the actual builder; never run a gate."""
import argparse
import datetime
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import stat
import subprocess
import tarfile

ROOT = Path('/home/doc/code/Ferraloom')
B = 'b58452a17d9a531a93810161321fe440c8149a1e'
PREFIX = 'review31-tick-integrated-builder-first'
HELPER = ROOT / '.cache/reviews/r31-tick-integrated-native-final-first/run-tick-integrated-all-sixteen.py'
HELPER_SHA = '45a7595eb9e85dc1f8a2512c28f8602579efe9bd4d067444f2e6733abfd46e9e'
DEST = ROOT / '.cache/reviews/r31-tick-integrated-native-first-export'


ROOT_EXECUTION_ENABLED = True
assert ROOT_EXECUTION_ENABLED, 'UNRUN preparation; ROOT must observe completed exit0 and bind execution'

def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def clean_source():
    assert subprocess.check_output(['git', '-C', str(ROOT), 'rev-parse', 'HEAD'], text=True, timeout=15).strip() == B
    assert not subprocess.check_output(['git', '-C', str(ROOT), 'status', '--porcelain=v1', '--untracked-files=all'], text=True, timeout=15)


def plain_file(path):
    assert path.is_absolute() and path.is_relative_to(ROOT) and path.is_file()
    assert not any(parent.is_symlink() for parent in (path, *path.parents)) and path.resolve(strict=True) == path


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--job-id', required=True, help='Actual completed builder job ID, supplied by ROOT')
args = parser.parse_args()
assert re.fullmatch(r'\d{8}T\d{6}Z-[0-9a-f]{8}', args.job_id)
assert socket.gethostname() == 'ferraloom-builder'
lock_path = Path('/home/doc/.cache/ferraloom-build/build.lock')
assert lock_path.is_file() and not lock_path.is_symlink()
lock = lock_path.open('r+')
fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
env = os.environ.copy()
env['XDG_RUNTIME_DIR'] = '/run/user/1000'
assert not subprocess.check_output(['systemctl', '--user', 'list-units', 'ferraloom-build-*.scope',
    '--state=active,activating,deactivating', '--no-legend', '--plain'], env=env, text=True, timeout=15).strip()
clean_source()
plain_file(HELPER)
assert sha(HELPER) == HELPER_SHA
native_path = ROOT / f'.cache/{PREFIX}-native.json'
outer_path = ROOT / f'.cache/remote-build-jobs/{args.job_id}.json'
for path in (native_path, outer_path):
    plain_file(path)
native, outer = json.loads(native_path.read_text()), json.loads(outer_path.read_text())
assert native['host'] == outer['host'] == 'ferraloom-builder' and native['source_commit'] == outer['source_commit'] == B
assert outer['command'] == ['python3', '-E', '-s', '-B', str(HELPER), B, PREFIX]
assert outer['exit_code'] == 0 and outer['source_status_after'] == '' and outer['cwd'] == str(ROOT)
assert outer['resource_limits'] == {'memory': '6G', 'swap': '0', 'cpu': '200%'}
assert Path(native['cgroup']).name == f'ferraloom-build-{args.job_id}.scope'
assert native['resource_limits'] == {'memory.max': str(6 * 1024**3), 'memory.swap.max': '0', 'cpu.max': '200000 100000'}
assert native['helper_sha256'] == HELPER_SHA and native['full_source_clean'] is True
assert native['outer_jar_sha256'] == 'd052f14d7a173734fba553711e5b570162e2f2a313267ee31a21b975a679be64'
assert type(native['outer_jar_bytes']) is int and native['outer_jar_bytes'] == 62294556
assert native['build_environment'] == {'CARGO_INCREMENTAL': '0', 'CARGO_BUILD_JOBS': '1'}
jar = ROOT / '.cache/vanilla/26.3/server.jar'
plain_file(jar)
assert jar.stat().st_size == native['outer_jar_bytes'] and sha(jar) == native['outer_jar_sha256']
steps = ['fmt-root', 'fmt-vendor', 'python', 'vendor-clippy', 'data', 'protocol', 'command', 'world',
         'inventory', 'core', 'gametest-compile', 'clippy', 'wrapper', 'node-protocol', 'typescript', 'build']
assert [row['step'] for row in native['steps']] == steps
assert all(type(row['exit_code']) is int and row['exit_code'] == 0 and row['execution_gate_passed'] is True for row in native['steps'])
names = [str(native_path.relative_to(ROOT)), str(outer_path.relative_to(ROOT))]
for row in native['steps']:
    path = ROOT / f'.cache/{PREFIX}-{row["step"]}.log'
    plain_file(path)
    assert row['log'] == str(path) and row['log_sha256'] == sha(path)
    names.append(str(path.relative_to(ROOT)))
artifact = native.get('preserved_generated_test_artifact')
if artifact:
    path = ROOT / f'.cache/{PREFIX}-generated-data/usercache.json'
    plain_file(path)
    assert artifact == {'path': str(path), 'sha256': sha(path)}
    names.append(str(path.relative_to(ROOT)))
assert len(names) == 19, 'Require native/outer, all16 logs and the preserved usercache'
binary = ROOT / 'target/debug/ferraloom'
plain_file(binary)
assert os.access(binary, os.X_OK) and re.fullmatch('[a-f0-9]{64}', native['binary_sha256'])
assert sha(binary) == native['binary_sha256']
needed = binary.stat().st_size + sum((ROOT / name).stat().st_size for name in names)
assert shutil.disk_usage(ROOT).free >= 2 * 1024**3 + needed + 1024**2
assert not DEST.exists() and not DEST.is_symlink()
DEST.mkdir()
preserved = ROOT / '.cache/validated-binaries' / B / 'ferraloom'
assert not any(parent.is_symlink() for parent in (preserved, *preserved.parents))
preserved.parent.mkdir(parents=True, exist_ok=True)
if not preserved.exists():
    temporary = DEST / 'binary-copy'
    shutil.copyfile(binary, temporary)
    temporary.chmod(0o555)
    assert sha(temporary) == native['binary_sha256']
    os.link(temporary, preserved)  # Refuse replacement, including a concurrent owner.
    temporary.unlink()
plain_file(preserved)
assert sha(binary) == sha(preserved) == native['binary_sha256'] and stat.S_IMODE(preserved.stat().st_mode) == 0o555
metadata = {'host': 'ferraloom-builder', 'source_commit': B, 'path': str(preserved), 'mode': '0555',
    'sha256': sha(preserved), 'bytes': preserved.stat().st_size, 'actual_job_id': args.job_id,
    'scope': 'Binary/evidence preservation only; independent full-log verification remains required.'}
with (DEST / 'binary-preservation.json').open('x') as stream:
    stream.write(json.dumps(metadata, indent=2) + '\n')
before = {name: {'bytes': (ROOT / name).stat().st_size, 'sha256': sha(ROOT / name)} for name in names}
with (DEST / 'raw.tar').open('xb') as output:
    with tarfile.open(fileobj=output, mode='w') as archive:
        for name in names:
            archive.add(ROOT / name, arcname=name, recursive=False)
assert before == {name: {'bytes': (ROOT / name).stat().st_size, 'sha256': sha(ROOT / name)} for name in names}
clean_source()
index = {'kind': 'original-native-evidence-export', 'host': 'ferraloom-builder', 'source_commit': B,
    'job_id': args.job_id, 'prefix': PREFIX, 'created_at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'archive_sha256': sha(DEST / 'raw.tar'), 'binary_metadata_sha256': sha(DEST / 'binary-preservation.json'),
    'files': before, 'new_validation_executed': False, 'native_runtime_admission_claimed': False}
with (DEST / 'export-index.json').open('x') as stream:
    stream.write(json.dumps(index, indent=2) + '\n')
for path in DEST.iterdir():
    path.chmod(0o444)
assert shutil.disk_usage(ROOT).free >= 2 * 1024**3
print(json.dumps(index, indent=2))
