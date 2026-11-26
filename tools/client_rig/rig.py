#!/usr/bin/env python3
"""A headless vanilla 26.3 client rig, run on the builder (ferraloom-builder).

The unmodified vanilla client runs on an Xvfb display with Mesa's software GL and is driven with
xdotool; screenshots come from the X framebuffer. A Ferraloom server can run next to it. Every file
lives in ~/client-rig/. Mojang's files are read from the builder checkout's caches or downloaded
from Mojang's own endpoints (as every launcher does), checked against the version manifest's SHA-1.

Run it through tools/client_rig/rig from devbox, which pipes this file to the builder:

    rig setup                      libraries, assets, options.txt (once; repeat to repair)
    rig start [--join HOST:PORT]   display and client (straight into a server with --join)
    rig join HOST:PORT             restart the client straight into a server
    rig key KEY...                 xdotool key names: Escape, e, 1, F3+b ...
    rig hold KEY SECONDS           hold a key (w to walk, shift+w ...)
    rig type TEXT                  type text into the focused field
    rig chat TEXT                  open chat, type, send (a leading / sends a command)
    rig click [left|right] [--hold S] [--at X Y] [--shots NAME --count N --interval S]
    rig look DX DY                 turn: relative mouse motion in pixels
    rig xdo ARGS...                one raw xdotool chain (timed sequences: click 1 sleep 0.1 click 1)
    rig shot NAME [--hold KEY] [--count N --interval S]
                                   screenshots to ~/client-rig/shots/NAME[-NN].png
    rig log [N]                    last N lines of the client log ([CHAT] lines are chat)
    rig server start [--fresh] [--flat] [--plugins DIR] [--port P]
    rig server cmd LINE...         one console command
    rig server log [N]
    rig server stop
    rig status
    rig stop [client|server|display] all three by default
"""
import argparse
import concurrent.futures
import hashlib
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
import urllib.request
import uuid
from pathlib import Path

HOME = Path.home()
RIG = Path(os.environ.get('RIG_DIR', HOME / 'client-rig'))
TREE = Path(os.environ.get('RIG_TREE', HOME / 'code/Ferraloom'))  # read-only: its caches only
CACHE = RIG / 'cache'
GAME = RIG / 'game'
SHOTS = RIG / 'shots'
RUN = RIG / 'run'
BIN = RIG / 'server'  # horizon-client and horizon-server, built by ferraloom-build
WORLD = RIG / 'world-server'
RESOURCES = RIG / 'resources'
JDK = TREE / '.cache/toolchains/jdk-25.0.4.1+1'
MANIFESTS = [TREE / '.local/minecraft/26.3/manifest.json', TREE / '.cache/vanilla/26.3/version.json']
LOCAL_CLIENT = TREE / '.local/minecraft/26.3/jars/client.jar'
LOCAL_LIBRARIES = [TREE / '.local/minecraft/26.3/libraries/client', TREE / '.cache/vanilla/26.3/libraries']
LOCAL_RESOURCES = TREE / '.local/horizon-client/26.3'
DISPLAY = os.environ.get('RIG_DISPLAY', ':47')
WIDTH, HEIGHT = 1280, 720
PORT = 25571
NAME = 'RigPlayer'

# Fixed for deterministic screens: a GUI scale of 2 at 1280x720, little to render, nothing that pops up.
OPTIONS = {
    'renderDistance': '4', 'simulationDistance': '5', 'guiScale': '2', 'maxFps': '30',
    'enableVsync': 'false', 'fullscreen': 'false', 'pauseOnLostFocus': 'false', 'rawMouseInput': 'false',
    'tutorialStep': 'none', 'narrator': '0', 'attackIndicator': '1', 'joinedFirstServer': 'true',
    'skipMultiplayerWarning': 'true', 'onboardAccessibility': 'false', 'onboardingAccessibilityFinished': 'true',
    'realmsNotifications': 'false', 'telemetryOptInExtra': 'false', 'darkMojangStudiosBackground': 'true',
    'menuBackgroundBlurriness': '0', 'preferredGraphicsBackend': '"opengl"', 'hideServerAddress': 'false',
    'musicToast': '"never"', 'chatLinksPrompt': 'false', 'lang': 'en_us', 'mouseSensitivity': '0.5',
    'renderClouds': '"false"', 'particles': '2', 'entityShadows': 'false', 'biomeBlendRadius': '0',
    'soundCategory_master': '0.0', 'soundCategory_music': '0.0',
}


