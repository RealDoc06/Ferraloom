# Client rig

The unmodified vanilla 26.3 client, run headless on the builder (`ferraloom-builder`) and driven from
devbox with one command per action. It needs no laptop, no account and no firewall change: client and
server both run on the builder and talk over `127.0.0.1`.

How it works: an Xvfb display (`:47`, 1280x720), Mesa's software renderer (llvmpipe), the client started
offline (`--accessToken 0`) with Mojang's own launch arguments, `xdotool` for keys and mouse, and
ImageMagick `import` for screenshots of the X framebuffer. No mod and nothing patched.

## Use

From any checkout of this branch on devbox:

```sh
R=tools/client_rig/rig
$R setup                                  # once: libraries, assets (~600 MB), server resources, options.txt
$R server start [--fresh] [--flat] [--plugins DIR]   # Ferraloom on 127.0.0.1:25571
$R start --join 127.0.0.1:25571           # display and client, straight into the server
$R server cmd op RigPlayer                # any console command; prints what the console answered
$R chat "/gamemode creative"              # chat, or a command with a leading /
$R key e  |  $R key Escape  |  $R key F3  # xdotool key names
$R hold w 1.5                             # walk 1.5 s (shift+w, space ...)
$R look 0 -200                            # turn: pixels of mouse motion (about 0.15 degrees each)
$R click [left|right] [--hold S] [--at X Y] [--shots NAME --count N --interval S]
$R xdo click 1 sleep 0.08 mousemove_relative -- 0 200 click 1   # one timed xdotool chain
$R shot NAME [--hold Tab] [--count N --interval S]               # screenshots, copied to devbox
$R log 20                                 # the client log: chat shows as [CHAT] lines
$R server log 40
$R status
$R stop                                   # client, display and server
```

Screenshots land in `~/client-rig/shots/` on the builder and are copied to
`~/.cache/ferraloom-client-rig/shots/` on devbox (`RIG_SHOTS` changes it); the command prints each local path.
Quote console commands that hold selectors or NBT: `$R server cmd 'data get entity @e[type=pig,limit=1] Health'`.

Server binaries come from one `ferraloom-build` job that copies `horizon-client` and `horizon-server` into
`~/client-rig/server/`:

```sh
ferraloom-build -- bash -c 'cargo build --release --locked -p horizon-client --no-default-features -p horizon-server && mkdir -p ~/client-rig/server && cp target/release/horizon-client target/release/horizon-server ~/client-rig/server/'
```

`server start` runs that engine as `--dedicated --nogui` in `~/client-rig/world-server/` (offline mode,
no whitelist, spawn protection off, `server-ip=127.0.0.1`), its console fed from a FIFO. `--plugins DIR` copies
`DIR/*.jar` into `plugins/` and `DIR/ferraloom-bukkit/` beside it; build those with
`crates/ferraloom-plugins/java/build.py --out ~/client-rig/plugins-build --cache ~/client-rig/plugins-cache`
from a copy of `crates/ferraloom-plugins/java` in `~/client-rig/`, never from the builder checkout (its
default output is that checkout's `.local/`). Without `--plugins` the world has no plugins.

## Where things are

Everything lives in `~/client-rig/` on the builder: `cache/` (client jar, libraries, assets), `game/` (the
client's game directory, `options.txt`, `logs/latest.log`), `world-server/`, `resources/` (a copy of the
prepared server resources), `run/` (pid files, `client.out`, `server.out`, the console FIFO), `shots/`.
The builder checkout `~/code/Ferraloom` is only read: the 26.3 manifest and client jar from
`.local/minecraft/26.3/`, libraries from there or `.cache/vanilla/26.3/libraries/`, JDK 25 from
`.cache/toolchains/`. What is missing is downloaded from Mojang's endpoints (`piston-meta`,
`libraries.minecraft.net`, `resources.download.minecraft.net`) and checked against the manifest's SHA-1.
Nothing of Mojang's is in the repository. Note: `.cache/vanilla/26.3/minecraft.jar` is the server's game jar,
not the client; the client jar is `.local/minecraft/26.3/jars/client.jar`.

`options.txt` is merged on every start with fixed values: GUI scale 2, render distance 4, 30 fps, sound
off, narrator and tutorial off, `pauseOnLostFocus:false`, `rawMouseInput:false`, attack indicator under the
crosshair, multiplayer warning and onboarding skipped. At GUI scale 2 a 176x166 container screen sits at
x 464 to 816, y 194 to 526, so slot centres are `464 + 2*(slot x + 8)`, `194 + 2*(slot y + 8)`: the player
inventory's hotbar slot 0 is at (496, 494), a chest's slot 13 at (640, 282).

## Timing

- `setup`: about 10 minutes the first time (5147 asset objects); seconds after.
- Server start: 6 to 7 s to "Done" on a generated world, plugins included.
- Client launch to in world: 15 to 20 s (`start --join` and `join` wait for the play phase and print it).
- Each rig command is one ssh round trip, about 0.3 to 0.5 s. A screenshot takes about 0.07 s raw, so
  `--count` bursts reach one frame every 70 to 90 ms; timed input sequences go in one `xdo` chain.
- llvmpipe on the builder renders 15 to 30 fps at 1280x720.

## Limits

- One rig per builder: one display, one client, one server. `rig status` shows whether someone is using it.
- No sound (`SDL_AUDIODRIVER=dummy`), and the software renderer may differ from a GPU in shading details.
- Input is synthetic: mouse turns go through `xdotool mousemove_relative` with raw input off, so aiming by
  pixels is approximate; aim precisely with `tp ... facing` (from the console the anchor is the feet, as in
  vanilla; use `execute ... anchored eyes run tp` to aim at eye height).
- Reading the game is by screenshot and log only: chat and command feedback in `rig log`, entity state through
  console commands (`data get entity`). There is no in-process hook; a Fabric control mod would add one.
- `join` restarts the client rather than using the multiplayer menu.
- Offline mode only, so chat is unsigned ("Chat messages can't be verified" toast, as vanilla shows on such
  servers), and skins are the defaults.
- The rig shares the builder with build jobs; client and server run under `nice -n 10`. Stop them (`rig stop`)
  when not looking.
