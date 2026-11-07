"""Stands in for horizon-bots on the VM: queues the command line for the laptop's relay worker and streams its stdout back.

Used as server_bench.py's `--bots-via "python3 relay_vm.py"`; the first argument (the bots binary name) is ignored by the worker.
"""
import json, sys, time
from pathlib import Path

JOBS = Path.home() / 'horizon-bench/relay/jobs'
job = str(time.time_ns())
tmp = JOBS / f'{job}.tmp'
tmp.write_text(json.dumps(sys.argv[2:]))
tmp.rename(JOBS / f'{job}.args')
out, done = JOBS / f'{job}.out', JOBS / f'{job}.done'
deadline = time.time() + 120
while not out.exists():
    if time.time() > deadline:
        sys.exit('relay: no laptop worker picked the job up within 120 s')
    time.sleep(0.5)
with out.open('rb') as stream:
    while True:
        chunk = stream.read()
        if chunk:
            sys.stdout.buffer.write(chunk)
            sys.stdout.flush()
        elif done.exists():
            sys.stdout.buffer.write(stream.read())
            sys.stdout.flush()
            sys.exit(int(done.read_text().strip() or 1))
        else:
            time.sleep(0.5)