def say(*parts):
    print(*parts, flush=True)


def sha1(path):
    digest = hashlib.sha1()
    with open(path, 'rb') as f:
        for block in iter(lambda: f.read(1 << 20), b''):
            digest.update(block)
    return digest.hexdigest()


def fetch(url, path, checksum, local=()):
    """A verified file: already in place, copied from a local cache, or downloaded."""
    if path.exists() and sha1(path) == checksum:
        return path
    path.parent.mkdir(parents=True, exist_ok=True)
    partial = path.with_name(path.name + '.part')
    source = next((p for p in local if p.exists() and sha1(p) == checksum), None)
    if source:
        shutil.copyfile(source, partial)
    else:
        for attempt in range(3):
            try:
                with urllib.request.urlopen(url, timeout=60) as response, open(partial, 'wb') as out:
                    shutil.copyfileobj(response, out)
                break
            except OSError:
                if attempt == 2:
                    raise
                time.sleep(1 + attempt)
    if sha1(partial) != checksum:
        partial.unlink()
        raise SystemExit(f'checksum mismatch for {url}')
    partial.replace(path)
    return path


def manifest():
    path = next((p for p in MANIFESTS if p.exists()), None)
    if path is None:
        raise SystemExit(f'no 26.3 version manifest in {MANIFESTS}')
    return json.loads(path.read_text())


def libraries(m):
    """The manifest's libraries for x86-64 Linux, as (artifact, relative path)."""
    for library in m['libraries']:
        rules = library.get('rules', [])
        if rules and not any(r['action'] == 'allow' and r.get('os', {}).get('name') in (None, 'linux') for r in rules):
            continue
        if any(arm in library['name'] for arm in ('aarch_64', 'arm64')):
            continue
        artifact = library['downloads']['artifact']
        yield artifact, artifact['path']


def classpath(m):
    jars = [CACHE / 'libraries' / path for _, path in libraries(m)]
    return ':'.join(map(str, jars + [CACHE / 'client.jar']))


def setup(_args):
    m = manifest()
    client = m['downloads']['client']
    fetch(client['url'], CACHE / 'client.jar', client['sha1'], [LOCAL_CLIENT])
    libs = list(libraries(m))
    for artifact, path in libs:
        fetch(artifact['url'], CACHE / 'libraries' / path, artifact['sha1'], [d / path for d in LOCAL_LIBRARIES])
    say(f'client and {len(libs)} libraries ready')
    info = m['assetIndex']
    index = fetch(info['url'], CACHE / 'assets/indexes' / f"{info['id']}.json", info['sha1'])
    objects = {o['hash'] for o in json.loads(index.read_text())['objects'].values()}

    def get(h):
        fetch(f'https://resources.download.minecraft.net/{h[:2]}/{h}', CACHE / 'assets/objects' / h[:2] / h, h)

    with concurrent.futures.ThreadPoolExecutor(16) as pool:
        list(pool.map(get, sorted(objects)))
    say(f'{len(objects)} asset objects ready')
    if LOCAL_RESOURCES.exists() and not RESOURCES.exists():
        shutil.copytree(LOCAL_RESOURCES, RESOURCES, symlinks=True)
        say(f'prepared server resources copied to {RESOURCES}')
    write_options()
    for d in (SHOTS, RUN):
        d.mkdir(parents=True, exist_ok=True)


def write_options():
    """Merge the rig's fixed options into the client's own options.txt."""
    GAME.mkdir(parents=True, exist_ok=True)
    path = GAME / 'options.txt'
    current = {}
    if path.exists():
        for line in path.read_text().splitlines():
            key, sep, value = line.partition(':')
            if sep:
                current[key] = value
    current.update(OPTIONS)
    path.write_text(''.join(f'{k}:{v}\n' for k, v in current.items()))


# Processes: each runs detached in its own session, with a pid file in RUN.

