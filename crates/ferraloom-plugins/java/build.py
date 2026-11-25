#!/usr/bin/env python3
"""Builds Ferraloom's Bukkit bridge (docs/ferraloom/plugins.md) with the JDK alone.

    python3 crates/ferraloom-plugins/java/build.py                    # bridge, test plugins, coverage
    python3 crates/ferraloom-plugins/java/build.py --install SERVER   # and copies the bridge into SERVER/ferraloom-bukkit/
    python3 crates/ferraloom-plugins/java/build.py --usage JARS...    # and what those plugin jars (or folders) need: usage.md

1. Fetches the Spigot API jar pinned in api.lock.json (checked by SHA-256, never committed).
2. Generates the stubs: tools/StubGen.java over the interfaces in stubs.txt.
3. Compiles bridge/ against the API into dist/ferraloom-bukkit.jar, beside the API jar.
4. Runs the bridge's own checks (tests/, TESTS below).
5. Compiles the test plugins into test-plugins/.
6. Prints tools/Coverage.java's table (also in coverage.md).
7. With --usage, writes tools/Usage.java's report on the plugins given to usage.md (docs/ferraloom/plugin-usage.md).

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
    # Server.
    'dev.ferraloom.bukkit.FerraloomOfflinePlayer',
    'dev.ferraloom.bukkit.FerraloomProfile',
    'dev.ferraloom.bukkit.FerraloomProfileBans',
    'dev.ferraloom.bukkit.FerraloomIpBans',
    'dev.ferraloom.bukkit.FerraloomHelpMap',
    'dev.ferraloom.bukkit.FerraloomBossBar',
    # Blocks and worlds.
    'dev.ferraloom.bukkit.FerraloomChunk',
    'dev.ferraloom.bukkit.FerraloomWorldBorder',
    'dev.ferraloom.bukkit.FerraloomBlockState',
    # Inventory.
    'dev.ferraloom.bukkit.FerraloomItemFactory',
    'dev.ferraloom.bukkit.FerraloomItemMeta',
    'dev.ferraloom.bukkit.FerraloomInventory',
]
# The bridge's own checks in tests/ (a main that throws on failure), run after the bridge compiles.
TESTS = [
    # Inventory.
    'dev.ferraloom.bukkit.ItemsTest',
]
TEST_PLUGINS = {'FerraloomTest': HERE / 'test-plugin', 'FerraloomNms': HERE / 'nms-plugin'}
# Classes a test plugin compiles against but the server lacks, written at build time and kept out of its jar
# (a server internal's path may not be in the tree: tools/check_public_tree.py).
PROVIDED = {
    'FerraloomNms': {
        'net/minecraft/server/MinecraftServer.java':
            'package net.minecraft.server;\n'
            'public final class MinecraftServer { public static MinecraftServer getServer() { return null; } }\n',
    },
}


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

    # The bridge's checks.
    compile_java(f'{api}{sep}{out / "classes"}', out / 'test-classes', HERE / 'tests')
    for test in TESTS:
        run(tool('java'), '-cp', sep.join(map(str, [api, out / 'classes', out / 'test-classes'])), test)

    # The test plugins.
    for name, folder in TEST_PLUGINS.items():
        classes = out / 'plugin-classes' / name
        classpath = str(api)
        if name in PROVIDED:
            source = out / 'plugin-provided' / name
            shutil.rmtree(source, ignore_errors=True)
            for path, text in PROVIDED[name].items():
                (source / path).parent.mkdir(parents=True, exist_ok=True)
                (source / path).write_text(text)
            compile_java(api, out / 'plugin-classes' / f'{name}-provided', source)
            classpath += sep + str(out / 'plugin-classes' / f'{name}-provided')
        compile_java(classpath, classes, folder)
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
