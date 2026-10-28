#!/usr/bin/env python3
"""Native 26.3 cache/source replay; no Rust build or persistent worlds."""
from pathlib import Path
import subprocess
import sys
import tempfile

here = Path(__file__).resolve().parent
cache = Path('/home/doc/code/Ferraloom/.cache')
jdk = cache / 'toolchains/jdk-25.0.4.1+1/bin'
vanilla = cache / 'vanilla/26.3'
classpath = ':'.join(map(str, [vanilla / 'versions/26.3/server-26.3.jar',
                             *sorted((vanilla / 'libraries').rglob('*.jar'))]))
target = here.parents[2] / 'src/structure/testdata/template_manager.tsv'
with tempfile.TemporaryDirectory(prefix='ferraloom-template-manager-') as temporary:
    subprocess.run([str(jdk / 'javac'), '-cp', classpath, '-d', temporary,
                    str(here / 'TemplateManagerVectors.java')], check=True)
    output = subprocess.check_output([str(jdk / 'java'), '-Xmx2G', '-cp',
                                      temporary + ':' + classpath,
                                      'TemplateManagerVectors', temporary],
                                     cwd=temporary, text=True)
rows = [line.removeprefix('TEMPLATE_CACHE_VECTOR\t') for line in output.splitlines()
        if line.startswith('TEMPLATE_CACHE_VECTOR\t')]
if len(rows) != 20:
    raise RuntimeError(f'expected 20 native cache/source vectors, got {len(rows)}')
captured = '\n'.join(rows) + '\n'
if '--check' in sys.argv:
    if target.read_text() != captured:
        raise RuntimeError('native template manager vectors differ')
else:
    target.write_text(captured)
print(f'{len(rows)} native template manager cache/source vectors: {target}')