def alive(name):
    pid_file = RUN / f'{name}.pid'
    if not pid_file.exists():
        return None
    pid = int(pid_file.read_text())
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        pid_file.unlink()
        return None
    return pid


def spawn(name, command, cwd, env=None, stdin=subprocess.DEVNULL):
    RUN.mkdir(parents=True, exist_ok=True)
    log = open(RUN / f'{name}.out', 'ab')
    process = subprocess.Popen(['nice', '-n', '10', *command], cwd=cwd, env=env, stdin=stdin, stdout=log,
                               stderr=subprocess.STDOUT, start_new_session=True)
    (RUN / f'{name}.pid').write_text(str(process.pid))
    return process


def kill(name, grace=10.0):
    pid = alive(name)
    if pid is None:
        return
    os.killpg(pid, signal.SIGTERM)
    deadline = time.time() + grace
    while time.time() < deadline and alive(name):
        time.sleep(0.2)
    if alive(name):
        os.killpg(pid, signal.SIGKILL)
    (RUN / f'{name}.pid').unlink(missing_ok=True)
    say(f'{name} stopped')


def x_env():
    return dict(os.environ, DISPLAY=DISPLAY)


def start_display():
    if alive('xvfb'):
        return
    spawn('xvfb', ['Xvfb', DISPLAY, '-screen', '0', f'{WIDTH}x{HEIGHT}x24', '-nolisten', 'tcp'], RIG)
    for _ in range(50):
        if subprocess.run(['xdpyinfo'], env=x_env(), capture_output=True).returncode == 0:
            return
        time.sleep(0.1)
    raise SystemExit(f'Xvfb did not come up on {DISPLAY}; see {RUN / "xvfb.out"}')


def offline_uuid(name):
    digest = bytearray(hashlib.md5(f'OfflinePlayer:{name}'.encode()).digest())
    digest[6] = (digest[6] & 0x0F) | 0x30
    digest[8] = (digest[8] & 0x3F) | 0x80
    return str(uuid.UUID(bytes=bytes(digest)))


def start_client(join, name):
    if alive('client'):
        raise SystemExit('client already running; `rig stop client` first, or `rig join`')
    m = manifest()
    if not (CACHE / 'client.jar').exists():
        raise SystemExit('run `rig setup` first')
    write_options()
    natives = RUN / 'natives'
    for sub in ('java', 'jna', 'lwjgl', 'netty'):
        (natives / sub).mkdir(parents=True, exist_ok=True)
    command = [
        str(JDK / 'bin/java'), '-Xms1G', '-Xmx3G', '-XX:StackShadowPages=32', '--enable-native-access=ALL-UNNAMED',
        '--add-exports', 'java.base/jdk.internal.misc=ALL-UNNAMED', f'-Djava.library.path={natives}/java',
        f'-Djna.tmpdir={natives}/jna', f'-Dorg.lwjgl.system.SharedLibraryExtractPath={natives}/lwjgl',
        f'-Dio.netty.native.workdir={natives}/netty', '-Dminecraft.launcher.brand=client-rig',
        '-cp', classpath(m), m['mainClass'],
        '--username', name, '--version', m['id'], '--gameDir', str(GAME), '--assetsDir', str(CACHE / 'assets'),
        '--assetIndex', m['assetIndex']['id'], '--uuid', offline_uuid(name), '--accessToken', '0',
        '--clientId', '0', '--xuid', '0', '--versionType', 'release', '--width', str(WIDTH), '--height', str(HEIGHT),
    ]
    if join:
        command += ['--quickPlayPath', str(RUN / 'quickplay.json'), '--quickPlayMultiplayer', join]
    env = dict(x_env(), LIBGL_ALWAYS_SOFTWARE='1', SDL_AUDIODRIVER='dummy', ALSOFT_DRIVERS='null')
    (RUN / 'client.out').unlink(missing_ok=True)
    spawn('client', command, GAME, env)
    say(f'client starting on {DISPLAY}' + (f', joining {join}' if join else ''))


def start(args):
    start_display()
    start_client(args.join, args.name)
    if args.wait:
        wait_for_client(args.join)


