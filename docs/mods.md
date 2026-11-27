# Horizon mods

Ferraloom runs Horizon's mod system: mods written in Rust, compiled to WebAssembly and run in a sandbox. On a
server that means **server mods** (API 0.3, experimental) and **content mods** (API 0.2: blocks, items, tags
and worldgen data). Fabric and NeoForge jars do **not** run as they are: their code calls and patches
Minecraft's Java classes. What carries over is the data and assets inside a jar ([modpacks.md](modpacks.md)).
Bukkit plugins are a separate system ([ferraloom/plugins.md](ferraloom/plugins.md)).

Mods are **off and absent by default**. A fresh server has no `horizon-mods` folder, loads no runtime and plays
exactly like vanilla 26.3.

The contract still holds Horizon's client interfaces (API 0.1: `game-read`, `chat`, `hud`, `ui`, `render`,
`input`, `recipes` and the `frame` hook). They ran in Horizon's game window, which was removed with the client:
nothing in this repository hosts a client mod now, and this page describes them only where the contract,
the SDK or the test runner still carry them.

| Part | Where | State |
|---|---|---|
| The contract (WIT) | `crates/horizon-mod-sdk/wit/horizon.wit` | the `horizon-mod` world (client, 0.1) and the `horizon-server-mod` world (0.3) |
| SDK for mod authors | `crates/horizon-mod-sdk` | client and server mods |
| Example mods | `crates/horizon-mod-sdk/examples/` | server: `welcome`, `beacons`; client: `coords-hud`, `predator-missile` |
| Runtime | `crates/horizon-mods` | wasmtime 48 LTS, component model |
| Authoring tool | `crates/cargo-horizon-mod` | `cargo horizon-mod new [--template code\|content] / build / test / check`, and the registry's CI (`registry build / review / list`) |
| In the engine | `crates/horizon-client/src/game/mods.rs`, `game/server_mods.rs`, `src/mod_content.rs` | server and content mods |
| Registry index (`index.json`, `.hpack`), signatures | `horizon_mods::index`, [api/mod-index.md](api/mod-index.md) | schema 1 |

## Design goals

1. **Safe to install.** A mod is untrusted code. It runs in a WebAssembly
   sandbox with no file, network, process or clock access, a memory cap and a
   time budget per call. It can only do what its capabilities grant, and the
   owner sees those before turning it on.
2. **One file for every platform.** The same `.hmod` runs on Windows, macOS
   and Linux, x86-64 and ARM: authors build once.
3. **Free when unused.** No mod on: no engine, no thread, no compile, one
   branch per hook. Measured below.
4. **Vanilla stays vanilla.** Mods add; they cannot patch the game's code the
   way Mixins do. What they may change is a list of hooks the game calls at
   fixed points, so the parity harnesses keep testing the same game.
5. **Stable for authors.** The contract is a versioned WIT package; a mod
   built for an API line keeps loading on later releases of that line.

## How it works

```
 mod crate (Rust)                         Horizon
 ───────────────                          ───────
 impl ServerMod for MyMod { ... }         horizon-mods::Mods
 export_server_mod!(MyMod)                  ├─ Engine (cranelift), made on first mod
        │ cargo horizon-mod build           ├─ one Store per mod: 64 MiB, fuel per call
        ▼                                   ├─ Linker: log, storage, players, world (WIT imports)
 my_mod.hmod = zip {                        └─ calls the server-events export: init, tick,
   horizon-mod.toml,                             joins, chat, commands, blocks
   mod.wasm  (a WebAssembly component)
 }
```

