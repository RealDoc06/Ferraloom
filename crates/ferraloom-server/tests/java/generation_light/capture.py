#!/usr/bin/env python3
"""Native-only trace, once per timing variant; no Rust or captured worlds."""
from pathlib import Path
import json
import os
import subprocess
import tempfile
import zipfile

HERE = Path(__file__).resolve().parent
CACHE = Path('/home/doc/code/Ferraloom/.cache')
JDK = CACHE / 'toolchains/jdk-25.0.4.1+1/bin'
VANILLA = CACHE / 'vanilla/26.3'
BUDDY = VANILLA / 'tree-harness/byte-buddy-1.15.4.jar'
if not BUDDY.exists():
    raise SystemExit(f'cached ByteBuddy missing: {BUDDY}')
CLASSPATH = ':'.join(map(str, [VANILLA / 'versions/26.3/server-26.3.jar',
                             *sorted((VANILLA / 'libraries').rglob('*.jar')), BUDDY]))
OUTPUT = CACHE / 'logs'
OUTPUT.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix='ferraloom-published-light-java-') as temporary:
    root = Path(temporary)
    subprocess.run([str(JDK / 'javac'), '-cp', CLASSPATH, '-d', temporary,
                    str(HERE / 'PublishedLightTrace.java')], check=True)
    agent = root / 'trace-agent.jar'
    with zipfile.ZipFile(agent, 'w') as archive:
        archive.writestr('META-INF/MANIFEST.MF',
                        'Manifest-Version: 1.0\nPremain-Class: PublishedLightTrace\nCan-Retransform-Classes: true\n\n')
        for file in root.glob('*.class'):
            archive.write(file, file.name)
    for name, delay in [('fresh', 0), ('delayed', 25)]:
        trace = OUTPUT / f'worldgen-published-light-{name}.jsonl'
        log = OUTPUT / f'worldgen-published-light-{name}.log'
        with log.open('w') as stream:
            subprocess.run([str(JDK / 'java'), '-Xmx2G',
                            '-Dnet.bytebuddy.experimental=true', f'-javaagent:{agent}',
                            '-cp', temporary + ':' + CLASSPATH, 'PublishedLightTrace',
                            str(trace), str(delay)], check=True, cwd=temporary,
                           stdout=stream, stderr=subprocess.STDOUT, timeout=240)
        reads = [json.loads(line) for line in trace.read_text().splitlines()
                 if json.loads(line)['kind'] == 'raw_read']
        if not reads:
            raise RuntimeError(f'no native light reads observed: {log}')
        print(json.dumps({'variant': name, 'light_delay_ms': delay, 'reads': reads,
                          'trace': str(trace), 'log': str(log)}), flush=True)