def join(args):
    kill('client')
    start_display()
    start_client(args.address, args.name)
    if args.wait:
        wait_for_client(args.address)


def wait_for_client(joining, timeout=300):
    """Until the client window is up and, when joining, the server has taken it into play."""
    log = GAME / 'logs/latest.log'
    started = time.time()
    while time.time() - started < timeout:
        if not alive('client'):
            raise SystemExit(f'client exited; see {RUN / "client.out"}')
        text = log.read_text(errors='replace') if log.exists() and log.stat().st_mtime >= started else ''
        # The client logs its advancements as the play phase starts, on any server.
        if window() is not None and (not joining or re.search(r'Loaded \d+ advancements', text)):
            break
        time.sleep(1)
    else:
        raise SystemExit('client did not come up in time')
    say(f'client up after {time.time() - started:.0f} s')


def window():
    found = subprocess.run(['xdotool', 'search', '--class', 'com.mojang.minecraft'], env=x_env(), capture_output=True, text=True)
    ids = found.stdout.split()
    return ids[-1] if ids else None


def xdo(*argv):
    subprocess.run(['xdotool', *argv], env=x_env(), check=True)


def focus():
    win = window()
    if win is None:
        raise SystemExit('no client window')
    subprocess.run(['xdotool', 'windowfocus', '--sync', win], env=x_env(), capture_output=True)


def key(args):
    focus()
    for k in args.keys:
        xdo('key', '--delay', '60', k)
        time.sleep(0.1)


def hold(args):
    focus()
    keys = args.key.split('+')
    for k in keys:
        xdo('keydown', k)
    time.sleep(args.seconds)
    for k in reversed(keys):
        xdo('keyup', k)


def type_text(args):
    focus()
    xdo('type', '--delay', '40', args.text)


def chat(args):
    focus()
    xdo('key', 't')
    time.sleep(0.4)
    xdo('type', '--delay', '40', args.text)
    time.sleep(0.2)
    xdo('key', 'Return')


def click(args):
    focus()
    button = {'left': '1', 'right': '3', 'middle': '2'}[args.button]
    if args.at:
        xdo('mousemove', str(args.at[0]), str(args.at[1]))
        time.sleep(0.1)
    xdo('mousedown', button)
    time.sleep(args.hold)
    xdo('mouseup', button)
    if args.shots:
        capture(args.shots, args.count, args.interval)


