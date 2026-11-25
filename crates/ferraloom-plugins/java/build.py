#!/usr/bin/env python3
"""Builds Ferraloom's Bukkit bridge (docs/ferraloom/plugins.md) with the JDK alone.

    python3 crates/ferraloom-plugins/java/build.py                    # bridge, test plugins, coverage
    python3 crates/ferraloom-plugins/java/build.py --install SERVER   # and copies the bridge into SERVER/ferraloom-bukkit/
    python3 crates/ferraloom-plugins/java/build.py --usage JARS...    # and what those plugin jars (or folders) need: usage.md

1. Fetches the Spigot API jar pinned in api.lock.json (checked by SHA-256, never committed).
2. Generates the stubs: tools/StubGen.java over the interfaces in stubs.txt.
3. Compiles bridge/ against the API into dist/ferraloom-bukkit.jar, beside the API jar.
4. Compiles the test plugins into test-plugins/.
5. Prints tools/Coverage.java's table (also in coverage.md).
6. With --usage, writes tools/Usage.java's report on the plugins given to usage.md (docs/ferraloom/plugin-usage.md).

Everything goes under --out (default .local/ferraloom-plugins/ in the checkout), the API jar in --cache. JAVA_HOME's javac, jar and java
are used when it is set, else those on PATH.
"""
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
RELEASE = '21'
# The bridge classes measured for coverage, each extending one generated stub.
IMPLEMENTATIONS = [
    'dev.ferraloom.bukkit.FerraloomServer',
    'dev.ferraloom.bukkit.FerraloomPlayer',
    'dev.ferraloom.bukkit.FerraloomWorld',
    'dev.ferraloom.bukkit.FerraloomBlock',
    'dev.ferraloom.bukkit.FerraloomConsole',
    'dev.ferraloom.bukkit.FerraloomScheduler',
    'dev.ferraloom.bukkit.FerraloomUnsafe',
    # Blocks and worlds.
    'dev.ferraloom.bukkit.FerraloomChunk',
    'dev.ferraloom.bukkit.FerraloomWorldBorder',
    'dev.ferraloom.bukkit.FerraloomBlockState',
    'dev.ferraloom.bukkit.FerraloomEntity',
    'dev.ferraloom.bukkit.FerraloomLivingEntity',
    'dev.ferraloom.bukkit.FerraloomMob',
]
TEST_PLUGINS = {'FerraloomTest': HERE / 'test-plugin', 'FerraloomNms': HERE / 'nms-plugin'}


def tool(name):
    home = os.environ.get('JAVA_HOME')
    path = Path(home) / 'bin' / name if home else shutil.which(name)
    if not path or not Path(path).exists():
        sys.exit(f'build.py: no {name} (set JAVA_HOME to a JDK 21 or newer)')
    return str(path)


def run(*command):
    subprocess.run([str(c) for c in command], check=True)


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fetch_api(cache):
    lock = json.loads((HERE / 'api.lock.json').read_text())
    jar = cache / lock['file']
    if not jar.exists() or sha256(jar) != lock['sha256']:
        cache.mkdir(parents=True, exist_ok=True)
        print(f'fetching {lock["artifact"]} ({lock["classifier"]})', flush=True)
        part = jar.with_suffix('.part')
        with urllib.request.urlopen(lock['url'], timeout=120) as response, part.open('wb') as out:
            shutil.copyfileobj(response, out)
        if sha256(part) != lock['sha256']:
            part.unlink()
            sys.exit(f'build.py: {lock["url"]} does not match api.lock.json\'s SHA-256; '
                     'if Spigot replaced the snapshot, pin the new build there')
        part.replace(jar)
    return jar


def sources(*folders):
    return sorted(str(p) for folder in folders for p in Path(folder).rglob('*.java'))


def compile_java(classpath, out, *folders):
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    run(tool('javac'), '--release', RELEASE, '-encoding', 'UTF-8', '-nowarn', '-proc:none', '-cp', classpath,
        '-d', out, *sources(*folders))


def make_jar(jar, *trees):
    jar.parent.mkdir(parents=True, exist_ok=True)
    jar.unlink(missing_ok=True)
    command = [tool('jar'), '--create', '--file', jar]
    for tree in trees:
        command += ['-C', tree, '.']
    run(*command)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--out', type=Path, default=ROOT / '.local/ferraloom-plugins')
    parser.add_argument('--cache', type=Path, default=ROOT / '.local/ferraloom-plugins/cache',
                        help='where the API jar is kept between builds')
    parser.add_argument('--install', type=Path, help='a server folder: copies the bridge into its ferraloom-bukkit/')
    parser.add_argument('--usage', type=Path, nargs='+', help='plugin jars or folders of them to report on')
    args = parser.parse_args()
    out = args.out.resolve()
    api = fetch_api(args.cache.resolve())
    sep = os.pathsep

    # The stubs, generated from the API jar.
    compile_java(api, out / 'tools', HERE / 'tools')
    shutil.rmtree(out / 'gen', ignore_errors=True)
    interfaces = [l.strip() for l in (HERE / 'stubs.txt').read_text().splitlines() if l.strip() and not l.startswith('#')]
    run(tool('java'), '-cp', f'{api}{sep}{out / "tools"}', 'StubGen', out / 'gen', *interfaces)

    # The bridge.
    compile_java(api, out / 'classes', out / 'gen', HERE / 'bridge')
    dist = out / 'dist'
    shutil.rmtree(dist, ignore_errors=True)
    make_jar(dist / 'ferraloom-bukkit.jar', out / 'classes')
    shutil.copy2(api, dist / api.name)

    # The test plugins.
    for name, folder in TEST_PLUGINS.items():
        classes = out / 'plugin-classes' / name
        compile_java(api, classes, folder)
        make_jar(out / 'test-plugins' / f'{name}.jar', classes, folder / 'resources')

    # Coverage.
    table = subprocess.run([tool('java'), '-cp', sep.join(map(str, [api, out / 'classes', out / 'tools'])), 'Coverage',
                            *IMPLEMENTATIONS], check=True, capture_output=True, text=True).stdout
    (out / 'coverage.md').write_text(table)
    print(table, end='')

    if args.usage:
        report = subprocess.run([tool('java'), '-cp', sep.join(map(str, [api, out / 'tools'])), 'Usage', api,
                                 dist / 'ferraloom-bukkit.jar', *args.usage], check=True, capture_output=True, text=True).stdout
        (out / 'usage.md').write_text(report)
        print(f'usage report: {out / "usage.md"}')

    if args.install:
        target = args.install.resolve() / 'ferraloom-bukkit'
        shutil.rmtree(target, ignore_errors=True)
        shutil.copytree(dist, target)
        print(f'installed into {target}')
    print(f'bridge: {dist}')


if __name__ == '__main__':
    main()
