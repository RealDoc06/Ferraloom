#!/usr/bin/env python3
"""Native worker-body schedule/output comparison. No Rust, ticks or listener."""
from pathlib import Path
import json
import gzip
import sys
import subprocess
import tempfile
import zipfile

HERE = Path(__file__).resolve().parent
CACHE = Path('/home/doc/code/Ferraloom/.cache')
VANILLA = CACHE / 'vanilla/26.3'
JDK = CACHE / 'toolchains/jdk-25.0.4.1+1/bin'
CLASSPATH = ':'.join(map(str, [VANILLA / 'versions/26.3/server-26.3.jar',
                             *sorted((VANILLA / 'libraries').rglob('*.jar')),
                             VANILLA / 'tree-harness/byte-buddy-1.15.4.jar']))
OUTPUT = CACHE / 'logs'
OUTPUT.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix='ferraloom-generation-schedule-java-') as temporary:
    root = Path(temporary)
    subprocess.run([str(JDK / 'javac'), '-cp', CLASSPATH, '-d', temporary,
                    str(HERE / 'GenerationScheduleTrace.java'),
                    str(HERE.parent / 'generation_light/PublishedLightTrace.java')], check=True)
    agent = root / 'schedule-agent.jar'
    with zipfile.ZipFile(agent, 'w') as archive:
        archive.writestr('META-INF/MANIFEST.MF',
                        'Manifest-Version: 1.0\nPremain-Class: GenerationScheduleTrace\n\n')
        for file in root.glob('*.class'):
            archive.write(file, file.name)
    outputs = []
    for variant in ['serial', 'ordered', 'permuted']:
        trace = OUTPUT / f'worldgen-schedule-{variant}.jsonl'
        log = OUTPUT / f'worldgen-schedule-{variant}.log'
        with log.open('w') as stream:
            subprocess.run([str(JDK / 'java'), '-Xmx2G', '-XX:ActiveProcessorCount=4',
                            '-Dnet.bytebuddy.experimental=true', f'-javaagent:{agent}',
                            '-cp', temporary + ':' + CLASSPATH, 'GenerationScheduleTrace',
                            str(trace), variant], check=True,
                           cwd=temporary, stdout=stream, stderr=subprocess.STDOUT, timeout=240)
        rows = [json.loads(line) for line in trace.read_text().splitlines()]
        summary = {}
        for status in ['BIOMES', 'TERRAIN', 'FEATURES']:
            spans = [row for row in rows if row['status'] == status]
            if not spans or any(row['failed'] for row in spans):
                raise RuntimeError(f'missing/failed native bodies: {variant} {status}: {log}')
            events = [(row['start_ns'], 1) for row in spans]
            events += [(row['end_ns'], -1) for row in spans]
            active = maximum = 0
            for _, delta in sorted(events):
                active += delta
                maximum = max(active, maximum)
            summary[status] = {'bodies': len(spans), 'max_overlap': maximum}
            if status == 'FEATURES' and maximum != 1:
                raise RuntimeError(f'native FEATURES overlapped: {trace}')
            if variant == 'ordered' and status != 'FEATURES' and maximum < 2:
                raise RuntimeError(f'no native worker overlap: {status}: {trace}')
        outputs.append(trace.with_name(trace.name + '.blocks').read_bytes())
        print(json.dumps({'variant': variant, 'summary': summary,
                          'trace': str(trace), 'log': str(log)}), flush=True)
    if outputs[0] != outputs[1]:
        raise RuntimeError('native serial/parallel computation changed fixed-order FEATURES output')
    if outputs[1] == outputs[2]:
        raise RuntimeError('native FEATURES order-dependence case no longer differs')
    terrain = [(OUTPUT / f'worldgen-schedule-{variant}.jsonl.terrain').read_bytes()
               for variant in ['serial', 'ordered', 'permuted']]
    if terrain[0] != terrain[1] or terrain[1] != terrain[2]:
        raise RuntimeError('native BIOMES/TERRAIN output depended on computation order')
    print(f'native serial/parallel TERRAIN agreement: {len(terrain[0])} bytes', flush=True)
    print(f'native fixed FEATURES order agreement: {len(outputs[0])} bytes; permuted order differs', flush=True)

    fixtures = HERE / 'fixtures'
    captures = {stage + '.nbt.gz': OUTPUT / ('worldgen-schedule-serial.jsonl.' + stage + '.nbt')
                for stage in ['bare-empty', 'bare-light-correct', 'generator-body', 'terrain', 'features', 'features-edited']}
    captures.update({
        'terrain.bin.gz': OUTPUT / 'worldgen-schedule-serial.jsonl.terrain',
        'features-request-order.bin.gz': OUTPUT / 'worldgen-schedule-ordered.jsonl.blocks',
        'features-permuted-order.bin.gz': OUTPUT / 'worldgen-schedule-permuted.jsonl.blocks',
    })
    if '--update-fixtures' in sys.argv:
        fixtures.mkdir(exist_ok=True)
        for name, source in captures.items():
            (fixtures / name).write_bytes(gzip.compress(source.read_bytes(), mtime=0))
    if '--check-fixtures' in sys.argv:
        for name, source in captures.items():
            if gzip.decompress((fixtures / name).read_bytes()) != source.read_bytes():
                raise RuntimeError(f'native fixture differs: {name}')