def look(args):
    """Relative motion in small steps, so the game reads every one."""
    focus()
    steps = max(1, int(max(abs(args.dx), abs(args.dy)) // 20))
    for i in range(steps):
        dx = args.dx * (i + 1) // steps - args.dx * i // steps
        dy = args.dy * (i + 1) // steps - args.dy * i // steps
        xdo('mousemove_relative', '--', str(dx), str(dy))
        time.sleep(0.03)


def capture(name, count=1, interval=0.0):
    """Screenshots as fast as the display allows (raw first, PNG after); prints `shot: PATH` for each."""
    SHOTS.mkdir(parents=True, exist_ok=True)
    raw = []
    started = time.time()
    for i in range(count):
        raw.append((SHOTS / f'{name}.ppm' if count == 1 else SHOTS / f'{name}-{i:02d}.ppm', time.time() - started))
        subprocess.run(['import', '-window', 'root', str(raw[-1][0])], env=x_env(), check=True)
        time.sleep(interval)
    for path, at in raw:
        png = path.with_suffix('.png')
        subprocess.run(['convert', str(path), str(png)], check=True)
        path.unlink()
        say(f'shot: {png}' + (f'  (+{at:.2f} s)' if count > 1 else ''))


def raw(args):
    """One xdotool chain, for timing a sequence closely: `xdo click 1 sleep 0.1 click 1`."""
    focus()
    xdo(*args.argv)


def shot(args):
    """With --hold KEY, the key is held while the shot is taken (Tab for the player list)."""
    if args.hold:
        focus()
        xdo('keydown', args.hold)
        time.sleep(0.5)
    try:
        capture(args.name, args.count, args.interval)
    finally:
        if args.hold:
            xdo('keyup', args.hold)


def tail(path, n):
    if not path.exists():
        raise SystemExit(f'no {path}')
    say('\n'.join(path.read_text(errors='replace').splitlines()[-n:]))


def client_log(args):
    tail(GAME / 'logs/latest.log', args.n)


# The server: Ferraloom's engine run directly, its console read from a FIFO.

PROPERTIES = {
    'online-mode': 'false', 'white-list': 'false', 'enforce-secure-profile': 'false', 'spawn-protection': '0',
    'view-distance': '6', 'simulation-distance': '6', 'pause-when-empty-seconds': '-1', 'difficulty': 'easy',
    'motd': 'Ferraloom client rig', 'server-ip': '127.0.0.1',
}


def server_start(args):
    if alive('server'):
        raise SystemExit('server already running')
    engine = BIN / 'horizon-client'
    if not engine.exists():
        raise SystemExit(f'no engine in {BIN}; build it with ferraloom-build')
    if not RESOURCES.exists():
        raise SystemExit('no prepared resources; run `rig setup`')
    if args.fresh:
        shutil.rmtree(WORLD, ignore_errors=True)
    WORLD.mkdir(parents=True, exist_ok=True)
    props = dict(PROPERTIES, **{'server-port': str(args.port)})
    if args.flat:
        props.update({'level-type': 'minecraft\\:flat', 'generate-structures': 'false'})
    path = WORLD / 'server.properties'
    if path.exists():  # keep what the game wrote (the seed); the rig's keys win
        for line in path.read_text(encoding='iso-8859-1').splitlines():
            key, sep, value = line.partition('=')
            if sep and not line.startswith('#'):
                props.setdefault(key, value)
    path.write_text(''.join(f'{k}={v}\n' for k, v in props.items()), encoding='iso-8859-1')
    (WORLD / 'eula.txt').write_text('eula=true\n')
    plugins = WORLD / 'plugins'
    shutil.rmtree(plugins, ignore_errors=True)
    if args.plugins:
        source = Path(args.plugins).expanduser()
        plugins.mkdir()
        for jar in source.glob('*.jar'):
            shutil.copy2(jar, plugins / jar.name)
        bridge = source / 'ferraloom-bukkit'
        shutil.rmtree(WORLD / 'ferraloom-bukkit', ignore_errors=True)
        if bridge.exists():
            shutil.copytree(bridge, WORLD / 'ferraloom-bukkit')
    fifo = RUN / 'server.console'
    RUN.mkdir(parents=True, exist_ok=True)
    if not fifo.exists():
        os.mkfifo(fifo)
    scratch = RIG / 'server-home'
    env = dict(os.environ, HORIZON_HOME=str(scratch / 'home'), XDG_DATA_HOME=str(scratch / 'data'),
               XDG_CONFIG_HOME=str(scratch / 'config'), HORIZON_NO_AUDIO='1', HORIZON_LAUNCHER_NO_SELF_UPDATE='1',
               JAVA_HOME=str(JDK))
    (RUN / 'server.out').unlink(missing_ok=True)
    console = os.open(fifo, os.O_RDWR)  # read-write: the server never sees EOF between commands
    spawn('server', [str(engine), '--resources', str(RESOURCES), '--dedicated', '--nogui'], WORLD, env, console)
    started = time.time()
    while time.time() - started < 300:
        text = (RUN / 'server.out').read_text(errors='replace') if (RUN / 'server.out').exists() else ''
        if 'Done (' in text:
            say(f'server up on 127.0.0.1:{args.port} after {time.time() - started:.0f} s')
            return
        if not alive('server'):
            raise SystemExit(f'server exited; see {RUN / "server.out"}')
        time.sleep(1)
    raise SystemExit('server did not finish starting in 300 s')


def server_cmd(args):
    if not alive('server'):
        raise SystemExit('server not running')
    mark = (RUN / 'server.out').stat().st_size
    fd = os.open(RUN / 'server.console', os.O_WRONLY | os.O_NONBLOCK)
    os.write(fd, (' '.join(args.line) + '\n').encode())
    os.close(fd)
    time.sleep(1.0)
    with open(RUN / 'server.out', 'rb') as f:
        f.seek(mark)
        sys.stdout.write(f.read().decode(errors='replace'))


def server_stop(_args=None):
    if not alive('server'):
        return
    fd = os.open(RUN / 'server.console', os.O_WRONLY | os.O_NONBLOCK)
    os.write(fd, b'stop\n')
    os.close(fd)
    deadline = time.time() + 60
    while time.time() < deadline and alive('server'):
        time.sleep(0.5)
    kill('server')


def server_log(args):
    tail(RUN / 'server.out', args.n)


def status(_args):
    for name in ('xvfb', 'client', 'server'):
        pid = alive(name)
        say(f'{name}: ' + (f'running (pid {pid})' if pid else 'stopped'))


def stop(args):
    if args.what in ('client', 'all'):
        kill('client')
    if args.what in ('server', 'all'):
        server_stop()
    if args.what in ('display', 'all'):
        kill('xvfb')


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('setup').set_defaults(run=setup)
    p = sub.add_parser('start')
    p.add_argument('--join', metavar='HOST:PORT')
    p.add_argument('--name', default=NAME)
    p.add_argument('--no-wait', dest='wait', action='store_false')
    p.set_defaults(run=start)
    p = sub.add_parser('join')
    p.add_argument('address', nargs='?', default=f'127.0.0.1:{PORT}')
    p.add_argument('--name', default=NAME)
    p.add_argument('--no-wait', dest='wait', action='store_false')
    p.set_defaults(run=join)
    p = sub.add_parser('key')
    p.add_argument('keys', nargs='+')
    p.set_defaults(run=key)
    p = sub.add_parser('hold')
    p.add_argument('key')
    p.add_argument('seconds', type=float)
    p.set_defaults(run=hold)
    p = sub.add_parser('type')
    p.add_argument('text')
    p.set_defaults(run=type_text)
    p = sub.add_parser('chat')
    p.add_argument('text')
    p.set_defaults(run=chat)
    p = sub.add_parser('click')
    p.add_argument('button', nargs='?', default='left', choices=('left', 'right', 'middle'))
    p.add_argument('--hold', type=float, default=0.05)
    p.add_argument('--at', type=int, nargs=2, metavar=('X', 'Y'))
    p.add_argument('--shots', metavar='NAME', help='screenshots right after the release')
    p.add_argument('--count', type=int, default=1)
    p.add_argument('--interval', type=float, default=0.0)
    p.set_defaults(run=click)
    p = sub.add_parser('look')
    p.add_argument('dx', type=int)
    p.add_argument('dy', type=int)
    p.set_defaults(run=look)
    p = sub.add_parser('xdo')
    p.add_argument('argv', nargs=argparse.REMAINDER)
    p.set_defaults(run=raw)
    p = sub.add_parser('shot')
    p.add_argument('name')
    p.add_argument('--hold', metavar='KEY')
    p.add_argument('--count', type=int, default=1)
    p.add_argument('--interval', type=float, default=0.0)
    p.set_defaults(run=shot)
    p = sub.add_parser('log')
    p.add_argument('n', nargs='?', type=int, default=30)
    p.set_defaults(run=client_log)
    server = sub.add_parser('server').add_subparsers(dest='action', required=True)
    p = server.add_parser('start')
    p.add_argument('--fresh', action='store_true', help='delete the world first')
    p.add_argument('--flat', action='store_true')
    p.add_argument('--plugins', metavar='DIR', help='plugin jars (and ferraloom-bukkit/) on the builder')
    p.add_argument('--port', type=int, default=PORT)
    p.set_defaults(run=server_start)
    p = server.add_parser('cmd')
    p.add_argument('line', nargs='+')
    p.set_defaults(run=server_cmd)
    p = server.add_parser('log')
    p.add_argument('n', nargs='?', type=int, default=30)
    p.set_defaults(run=server_log)
    server.add_parser('stop').set_defaults(run=server_stop)
    sub.add_parser('status').set_defaults(run=status)
    p = sub.add_parser('stop')
    p.add_argument('what', nargs='?', default='all', choices=('all', 'client', 'server', 'display'))
    p.set_defaults(run=stop)
    args = parser.parse_args()
    args.run(args)


if __name__ == '__main__':
    main()
