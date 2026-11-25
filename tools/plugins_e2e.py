#!/usr/bin/env python3
"""Ferraloom's Bukkit plugin bridge end to end (docs/ferraloom/plugins.md).

Builds the bridge and the test plugins (crates/ferraloom-plugins/java/build.py), then runs the dedicated
server twice on a fresh flat world:

1. without a plugins folder: the server starts, and no JVM is mapped into it (/proc/<pid>/maps);
2. with plugins/FerraloomTest.jar and FerraloomNms.jar (which names server internals in an adapter: it loads
   with a warning, and is disabled in one line when a command reaches the adapter; FerraloomTest runs on):
   one scripted player (crates/ferraloom-plugins/examples/plugin_bot.rs) joins, and the checks read the
   server's log and what the player is sent: lifecycle order, config and data folder, Bukkit.getServer()'s
   basics, the scheduler's ticks (a 40-tick delay, a 10-tick timer, an async task), the join and quit
   events and messages, the command tree, a command's reply from a player and from the console, a stub's
   UnsupportedOperationException, chat format and cancel with listener priorities and ignoreCancelled,
   a cancelled block break leaving the block (and resending it) and an uncancelled one breaking it,
   getLocation and teleport; the world load events, block reads (type, data, light, biome) and writes,
   BlockPhysicsEvent, a cancelled and an uncancelled block place, world reads (heights, chunks, border,
   game rules, another dimension) and a storm with its WeatherChangeEvent; a chest GUI a command opens
   (its title, its items' names, lore and plugin data) with a cancelled click leaving both sides as they
   were and an uncancelled one taking the item, its close event, and a held item written through as
   CraftItemStack's mirror is; the server area: a command hidden from a player without its permission, tab
   completion (a TabCompleter, Bukkit's player-name default, TabCompleteEvent), a boss bar shown and removed,
   an offline player's op, whitelist and ban round trip (the ban seen by vanilla's /banlist), and the
   watchdog naming a plugin that stalls the server thread.

    python3 tools/plugins_e2e.py --engine target/release/horizon-client --bot target/release/examples/plugin_bot

Needs the prepared resources (.local/horizon-client/26.3) and a JDK (JAVA_HOME). Logs go to --out.
"""
import argparse
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RESOURCES = ROOT / '.local/horizon-client/26.3'
BUILD = ROOT / 'crates/ferraloom-plugins/java/build.py'
PLAYER = 'Tester'
PROPERTIES = {
    'online-mode': 'false',
    'white-list': 'false',
    'level-type': 'minecraft\\:flat',
    'generate-structures': 'false',
    'gamemode': 'creative',
    'force-gamemode': 'true',
    'spawn-protection': '0',
    'view-distance': '3',
    'simulation-distance': '3',
    'pause-when-empty-seconds': '-1',
}


class Failed(AssertionError):
    pass


def free_port():
    with socket.socket() as s:
        s.bind(('127.0.0.1', 0))
        return s.getsockname()[1]


class Lines:
    """A process's output lines, collected on a thread, waited on by pattern."""

    def __init__(self, process, log_path, name):
        self.process, self.name, self.lines = process, name, []
        self.log = log_path.open('w', encoding='utf-8')
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        for line in self.process.stdout:
            self.lines.append(line.rstrip('\n'))
            self.log.write(line)
            self.log.flush()

    def wait_for(self, pattern, timeout=120, start=0):
        """The first line from `start` on matching `pattern`, and its index."""
        regex = re.compile(pattern)
        deadline = time.time() + timeout
        while time.time() < deadline:
            for i in range(start, len(self.lines)):
                if regex.search(self.lines[i]):
                    return i, self.lines[i]
            if self.process.poll() is not None and not self.process.stdout.readable():
                break
            time.sleep(0.1)
        tail = '\n'.join(self.lines[-40:])
        raise Failed(f'{self.name}: no line matching {pattern!r} in {timeout} s; last lines:\n{tail}')

    def absent(self, pattern, start=0):
        regex = re.compile(pattern)
        for line in self.lines[start:]:
            if regex.search(line):
                raise Failed(f'{self.name}: unexpected line {line!r}')


