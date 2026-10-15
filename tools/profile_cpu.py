#!/usr/bin/env python3
"""Sample an explicitly selected Linux server process without attaching a debugger.

CPU seconds divided by elapsed wall seconds measures cores consumed. This shows
concurrent CPU use, not throughput, tick latency, or a speedup over vanilla.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import sys
import time


def counters(path: Path) -> tuple[int, int, str]:
    raw = path.read_text()
    end = raw.rindex(')')
    fields = raw[end + 2:].split()
    return int(fields[11]) + int(fields[12]), int(fields[19]), raw[raw.index('(') + 1:end]


def snapshot(process: Path) -> tuple[int, int, dict[int, tuple[int, str]]]:
    ticks, started, _ = counters(process / 'stat')
    threads = {}
    for task in (process / 'task').iterdir():
        try:
            count, _, name = counters(task / 'stat')
            threads[int(task.name)] = count, name
        except FileNotFoundError:
            continue  # A worker may exit between listing and reading its counter.
    return ticks, started, threads


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--pid', type=int, required=True)
    parser.add_argument('--duration', type=float, default=15)
    parser.add_argument('--interval', type=float, default=1)
    parser.add_argument('--workload', required=True, help='Describe the actual controlled workload')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if sys.platform != 'linux' or args.pid < 1 or not 0 < args.interval <= args.duration <= 300:
        parser.error('Requires Linux, a positive PID, and 0 < interval <= duration <= 300 seconds')
    process = Path('/proc') / str(args.pid)
    executable = process / 'exe'
    binary_sha256 = hashlib.file_digest(executable.open('rb'), 'sha256').hexdigest()
    previous_ticks, identity, previous_threads = snapshot(process)
    first_ticks = previous_ticks
    began = previous_at = time.monotonic()
    samples = []
    thread_totals: dict[int, dict] = {}
    exited = False
    while previous_at - began < args.duration:
        time.sleep(min(args.interval, args.duration - (previous_at - began)))
        now = time.monotonic()
        try:
            ticks, started, threads = snapshot(process)
        except FileNotFoundError:
            exited = True
            break
        if started != identity:
            raise RuntimeError('PID was reused during measurement')
        wall = now - previous_at
        frequency = os.sysconf('SC_CLK_TCK')
        cores = (ticks - previous_ticks) / frequency / wall
        samples.append({'elapsed_seconds': round(now - began, 3), 'cores_consumed': round(cores, 3)})
        for tid, (count, name) in threads.items():
            if tid in previous_threads:
                entry = thread_totals.setdefault(tid, {'name': name, 'cpu_seconds': 0.0})
                entry['cpu_seconds'] += (count - previous_threads[tid][0]) / frequency
        previous_ticks, previous_at, previous_threads = ticks, now, threads
    if not samples:
        raise RuntimeError('Process exited before a complete sample')
    elapsed = previous_at - began
    cpu_seconds = (previous_ticks - first_ticks) / os.sysconf('SC_CLK_TCK')
    report = {
        'schema_version': 1, 'created_at': datetime.now(timezone.utc).isoformat(),
        'platform': sys.platform, 'host': os.uname().nodename, 'logical_cpus': os.cpu_count(),
        'rust_binary_sha256': binary_sha256, 'workload': args.workload,
        'scope': 'Observed process CPU use only; no vanilla speedup, tick latency or determinism claim.',
        'duration_seconds': round(elapsed, 3), 'cpu_seconds': round(cpu_seconds, 3),
        'average_cores_consumed': round(cpu_seconds / elapsed, 3),
        'peak_interval_cores_consumed': max(sample['cores_consumed'] for sample in samples),
        'process_exited': exited, 'samples': samples,
        'threads': sorted(thread_totals.values(), key=lambda entry: entry['cpu_seconds'], reverse=True),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(f"Observed {report['average_cores_consumed']} cores on average; peak interval {report['peak_interval_cores_consumed']}. Report: {args.output}")
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
