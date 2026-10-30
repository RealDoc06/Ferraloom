"""Laptop side of the bots relay: claims jobs from the VM over outbound SSH, runs horizon-bots.exe, streams stdout back.

    python relay_worker.py <ssh key> <user@vm> <horizon-bots.exe> <log dir>
Exits when the VM's relay/stop file exists."""
import json, subprocess, sys, time
from pathlib import Path

key, host, exe, logs = sys.argv[1], sys.argv[2], sys.argv[3], Path(sys.argv[4])
logs.mkdir(parents=True, exist_ok=True)
ssh = ['ssh', '-i', key, '-o', 'BatchMode=yes', '-o', 'ServerAliveInterval=30', '-o', 'StrictHostKeyChecking=accept-new', host]
JOBS = '~/horizon-bench/relay/jobs'
while True:
    claim = subprocess.run([*ssh, 'python3 ~/horizon-bench/relay_next.py'], capture_output=True, text=True)
    lines = claim.stdout.splitlines()
    if claim.returncode or not lines:
        if claim.returncode:
            print(time.strftime('%H:%M:%S'), 'claim failed:', claim.stderr.strip(), flush=True)
            time.sleep(10)
        continue
    if lines[0] == 'STOP':
        print(time.strftime('%H:%M:%S'), 'stop requested', flush=True)
        break
    job, args = lines[0], json.loads(lines[1])
    print(time.strftime('%H:%M:%S'), 'job', job, args, flush=True)
    with (logs / f'{job}.stderr.txt').open('wb') as err:
        bots = subprocess.Popen([exe, *args], stdout=subprocess.PIPE, stderr=err)
        upload = subprocess.Popen([*ssh, f'cat > {JOBS}/{job}.out'], stdin=subprocess.PIPE)
        for line in bots.stdout:
            upload.stdin.write(line)
            upload.stdin.flush()
        code = bots.wait()
        upload.stdin.close()
        upload.wait()
    subprocess.run([*ssh, f'echo {code} > {JOBS}/{job}.done'], check=False)
    print(time.strftime('%H:%M:%S'), 'job', job, 'exit', code, flush=True)