class Server:
    def __init__(self, engine, directory, out, name, scratch):
        env = dict(os.environ, HORIZON_HOME=str(scratch / 'home'), XDG_DATA_HOME=str(scratch / 'data'),
                   XDG_CONFIG_HOME=str(scratch / 'config'), HORIZON_NO_AUDIO='1',
                   HORIZON_LAUNCHER_NO_SELF_UPDATE='1')
        self.process = subprocess.Popen([str(engine), '--resources', str(RESOURCES), '--dedicated', '--nogui'],
                                        cwd=directory, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.STDOUT, text=True, encoding='utf-8', errors='replace')
        self.out = Lines(self.process, out / f'{name}.log', name)

    def console(self, line):
        self.process.stdin.write(line + '\n')
        self.process.stdin.flush()

    def maps(self):
        return Path(f'/proc/{self.process.pid}/maps').read_text()

    def stop(self):
        if self.process.poll() is None:
            try:
                self.console('stop')
                self.process.wait(timeout=120)
            except (BrokenPipeError, subprocess.TimeoutExpired):
                self.process.kill()
                self.process.wait()


class Bot:
    def __init__(self, binary, port, out):
        self.process = subprocess.Popen([str(binary), '--server', f'127.0.0.1:{port}', '--name', PLAYER],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                        text=True)
        self.out = Lines(self.process, out / 'bot.log', 'bot')

    def send(self, line):
        self.process.stdin.write(line + '\n')
        self.process.stdin.flush()


def server_dir(root, port, plugins):
    root.mkdir(parents=True)
    props = dict(PROPERTIES, **{'server-port': str(port)})
    (root / 'server.properties').write_text(''.join(f'{k}={v}\n' for k, v in props.items()), encoding='iso-8859-1')
    (root / 'eula.txt').write_text('eula=true\n')
    if plugins:
        (root / 'plugins').mkdir()
        for jar in plugins:
            shutil.copy2(jar, root / 'plugins' / jar.name)
    return root


def check_without_plugins(engine, out, scratch):
    """No plugins folder: the server runs as before, with no JVM in it."""
    port = free_port()
    server = Server(engine, server_dir(scratch / 'bare', port, []), out, 'bare', scratch)
    try:
        server.out.wait_for(r'Done \(', timeout=300)
        if sys.platform.startswith('linux'):
            jvm = [l for l in server.maps().splitlines() if 'libjvm' in l]
            if jvm:
                raise Failed(f'bare: libjvm is mapped without plugins: {jvm[0]}')
        server.out.absent(r'Starting a JVM|No plugins loaded', 0)
    finally:
        server.stop()
    print('no plugins: no JVM mapped: ok')


