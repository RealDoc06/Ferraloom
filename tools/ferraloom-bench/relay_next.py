"""Run by the laptop worker over SSH: waits up to 10 min for the oldest queued job, claims it and prints `<id>\n<json args>`.
Prints STOP when relay/stop exists, nothing on timeout."""
import time
from pathlib import Path

RELAY = Path.home() / 'horizon-bench/relay'
deadline = time.time() + 600
while time.time() < deadline:
    if (RELAY / 'stop').exists():
        print('STOP')
        break
    queued = sorted((RELAY / 'jobs').glob('*.args'))
    if queued:
        job = queued[0]
        claimed = job.with_suffix('.claimed')
        job.rename(claimed)
        print(job.stem)
        print(claimed.read_text())
        break
    time.sleep(0.5)