- **WebAssembly components.** Mods compile to `wasm32-unknown-unknown`;
  `cargo horizon-mod build` wraps the module into a
  [component](https://component-model.bytecodealliance.org/) for the
  `horizon:mods/horizon-server-mod` world (or `horizon-mod` for a client mod). The WIT file is the whole API:
  host functions the mod imports and the events interface it exports. No WASI: the component cannot even
  name a file or a socket.
- **Capabilities are imports.** Every imported interface except `log` is a
  capability. The manifest declares them; the game reads the component's
  actual imports and refuses a mod whose code imports one its manifest does not
  declare (`uses capability world, missing from horizon-mod.toml`). So the list
  the owner reads is the truth, not a promise.
- **Hooks on request.** `init` returns a `registration`: the commands the mod
  owns and which events it wants. The game calls only those.
- **No references into the world.** Mods never hold references into the game,
  so they cannot stall or corrupt it: reads are lent for the length of a call
  and writes are queued (below).

## Choosing the runtime

| Option | Sandbox | One file for all OSes | Typed API | Cost | Verdict |
|---|---|---|---|---|---|
| **wasmtime + component model** | yes: memory, fuel, no imports but ours | yes | WIT → Rust on both sides | 85 ns per empty hook call; 50 ms to compile a 64 KB mod on load | **chosen** |
| wasmtime, core modules with a hand ABI | yes | yes | no: pointers and lengths by hand | a few ns less per call | not worth the fragile ABI |
| wasmi (interpreter) | yes | yes | no components | no compile time, execution ~10-50x slower than cranelift code | kept in mind for a platform without JIT |
| Lua (mlua) or another script | partial (strip the stdlib, hope) | yes | no | fast to write | not Rust; weak sandbox |
| native `cdylib` plugins | none | no: per OS and arch, unstable Rust ABI | yes | none | unsafe for players |
| an embedded JVM | none per mod | yes | Java | 100+ MB, seconds of startup | see the research |

wasmtime 48 is a long-term-support line (security fixes until August 2028)
and needs Rust 1.95. It reads WebAssembly with the same wasm-tools release
(0.254) as the `wit-component` that packs mods and the SDK's `wit-bindgen`, so
the build carries one copy of them. Only its `runtime`, `cranelift`,
`component-model` and `std` features are on: no WASI, no async, no GC
proposal, no profiling. Its cranelift backend generates native code for x86-64
and ARM64 on Windows, macOS and Linux, which is why one `.hmod` serves them
all.

The engine links it only with its `mods` feature: every release build turns it
on (`game-build.yml`), and a build from source needs
`cargo build --release -p horizon-client --features mods` to run code mods.
Without it (the tests and probes) wasmtime and cranelift aren't built, a
quarter of a clean build; content mods still load, and the engine logs why the
code mods don't start.


### Performance

Measured on the Mac mini (M4 Pro), release build, `cargo test -p
horizon-mods --features runtime,pack --release --test sandbox -- --ignored --nocapture`:

| | per call |
|---|---|
| tick with no mod loaded | 1.2 ns (one check) |
| empty `tick` hook (call into the sandbox and back, fuel on) | 85.5 ns |
| coords-hud's `tick` (reads the player and clock, formats 3 lines, compares) | 2.3 µs |
| loading coords-hud (read, compile with cranelift, instantiate, `init`) | 50 ms, once |

**Budgets.** Each call gets fuel (about one unit per WebAssembly instruction):
2 billion for `init`, 50 million for `tick`, 100 million for a command or chat
line. A runaway loop ends in a fraction of a second (the sandbox test's endless
`/spin` is cut well within its 5 s limit) instead of hanging the server. The
target a mod should stay under is **1 ms per tick**. The runtime keeps each
mod's average call time (`Entry::cost`).

### Crash isolation

A trap (panic, out-of-bounds access, `unreachable`), running out of fuel or
hitting the 64 MiB memory cap stops **that mod only**. The engine logs why
(`Mod Test Guest stopped: it took too long (an endless loop?)`) and carries on;
the other mods keep running. Host functions clip what a mod hands back
(messages per call, block changes per call). Tests:
`crates/horizon-mods/tests/sandbox.rs` (an endless loop, a panic and a 1 GiB
allocation, next to a healthy mod; `cargo test -p horizon-mods --features
runtime,pack`).

## Running mods on a server

1. Put `.hmod` files (or mod folders holding `horizon-mod.toml` and
   `mod.wasm`) in `horizon-mods/` in the server folder.
2. Turn each on with a `mod.<id>:true` line in `horizon-mods.txt` beside it.
   They start off.
3. A mod that failed to load or stopped says why in the server log.

Two turned-on mods never share an `id` while running (it is also capability
`storage`'s directory): the one `find` lists first starts, the other stays off.

## Making a mod

```sh
rustup target add wasm32-unknown-unknown
cargo install --git https://github.com/RealDoc06/Ferraloom cargo-horizon-mod
cargo horizon-mod new my-mod        # Cargo.toml, horizon-mod.toml, src/lib.rs, tests/hello.toml
                                    # (--template content: a content mod, no code)
cd my-mod
cargo horizon-mod build             # target/horizon-mod/<id>.hmod
cargo horizon-mod test              # build, then run tests/*.toml with no game
```

To try it, copy the built `.hmod` into a server's `horizon-mods/` and turn it on
([Running mods on a server](#running-mods-on-a-server)).

Before you submit it to the registry, run what its CI runs:

```sh
cargo horizon-mod check target/horizon-mod/my_mod.hmod   # --json for the report as JSON
```

It reads the manifest and content, refuses imports the capabilities don't
cover, then plays the mod in the sandbox for 40 ticks, a chat line and each of
its commands: it must start and never trap or overrun. It checks a `.hpack` and
a resource pack `.zip` too, and a signature with `--key <public key>
--signature <file>.minisig`.

### Testing a mod

`cargo horizon-mod test` builds the mod, then runs each `tests/<name>.toml` in
the sandbox the game uses, with no game: the test plays the game's part (for a
client mod, the player and the clock it reads, ticks, frames, chat lines and
commands; for a server mod, players joining, chatting, typing commands,
breaking and placing blocks and leaving) and checks what the mod showed. A script fails at its first wrong
step, with what it expected and what it got; the command fails if any script
does. `--debug` builds without optimizing, `--hmod <file>` tests a `.hmod`
already built, and a name runs only the scripts whose path holds it. Each
script gets a scratch copy of the mod, so its `storage` starts empty and is
removed afterwards.

```toml
# tests/panel.toml (examples/coords-hud)
[player]                 # where the script starts; unset fields keep the defaults
x = 12.5                 # (0.5 64 0.5, yaw 0, 20 health and food, the Overworld)
z = -3.25
yaw = 180.0

[clock]                  # day-time 0 is 6:00 on day 1; each tick adds one
day-time = 29999

[[step]]
ticks = 1
expect.panel = ["XYZ 12.5 / 64.0 / -3.2", "Facing North (-Z)", "Day 2, 12:00"]
expect.corner = "top-left"

[[step]]
command = "coords"
expect.feedback = "Coordinates panel hidden"
```

A step's parts happen in this order:

| Key | Does |
| --- | --- |
| `reload = true` | stops the mod and starts it again, as a rebuild does in the game: its memory starts over, its `storage` stays |
| `title-screen = true` | no player and no clock until a step sets them |
| `blocks = [{ x = 0, y = 63, z = 0, state = "minecraft:stone" }]` | server mods with `world`: those blocks of the Overworld, before the rest of the step (every other block is air, every biome the dimension's plainest: plains, nether wastes, the end) |
| `player = { x = 3.0, ... }`, `clock = { day-time = 0 }` | changes those fields (`x y z yaw pitch health food dimension`, `game-time day-time`) |
| `ticks = n`, `frames = n` | that many ticks (the game's tick counter and the clock move on), then frames 1/60 s apart |
| `chat = "<Steve> hi"` | a chat line the player receives |
| `command = "where north"` | the player types `/where north`; a command the mod doesn't own fails the step |
| `break = { x = 1, y = 63, z = 0, state = "minecraft:stone" }`, `place = { ... }` | server mods: the player breaks or places that block |
| `leave = "Alex"` | server mods: that player leaves |

A server mod starts with nobody on: `join = "Alex"` (after `reload` and `title-screen`) brings a
player in at the defaults, in survival, with the id
`00000000-0000-4000-8000-000000000001` for the first name that joined, `...2`
for the second, the same id when a name joins again. `player`, `chat`,
`command`, `break` and `place` are then that player's doing: the one named by
`as = "Steve"`, or the first one on. `player` takes `game-mode` there, and the
script's own `[player]` and `[clock]` are a client mod's.

Then `expect` checks what the mod did during the step; a key left out is not
checked:

| `expect.` | Checks |
| --- | --- |
| `commands = ["where"]` | the commands the mod owns |
| `chat = ["..."]` | every chat line it showed, in order (`[]`: none) |
| `feedback = "..."`, `error = "..."` | the command's answer, shown plain or in red |
| `panel = ["..."]`, `corner = "top-left"` | its HUD panel's lines (`[]`: no panel) and corner |
| `told = ["Alex: Welcome!"]`, `broadcast = ["..."]` | server mods: every `players.tell` (as `name: text`) and `players.broadcast`, in order |
| `blocks = [{ x = 2, y = 64, z = 0, state = "minecraft:glowstone" }]` | server mods with `world`: those blocks of the Overworld after the step, the mod's `set-blocks` made (the script checks a state's shape; the game checks its name) |
| `stopped = "panic"` | the mod was turned off, for a reason holding this text; without it, a mod that traps, panics or overruns fails the test |

Unknown keys are errors, so a typo never passes silently. The same runner is
`horizon_mods::testing` for Rust code that drives a mod (`TestGame`,
`Script`): Horizon's own tests run the examples' scripts with it. Scripts test
any mod, whatever it was written in.

### Template layout

```
my-mod/
├── Cargo.toml           # cdylib, horizon-mod-sdk, release: opt-level "s", LTO
├── horizon-mod.toml     # the manifest
├── src/lib.rs           # impl Mod, export_mod!
└── tests/hello.toml     # cargo horizon-mod test
```

The template is a client mod: a type implementing `horizon_mod_sdk::Mod`, its
state in the type. A server mod implements `ServerMod` instead ([below](#server-mods-api-03-experimental)).

```rust
use horizon_mod_sdk::{Mod, Registration, chat};

struct Hello { greeted: u32 }

impl Mod for Hello {
    fn init() -> Result<(Self, Registration), String> {
        Ok((Hello { greeted: 0 }, Registration::new().command("hello")))
    }
    fn command(&mut self, _name: &str, args: &str) -> Result<Option<String>, String> {
        self.greeted += 1;
        chat::show(&format!("Hello {args}! ({} so far)", self.greeted));
        Ok(None)
    }
}
horizon_mod_sdk::export_mod!(Hello);
```

Hooks not asked for keep their default bodies. The full example is
[`coords-hud`](../crates/horizon-mod-sdk/examples/coords-hud/src/lib.rs). Logic
that does not touch the API (formatting, maths) is ordinary Rust and unit-tests
on the host with `cargo test`.

### The manifest

```toml
id = "coords_hud"            # [a-z0-9_]{1,64}: settings key, log name
name = "Coordinates HUD"
version = "0.1.0"
description = "..."
authors = ["..."]
api = "0.1"                  # the API the SDK targets
side = "client"              # client | server | both
capabilities = ["game-read", "hud", "chat"]
experimental = []            # experimental capabilities it uses, e.g. ["ui"]
```

Unknown keys are refused, so a typo is an error, not a silently ignored
permission. An experimental capability (today `ui`, `render` and `input`) may change in any release
([stability](api/stability.md)): a mod uses it only by naming it in
`experimental` as well, so nobody depends on it by accident.

### Content mods (API 0.2)

A mod with `api = "0.2"` and capability `registries` may carry `assets/` and
`data/` in the `.hmod`, and needs no `mod.wasm` if it has no code
(`cargo horizon-mod new --template content` starts one; `build` in a folder
with no `Cargo.toml` packs the manifest, `assets/` and `data/`, and `check`
reads it without a trial run). It adds
blocks and items by declaring them in `data/<id>/horizon/block/*.json` and
`data/<id>/horizon/item/*.json`, each copying a vanilla template
([modpacks.md](modpacks.md#registries-02-data-driven-blocks-and-items)):

```json
{ "template": "minecraft:iron_block",
  "properties": { "hardness": 4.0, "light": 3, "sounds": "minecraft:amethyst_block" },
  "item": true }
```

The fence: files go in the mod's own namespaces (`assets/<id>/`,
`data/<id>/`, and those its manifest lists), plus additions to vanilla tags
(`data/minecraft/tags/`) and to the convention tags NeoForge and Fabric mods
share (`data/c/tags/`).
Templates and `sounds` name vanilla blocks; unknown keys are refused.

Some Java mods ship their files under other names than their mod id (Towns
and Towers, mod id `t_and_t`, under `kaisyn` and `towns_and_towers`). A
content mod lists those in its manifest:

```toml
id = "t_and_t"
api = "0.2"
capabilities = ["registries"]
namespaces = ["kaisyn", "towns_and_towers"]
```

A listed namespace is the mod's as its id is: its `assets/` and `data/`
files load, and the game reads its tags, recipes, names and loot. Blocks and
items are still declared under `data/<id>/horizon/` only. `minecraft`, `c`
and `horizon` cannot be listed. Two mods that are on may not own the same
namespace (one's id or listed namespace being the other's): the game keeps
the first in load order and leaves the other's content out, with
"mods A and B both use the namespace N; turn one of them off" in the log. Paths
use vanilla's resource characters, at most 65,536 files, 16 MiB each and
512 MiB in all. A mod whose content is refused stays off, with the reason in
the log. The game reads this content into its block and item tables
(`horizon_client::mod_content`, docs/modpacks.md). A tag entry naming an id
nobody registers (a block a port left out, another mod's item) is dropped,
so the rest of the tag still loads; vanilla would drop the whole tag.

Tags merge as a data pack's do. Every mod's file for one tag (vanilla's
`data/minecraft/tags/`, the convention `data/c/tags/`, or another mod's) adds
its values after vanilla's and the mods' before it, in load order. A file with
`"replace": true` first drops what came before it, vanilla's entries and the
earlier mods', and the mods after it still add theirs, as a higher data pack
does. Since the result depends on load order, the game says so in the log:
"mod towns replaces data/minecraft/tags/worldgen/biome/has_structure/village_plains.json:
vanilla's entries and those of adds are left out, ...". Tags are the only
`minecraft` files a mod may hold: its other content stays in its own
namespaces.

A mod adds to structure sets and template pools that are not its own
(vanilla's villages, another mod's houses) with a set or pool file in its own
namespace holding `horizon:add_to`, a key vanilla data never has. Its entries
are appended to each set or pool named, after every mod's own files, in mod
and file order; the file is no set or pool itself. A name nobody defines is
skipped.

```json
{ "horizon:add_to": "minecraft:villages",
  "structures": [ { "structure": "mymod:village_beach", "weight": 10 } ] }
```

A pool addition holds `"elements"` as a pool does. This is what lithostitched's
`add_structure_set_entries` and `add_template_pool_elements` modifiers do on
NeoForge and Fabric, and `horizon-port-pack carry` writes them from those.

A structure (and any of its pool elements, in place of the structure's) may
hold `horizon:enhanced_terrain_adaptation`: MoogsStructureLib's terrain
kernel, which carves or buries terrain around the
pieces on top of vanilla's `terrain_adaptation`. `kernel_size` and
`kernel_distance` shape the kernel; `top` and `bottom` (`carve`, `bury` or
`none`) act above and below a piece's floor; `bottom_offset`, `padding`
(`x`, `top`, `bottom`, `z`) and MoogsStructureLib's `band` (`bottom`, `top`,
`piece_heights`) move the box it measures from. On a structure,
`rigid_junctions` adds rigid pieces' junctions as well and
`inflate` grows the start's box by the kernel radius. `kernel_size` 0 is
none.

```json
{ "horizon:enhanced_terrain_adaptation": { "kernel_size": 24, "kernel_distance": 24,
    "top": "carve", "bottom": "none", "rigid_junctions": true, "inflate": true } }
```

### Server mods (API 0.3, experimental)

A server mod runs where the world is simulated: on the dedicated server. Its
manifest says `side = "server"` and `api = "0.3"` (and only a server mod may);
its component targets the WIT world `horizon-server-mod` instead of
`horizon-mod`. Experimental: it may change in any release
([stability](api/stability.md)).

| Interface | Capability | Functions |
|---|---|---|
| `log` | always | `write(level, message)`, in the server log as `[mod <id>]` |
| `storage` | `storage` | `get(key)`, `set(key, value)`, `delete(key)`: a per-mod folder in the server's `horizon-mods/` of up to 64 files, 4 MiB total, at most 1 MiB written per call |
| `players` | `players` | `online()`: id, name, position, dimension, game mode, health of each player on; `tell(id, text)`, `broadcast(text)`: system messages after the mod's name, 8 per call |
| `world` | `world` | `get-block(dimension, x, y, z)`: the state in a loaded chunk, `minecraft:oak_log[axis=y]`; `get-biome(dimension, x, y, z)`: `minecraft:plains`, ...; `set-blocks(dimension, changes)`: up to 4096 per call, checked at once, made as `/setblock` makes them (neighbours, light, block entities, every player told) after the call, before that dimension's next tick |

| Hook (export `server-events`) | When |
|---|---|
| `init() -> result<registration, string>` | once, after loading |
| `tick(tick)` | each server tick |
| `player-joined(player)`, `player-left(player)` | `registration.joins` |
| `chat(player, text)` | `registration.chat`: each chat message a player sends |
| `command(player, name, args, values)` | `/name args` typed by a player, for a command it registered, with its arguments' values (below); `ok(some)` is that player's feedback, `err` their red error |
| `block-broken(player, pos, state)`, `block-placed(player, pos, state)` | `registration.blocks`, with the state as `minecraft:oak_log[axis=y]` |

Capabilities that act on a local player (`game-read`, `chat`, `hud`) belong
to the client world; `players` is for server mods. In Rust: `impl
horizon_mod_sdk::server::ServerMod` and `export_server_mod!`; the example is
[`welcome`](../crates/horizon-mod-sdk/examples/welcome/src/lib.rs), and with
`world`, [`beacons`](../crates/horizon-mod-sdk/examples/beacons/src/lib.rs).

**Typed commands.** Each command in `registration.commands` lists its
arguments, in order: a name and a kind, required or optional (optional ones
last). The server adds them to the command tree every player receives, after
vanilla's commands, so a player's chat box completes and colours them as it
does vanilla's (player names, coordinates, bounds); when a player sends one,
the server parses the line the way vanilla's parsers do and gives the mod the
values, by argument name, or answers with vanilla's error (`Integer must not
be more than 64, found 65`, `No player was found`) without calling the mod.

| Kind | Takes | Value | Chat box parser |
|---|---|---|---|
| `word` | one word: letters, digits, `_ - . +` | `word(string)` | `brigadier:string` (word) |
| `text` | the rest of the line, only last | `text(string)` | `brigadier:string` (greedy) |
| `integer(min, max)` | a whole number within the bounds given | `integer(s32)` | `brigadier:integer` |
| `number` | a number | `number(f64)` | `brigadier:double` |
| `boolean` | `true` or `false` | `boolean(bool)` | `brigadier:bool` |
| `player` | an online player's name, `@s` or `@p` (whoever typed it) | `player(player)` | `minecraft:entity` (one player) |
| `block-pos` | `x y z`, each a whole number, `~` or `~n` (from whoever typed it) | `block-pos(block-pos)` | `minecraft:block_pos` |

A command with no arguments takes nothing after its name. In Rust,
`Registration::command_with("beacon", [Argument::integer("height", Some(1),
Some(64)).optional()])`, read with `args.integer("height")`; `command(name)`
alone takes free text, an optional `text` argument named `args`
(`args.text()` is the line as typed either way).

**Capability `world`.** Reads are of the server's loaded chunks as they are
during the call: the game lends a server mod's call the levels' chunk maps
(moved, not copied, and only while a running mod has `world`), so a read
costs a map lookup and no copy of the world. A dimension with no chunk loaded
at a position reads as none. Writes go through the same code as `/setblock`,
so block updates, light, block entities and every player see them as
vanilla's; the parity harnesses test that code. Changes for a dimension whose
level isn't ticking wait for it, up to 65,536 blocks; a
change in a chunk that isn't loaded when it is made is skipped, as `/setblock`
refuses one.

**Where they run.** The dedicated server (`--dedicated`, `horizon-server`) starts the server mods that are
on, and they see every connected player. The mods live in `horizon-mods/` of the server's folder and are
turned on with `mod.<id>:true` in `horizon-mods.txt`.

**When hooks run.** At the end of each server tick the game tells the server
mods who joined and who left since the last one, then what players said,
broke and placed, in order, then calls `tick`. A command a player types
reaches its mod at once, whatever the player's permission level, and its
answer goes to that player. `tell` reaches one player as a system message;
`broadcast` reaches every player and the server's log. With no server mod
on, each of these points costs one branch (measured in the PR that added
them).

**Not yet.** Block events are a player's own breaking and placing, not
explosions, pistons or other mods. Command arguments don't take entities,
items or block states yet, and `^` coordinates.

## Versions and compatibility

- The API is the WIT package `horizon:mods@0.1.0`. New functions and
  interfaces arrive in patch versions (0.1.1, ...); a game serving 0.1.N loads
  mods built for 0.1.0 to 0.1.N. A breaking change makes `0.2` (and later
  `1.0`); the game then links both versions side by side for at least one
  release cycle, so existing mods keep working while authors move.
- The manifest's `api` names the line it needs (`0.1`); the game refuses a
  line it does not serve with a clear message.
- Within a line, each function has an API level, the one it arrived at
  ([stability](api/stability.md#mod-api-levels)). A mod needs the level of
  the newest function it calls, read from its file: an older Horizon refuses
  it with "made for a newer Horizon: update Horizon", and the registry lists
  the level so an installer can take a version the engine runs.
  `cargo horizon-mod check` prints it ("needs mod API level 1 or newer").
- Horizon follows Minecraft releases; the mod API does not have to. Vanilla
  concepts mods see (dimensions, block and item ids) are the game's own ids,
  so a version port changes the data, not the API.


## Multiplayer

- **Server mods** use only vanilla messages, so any vanilla client can join a server running them.
- **Client mods** (`side = "client"`) ran in Horizon's own game; nothing hosts them now.
- **Both** (`side = "both"`, a server mod with a client half) and registries synced to clients were planned
  with Horizon's client and are not built.

## Distribution

- **The file.** `.hmod`: a zip holding `horizon-mod.toml`, `mod.wasm`, and for
  content `assets/` and `data/`. `mod.wasm` is capped at 32 MiB.
- **Integrity and signing.** The registry's index lists each file's SHA-256 and
  pins each project's Ed25519 public key (minisign format: a `.hmod.minisig`
  beside the file), so an update signed by another key is refused
  (`horizon_mods::index::check_file`). The formats: [api/mod-index.md](api/mod-index.md).
- **Where mods came from.** Horizon's registry (reviewed pull requests in
  `horizon-rust/horizon-mods`) went with upstream's repository. Install files by hand (above).