def check_plugins(engine, bot_binary, built, out, scratch):
    port = free_port()
    jars = [built / 'test-plugins/FerraloomTest.jar', built / 'test-plugins/FerraloomNms.jar']
    root = server_dir(scratch / 'plugins', port, jars)
    shutil.copytree(built / 'dist', root / 'ferraloom-bukkit')
    server = Server(engine, root, out, 'server', scratch)
    log = server.out
    bot = None
    try:
        # Lifecycle: onLoad before the level, onEnable before Done; the NMS plugin loads with a warning.
        loaded, _ = log.wait_for(r'\[FerraloomTest\] lifecycle: onLoad', timeout=300)
        level, _ = log.wait_for(r'Preparing level')
        enabled, _ = log.wait_for(r'\[FerraloomTest\] lifecycle: onEnable')
        done, _ = log.wait_for(r'Done \(')
        if not loaded < level < enabled < done:
            raise Failed(f'lifecycle out of order: onLoad {loaded}, level {level}, onEnable {enabled}, Done {done}')
        # A STARTUP plugin sees every world's WorldInitEvent, then their WorldLoadEvent.
        init, _ = log.wait_for(r'\[FerraloomTest\] event: world-init world$')
        world_load, _ = log.wait_for(r'\[FerraloomTest\] event: world-load world_the_end$')
        if not enabled < init < world_load < done:
            raise Failed(f'world events out of order: onEnable {enabled}, init {init}, load {world_load}, Done {done}')
        log.wait_for(r"'plugins/FerraloomNms\.jar' names net\.minecraft\.server\.MinecraftServer "
                     r"\(from dev\.ferraloom\.nmsplugin\.Adapter\).*it is disabled if it reaches one")
        log.wait_for(r'\[FerraloomNms\] probe: no internals, enabled')
        log.wait_for(r'\[FerraloomTest\] config: greeting=hello from config\.yml folder=true')
        log.wait_for(r'\[FerraloomTest\] server: name=Ferraloom bukkit=26\.3-R0\.1-SNAPSHOT online=0 '
                     r'worlds=world/NORMAL,world_nether/NETHER,world_the_end/THE_END primary=true')
        if sys.platform.startswith('linux') and 'libjvm' not in server.maps():
            raise Failed('plugins: no libjvm mapped')
        print('lifecycle, config, server basics, NMS plugin loaded: ok')

        # The scheduler: async off the server thread; a 40-tick delay and a 10-tick timer, counted in game ticks.
        log.wait_for(r'\[FerraloomTest\] schedule: async primary=false')
        log.wait_for(r'\[FerraloomTest\] blocks: async primary=false air=true solid=true block=true '
                     r'data=minecraft:oak_log\[axis=y\] parsed=minecraft:oak_stairs\[facing=east\]$')
        log.wait_for(r'\[FerraloomTest\] schedule: later after 40 ticks')
        log.wait_for(r'\[FerraloomTest\] schedule: timer ran 5 times in 41 ticks')
        print('scheduler: ok')

        # A player joins.
        bot = Bot(bot_binary, port, out)
        _, line = bot.out.wait_for(r'^joined$|^disconnected', timeout=120)
        if line.startswith('disconnected'):
            raise Failed(f'bot: {line}')
        log.wait_for(rf'\[FerraloomTest\] event: join {PLAYER} [0-9a-f-]{{36}} online=1')
        bot.out.wait_for(rf'^chat .*FerraloomTest welcomes {PLAYER}')
        bot.out.wait_for(rf'^chat .*{PLAYER} joined through FerraloomTest')
        log.wait_for(rf'System chat: {PLAYER} joined through FerraloomTest')
        bot.out.wait_for(r'^commands ftest=true fadmin=false$')
        print('join event and message, command tree without a command the player may not use: ok')

        # Commands, from the player and the console; a stubbed method's exception.
        bot.send('command ftest ping')
        bot.out.wait_for(rf'^chat pong {PLAYER}$')
        server.console('ftest ping')
        log.wait_for(r'pong CONSOLE')
        bot.send('command ftest stub')
        bot.out.wait_for(r'^chat .*An internal error occurred while attempting to perform this command')
        log.wait_for(r'Ferraloom does not implement Player\.getHealth\(\) yet')
        print('commands, stub exception: ok')

        # The NMS plugin's adapter reached: one line, no trace, the plugin disabled, the others unaffected.
        mark, bot_mark = len(log.lines), len(bot.out.lines)
        bot.send('command fnms')
        bot.out.wait_for(r'^chat .*An internal error occurred while attempting to perform this command', start=bot_mark)
        log.wait_for(r'Disabling FerraloomNms v1\.0: it reached net\.minecraft\.server\.MinecraftServer '
                     r'\(from dev\.ferraloom\.nmsplugin\.Adapter\.server\)', start=mark)
        log.wait_for(r'\[FerraloomNms\] Disabling FerraloomNms v1\.0', start=mark)
        log.absent(r'NoClassDefFoundError', mark)
        bot.send('command ftest ping')
        bot.out.wait_for(rf'^chat pong {PLAYER}$', start=bot_mark)
        print('NMS adapter reached: plugin disabled in one line, others run: ok')

        # Tab completion: the plugin's TabCompleter for the first word, Bukkit's player names after it.
        mark = len(bot.out.lines)
        bot.send('suggest ftest pr')
        bot.out.wait_for(r'^suggestions 7 2 protect$', start=mark)
        log.wait_for(r'\[FerraloomTest\] event: tab /ftest pr -> \[protect\]')
        bot.send(f'suggest ftest ping {PLAYER[:2]}')
        bot.out.wait_for(rf'^suggestions 12 2 {PLAYER}$', start=mark)
        print('tab completion, TabCompleteEvent: ok')

        # A plugin's boss bar, shown to the player and taken off.
        mark = len(bot.out.lines)
        bot.send('command ftest bar')
        bot.out.wait_for(r'^boss add Ferraloom bar 0\.5$', start=mark)
        bot.send('command ftest bar off')
        bot.out.wait_for(r'^boss remove$', start=mark)
        print('boss bar: ok')

        # An offline player through the server's lists: op, whitelist, a ban vanilla's /banlist shows, a pardon.
        mark = len(log.lines)
        server.console('ftest lists Offliner')
        log.wait_for(r'lists: offline=true op=true,false whitelist=true,false banned=true', start=mark)
        server.console('banlist players')
        log.wait_for(r'Offliner was banned by FerraloomTest: testing bans', start=mark)
        server.console('ftest pardon Offliner')
        log.wait_for(r'pardon: banned=false', start=mark)
        print('offline player, ops, whitelist, bans: ok')

        # Where the player stands, and the block under their feet.
        mark = len(bot.out.lines)
        bot.send('command ftest where')
        _, where = bot.out.wait_for(r'^chat at world (-?[\d.]+) (-?[\d.]+) (-?[\d.]+)$', start=mark)
        x, y, z = (float(v) for v in where.split()[3:6])
        below = (int(x // 1), int(y // 1) - 1, int(z // 1))
        bx, by, bz = below

        # Block reads, then a write with physics: BlockPhysicsEvent for its neighbours.
        mark = len(bot.out.lines)
        bot.send(f'command ftest block {bx} {by} {bz}')
        bot.out.wait_for(r'^chat block GRASS_BLOCK minecraft:grass_block\[snowy=false\] solid=true empty=false '
                         r'above=AIR sky=15 biome=minecraft:plains$', start=mark)
        bot.send(f'command ftest set {bx + 2} {by + 1} {bz} minecraft:oak_log')
        bot.out.wait_for(r'^chat set minecraft:oak_log\[axis=x\]$', start=mark)
        log_mark = len(log.lines)
        server.console(f'execute if block {bx + 2} {by + 1} {bz} minecraft:oak_log[axis=x] run say log set')
        log.wait_for(r'\[Server\] log set', start=log_mark)
        bot.send('command ftest physics')
        bot.out.wait_for(r'^chat watching physics$', start=mark)
        bot.send(f'command ftest set {bx + 2} {by + 1} {bz + 1} minecraft:stone')
        log.wait_for(r'\[FerraloomTest\] event: physics at -?\d+ -?\d+ -?\d+ changed=\w+', start=log_mark)
        # A sign's state: its text, written by update() and read back.
        bot.send(f'command ftest sign {bx - 2} {by + 1} {bz}')
        bot.out.wait_for(r'^chat sign hello$', start=mark)
        print('block reads and writes, BlockPhysicsEvent, sign text: ok')

        # World reads: heights, chunks, the border, a game rule, another dimension's (unloaded) block.
        bot.send('command ftest world')
        bot.out.wait_for(rf'^chat world sea=-?\d+ highest={by} loaded=true generated=true far=false '
                         r'border=5\.99999\d*E7 inside=true keepInventory=false nether=AIR$', start=mark)
        print('world reads: ok')

        # A cancelled place leaves the air; an uncancelled one places stone.
        target = (bx + 3, by + 1, bz)
        bot.send('command ftest protect on')
        bot.out.wait_for(r'^chat protect true$', start=mark)
        log_mark = len(log.lines)
        bot.send(f'place {bx + 3} {by} {bz}')
        log.wait_for(r'\[FerraloomTest\] event: place STONE at {} {} {} against GRASS_BLOCK replacing AIR by {} '
                     r'cancelled=true'.format(*target, PLAYER), start=log_mark)
        server.console('execute if block {} {} {} minecraft:air run say not placed'.format(*target))
        log.wait_for(r'\[Server\] not placed', start=log_mark)
        bot.send('command ftest protect off')
        bot.out.wait_for(r'^chat protect false$', start=mark)
        time.sleep(0.5)
        log_mark = len(log.lines)
        bot.send(f'place {bx + 3} {by} {bz}')
        log.wait_for(r'event: place STONE at {} {} {} .* cancelled=false'.format(*target), start=log_mark)
        server.console('execute if block {} {} {} minecraft:stone run say placed'.format(*target))
        log.wait_for(r'\[Server\] placed', start=log_mark)
        print('block place: cancelled kept air, uncancelled placed: ok')

        # A storm from a plugin, with its WeatherChangeEvent.
        log_mark = len(log.lines)
        bot.send('command ftest storm on')
        log.wait_for(r'\[FerraloomTest\] event: weather world to=true', start=log_mark)
        bot.out.wait_for(r'^chat storm true$', start=mark)
        bot.send('command ftest storm off')
        bot.out.wait_for(r'^chat storm false$', start=mark)
        print('storm and WeatherChangeEvent: ok')

        # A cancelled break leaves the block and resends it; an uncancelled one breaks it.
        bot.send('command ftest protect on')
        bot.out.wait_for(r'^chat protect true$')
        mark = len(log.lines)
        bot.send('dig {} {} {}'.format(*below))
        _, line = log.wait_for(r'\[FerraloomTest\] event: break (\w+) at {} {} {} by {} cancelled=true'.format(
            *below, PLAYER), start=mark)
        kind = re.search(r'event: break (\w+)', line).group(1).lower()
        bot.out.wait_for(r'^block {} {} {} [1-9]\d*$'.format(*below))
        server.console('execute if block {} {} {} minecraft:{} run say kept'.format(*below, kind))
        log.wait_for(r'\[Server\] kept', start=mark)
        bot.send('command ftest protect off')
        bot.out.wait_for(r'^chat protect false$')
        # A creative player's next break waits out the 5-tick destroy delay the first one started.
        time.sleep(1)
        mark = len(log.lines)
        bot.send('dig {} {} {}'.format(*below))
        log.wait_for(r'event: break {} at {} {} {} by {} cancelled=false'.format(kind.upper(), *below, PLAYER),
                     start=mark)
        server.console('execute if block {} {} {} minecraft:air run say gone'.format(*below))
        log.wait_for(r'\[Server\] gone', start=mark)
        print(f'block break: cancelled {kind} kept, uncancelled broken: ok')

        # Chat: LOW formats, HIGH (ignoreCancelled) and MONITOR follow; a cancelled line reaches no one.
        mark = len(log.lines)
        bot.send('chat hello plugins')
        bot.out.wait_for(rf'^chat \[test\] <{PLAYER}> hello plugins$')
        low, _ = log.wait_for(r'event: chat-low async=true primary=false', start=mark)
        high, _ = log.wait_for(r'event: chat-high hello plugins', start=mark)
        monitor, _ = log.wait_for(r'event: chat-monitor cancelled=false', start=mark)
        if not low < high < monitor:
            raise Failed(f'chat listeners out of priority order: {low} {high} {monitor}')
        mark = len(log.lines)
        bot_mark = len(bot.out.lines)
        bot.send('chat my secret')
        log.wait_for(r'event: chat-monitor cancelled=true', start=mark)
        bot.send('command ftest ping')
        bot.out.wait_for(rf'^chat pong {PLAYER}$', start=bot_mark)
        bot.out.absent(r'secret', bot_mark)
        log.absent(r'event: chat-high my secret', mark)
        print('chat format, cancel, priorities, ignoreCancelled: ok')

        # Teleport, then where the player is.
        mark = len(bot.out.lines)
        bot.send('command ftest tp 10.5 -50 10.5')
        bot.out.wait_for(r'^chat teleported true$', start=mark)
        bot.out.wait_for(r'^pos 10\.5 -50(\.0)? 10\.5$', start=mark)
        bot.send('command ftest where')
        bot.out.wait_for(r'^chat at world 10\.5 -50\.0 10\.5$', start=mark)
        print('getLocation, teleport: ok')

        # A chest GUI a command opens: its title and its items' names and lore reach the client.
        mark, log_mark = len(bot.out.lines), len(log.lines)
        bot.send('command ftest gui')
        _, opened = bot.out.wait_for(r'^open (\d+) Ferraloom GUI$', start=mark)
        window = opened.split()[1]
        log.wait_for(r'\[FerraloomTest\] event: open \S*Ferraloom GUI gui=true', start=log_mark)
        _, content = bot.out.wait_for(rf'^content {window} ', start=mark)
        emerald = re.search(r'\b13=(\d+)x1 name=Buy lore=1\b', content)
        if not emerald or not re.search(r'\b0=\d+x2\b', content):
            raise Failed(f'GUI content: {content}')
        # A cancelled click on the priced emerald: the plugin sees it, nothing moves, the client is corrected.
        mark, log_mark = len(bot.out.lines), len(log.lines)
        bot.send(f'click {window} 13 0 0')
        log.wait_for(r'event: click gui raw=13 LEFT PICKUP_ALL CONTAINER current=EMERALD price=5 '
                     r'title=\S*Ferraloom GUI cancelled=true', start=log_mark)
        bot.out.wait_for(rf'^slot {window} 13 {emerald.group(1)}x1 name=Buy lore=1$', start=mark)
        bot.out.wait_for(r'^cursor empty$', start=mark)
        # An uncancelled click takes the diamonds onto the cursor.
        log_mark = len(log.lines)
        bot.send(f'click {window} 0 0 0')
        log.wait_for(r'event: click gui raw=0 LEFT PICKUP_ALL CONTAINER current=DIAMOND price=null '
                     r'title=\S*Ferraloom GUI cancelled=false', start=log_mark)
        mark = len(bot.out.lines)
        bot.send('command ftest cursor')
        bot.out.wait_for(r'^chat cursor DIAMOND 2$', start=mark)
        bot.send(f'close {window}')
        log.wait_for(r'event: close \S*Ferraloom GUI gui=true', start=log_mark)
        # The held item a plugin reads is the slot's: taking two of five leaves three.
        bot.send('command ftest hand')
        bot.out.wait_for(r'^chat hand 3 true$', start=mark)
        print('chest GUI: open, names and lore, cancelled and uncancelled clicks, close; held item mirror: ok')

        # The player leaves.
        mark = len(log.lines)
        bot.send('quit')
        bot.process.wait(timeout=30)
        log.wait_for(rf'\[FerraloomTest\] event: quit {PLAYER} online=true', start=mark)
        log.absent(rf'System chat: {PLAYER} left the game', mark)
        print('quit event, message removed: ok')

        # A plugin stalling the server thread: the watchdog names it, with its stack, and the server goes on.
        mark = len(log.lines)
        server.console('ftest stall 13')
        log.wait_for(r'The server thread has been stuck in FerraloomTest v1\.0 for 1\d s', start=mark, timeout=40)
        log.wait_for(r'at dev\.ferraloom\.testplugin\.TestPlugin\.onCommand', start=mark)
        log.wait_for(r'stalled', start=mark, timeout=20)
        print('watchdog: ok')
    finally:
        if bot is not None and bot.process.poll() is None:
            bot.process.kill()
        mark = len(log.lines)
        server.stop()
    stopping, _ = log.wait_for(r'Stopping server', start=mark, timeout=10)
    disabled, _ = log.wait_for(r'\[FerraloomTest\] lifecycle: onDisable', start=mark, timeout=10)
    if disabled < stopping:
        raise Failed('onDisable before "Stopping server"')
    # The plugin loader's chunk ticket cleanup at disable reaches every world.
    log.absent(r'removing chunk tickets', mark)
    print('onDisable at stop: ok')


def target_dir():
    metadata = subprocess.run(['cargo', 'metadata', '--no-deps', '--format-version', '1'], cwd=ROOT, check=True,
                              capture_output=True, text=True).stdout
    return Path(json.loads(metadata)['target_directory'])


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--engine', type=Path, help='default: the release horizon-client')
    parser.add_argument('--bot', type=Path, help='default: the release plugin_bot example')
    parser.add_argument('--out', type=Path, default=ROOT / '.local/plugins-e2e')
    args = parser.parse_args()
    if args.engine is None or args.bot is None:
        target = target_dir() / 'release'
        args.engine = args.engine or target / 'horizon-client'
        args.bot = args.bot or target / 'examples/plugin_bot'
    if not (RESOURCES / 'items.json').exists():
        sys.exit(f'plugins_e2e: no prepared resources in {RESOURCES}')
    shutil.rmtree(args.out, ignore_errors=True)
    args.out.mkdir(parents=True)
    built = args.out / 'bridge'
    subprocess.run([sys.executable, str(BUILD), '--out', str(built)], check=True)  # the API jar stays cached in .local
    with tempfile.TemporaryDirectory(prefix='ferraloom-plugins-e2e-') as scratch:
        scratch = Path(scratch)
        try:
            check_without_plugins(args.engine.resolve(), args.out, scratch)
            check_plugins(args.engine.resolve(), args.bot.resolve(), built, args.out, scratch)
        except Failed as e:
            print(f'FAILED: {e}', file=sys.stderr)
            sys.exit(1)
    print('plugins e2e: all checks passed')


if __name__ == '__main__':
    main()
