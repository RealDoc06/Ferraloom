#!/usr/bin/env python3
"""Capture native copper ages/NEXT, and an independent inverse/state API dump."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
CACHE = Path('/home/doc/code/Ferraloom/.cache')
JAVA = CACHE / 'toolchains/jdk-25.0.4.1+1/bin'
VANILLA = CACHE / 'vanilla/26.3'
CP = os.pathsep.join(map(str, [VANILLA / 'versions/26.3/server-26.3.jar', *sorted((VANILLA / 'libraries').rglob('*.jar'))]))
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--check', action='store_true')
args = parser.parse_args()
with tempfile.TemporaryDirectory(dir=CACHE / 'tools', prefix='weathering-facts-') as temporary:
    temporary = Path(temporary)
    log_path = CACHE / 'logs/weathering-copper-facts-native.log'
    with log_path.open('w') as log:
        result = subprocess.run([str(JAVA / 'javac'), '-cp', CP, '-d', str(temporary), str(ROOT / 'tools/extractor/ExtractWeatheringCopper.java'), str(HERE / 'WeatheringCopperFacts.java')], stdout=log, stderr=subprocess.STDOUT)
        if result.returncode:
            raise RuntimeError(log_path.read_text()[-8000:])
        cp = str(temporary) + os.pathsep + CP
        report = temporary / 'weathering.json'
        result = subprocess.run([str(JAVA / 'java'), '-Xmx512M', '-cp', cp, 'ExtractWeatheringCopper', str(report)], cwd=temporary, stdout=log, stderr=subprocess.STDOUT, timeout=120)
        if result.returncode:
            raise RuntimeError(log_path.read_text()[-8000:])
        result = subprocess.run([str(JAVA / 'java'), '-Xmx512M', '-cp', cp, 'WeatheringCopperFacts'], cwd=temporary, stdout=subprocess.PIPE, stderr=log, text=True, timeout=120)
        if result.returncode:
            log.write(result.stdout)
            log.flush()
            raise RuntimeError(log_path.read_text()[-8000:])
    lines = [line.removeprefix('WEATHERING\t') for line in result.stdout.splitlines() if line.startswith('WEATHERING\t')]
    facts = json.loads(report.read_text())
    assert len(facts['blocks']) == 60
    assert sum(row['next'] is not None for row in facts['blocks']) == 45
    assert sum(line.startswith('block\t') for line in lines) == facts['block_count']
    targets = {ROOT / 'data/generated/weathering_copper.json': report.read_text(), ROOT / 'crates/ferraloom-data/tests/fixtures/weathering_copper/native.tsv': '\n'.join(lines) + '\n'}
    for target, text in targets.items():
        if args.check:
            assert target.read_text() == text, f'native weathering facts differ: {target}'
        else:
            target.write_text(text)
    print(f'{len(lines)} native weathering registry/state facts; 60 ages and 45 NEXT edges')
