# Writing parity scenarios

`ferraloom-parity` runs the same async function against a disposable vanilla
26.3 server and Ferraloom, then compares only the observations the scenario
records. Put assertions in the function too: vanilla-only execution must prove
the scenario's expectations, rather than merely produce a recording.

```rust
use ferraloom_parity::{parity_test, ObservationLog, Result, Session, Ticks};
use ferraloom_parity::protocol::play::clientbound as cb;

async fn broadcast(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    bot.chat("hello parity").await?;
    let packet = bot.wait_for("echo", Ticks(200), |_, packet| {
        matches!(packet, cb::Packet::PlayerChat(p) if p.body.content.0 == "hello parity")
    }).await?;
    let cb::Packet::PlayerChat(chat) = packet.as_ref() else { unreachable!() };
    let mut observations = ObservationLog::default();
    observations.record("message", &chat.body.content.0)?;
    observations.record("sender", chat.sender.to_string())?;
    Ok(observations)
}
parity_test!(parity_broadcast, broadcast);
```

The macro creates one ignored Tokio test. `run(options, mode, scenario)` is the
same runner with explicit `LaunchOptions` and `RunMode`; use it when a scenario
needs multiple operators, property overrides, fixtures, or custom world settings.
Names in `options.operators` receive level-four permission. The default op is
`ParityBot`. `Session::join` returns a typed bot handle; `bots()` allows borrowing
several clients at once. Each client has independent read/write tasks, so idle
bots still acknowledge keep-alives, pings, chunk batches and teleports.

`Bot::command` strips an optional leading slash and sends `ChatCommand`.
`signed_command` sends `ChatCommandSigned` with empty signatures, the offline
client path for signable message arguments. These paths remain distinct: an
unsigned `/say` produces `DisguisedChat`, while the signable packet produces
`PlayerChat`. The starter asserts both packet variants explicitly. `teleport` waits for the
acknowledged authoritative update. `move_to` interpolates a path in steps of at
most one block, waits at least 100 ms between steps, sends `ClientTickEnd`, and
uses an ordered ping barrier. Choose collision-free spectator travel when the
scenario tests chunk tracking rather than walking physics. `look`, `input`,
`sneak` and `jump(pressed)` send client inputs; jump input alone does not simulate
a client's positional trajectory. `send` accepts any typed serverbound play
packet for scenarios requiring a more exact input trace.

`use_item`, `use_item_on`, `attack`, `swing`, `pick_block`, `drop_item` and
`swap_hands` provide common interactions. `click` uses the last received
container state ID and hashes the caller's predicted slot/carried `ItemStack`s
with the protocol crate's vanilla component hashing. Supply predictions for the
menu being tested; the harness does not duplicate menu simulation. Await the
relevant server update before a subsequent click.

`wait_for(description, Ticks(n), predicate)` consumes received packets in order.
The predicate sees both the packet and current `BotState`. Tick deadlines use
`SetTime.game_time`, normally published every 20 ticks, so timeout resolution is
20 ticks. A separate, generous wall-clock watchdog catches stopped clocks and
hung servers. `wait_ticks` uses that same server clock. Packet queue overruns,
disconnects and decode errors fail explicitly. Keep predicates specific so an
older queued response cannot satisfy the wrong step. `synchronize` is a packet
ordering barrier, not a world-generation or save-completion barrier.

`snapshot()` exposes loaded chunks, block updates, entity spawn/data/position
state, containers, scores/objectives, and player/system/disguised messages.
`BotState::block` returns `None` for unknown or unloaded data. It assumes the
overworld minimum section (-4); custom dimension scenarios must set the bot's
minimum section explicitly with `set_min_section`. It never invents air.
Entity IDs remain the real wire IDs for actions. Record the fields being tested
and explicitly normalize opaque IDs in the observation log when required.

Every observation label is unique. Values are serialized through Serde; diffs
name exact JSON paths and show vanilla and Ferraloom values. Nothing is
normalized by default. For volatile fields, register exact JSON pointers:

```rust,ignore
session.normalize(ferraloom_parity::Normalization::EntityId("/cow/id".into()));
session.normalize(ferraloom_parity::Normalization::RandomUuid("/cow/uuid".into()));
```

`Timestamp` and `VaryingSeed` are available for specifically permitted volatile
numeric fields. Missing paths and wrong types fail. Identity mappings preserve
repeated references across selected fields; normalize every occurrence you
intend to compare. Both server runs must register identical rules. Select
semantic observations deliberately; do not normalize away a gameplay mismatch.

`Session::stop` waits for stop/save completion. Only then can `saved_world()`
open `ferraloom-storage` readers for level data, players, block palettes, block
entities, entity region files and chunks. `restart` relaunches the same universe;
all previous bot handles expire. Drop saved-world readers before restarting so
the storage session lock is released. Failed scenarios still stop/save their
server before returning the error. The fallback Drop path schedules cleanup if
a running Tokio runtime remains; normal callers should always await `stop`.

Launchers use an ephemeral loopback port, offline login, fixed seed, and a new
temp universe; EULA acceptance exists only there. A fixture is a world directory
copied into that universe, excluding `session.lock`; symlinks are rejected.
`WorldPreset::Flat` takes bottom-to-top `(block, height)` layers and a biome;
`Void` uses an air-only flat world. Properties are shared across backends;
loopback, offline login, port and world directory cannot be overridden.
Unsupported in-process presets, game rules and `Regions::On` fail explicitly.
`ferraloom_binary` uses an external binary with `--regions off|on` instead;
that binary is responsible for supporting the selected options. Vanilla launch
uses `JAVA_HOME` and `FERRALOOM_VANILLA_JAR`, or explicit java/jar paths. Its output
is included in scenario errors. External children are killed if a watchdog fires.

Commit first, then run on the dedicated builder:

```sh
ferraloom-build -- sh -c 'cargo test -p ferraloom-parity --locked && cargo test -p ferraloom-parity --locked -- --ignored --test-threads=1 && cargo clippy -p ferraloom-parity --all-targets --locked -- -D warnings && cargo fmt -p ferraloom-parity -- --check'
```

For one side only, put the environment setting *inside* the builder command:

```sh
ferraloom-build -- env FERRALOOM_PARITY_SIDE=vanilla cargo test -p ferraloom-parity --locked -- --ignored --test-threads=1
ferraloom-build -- env FERRALOOM_PARITY_SIDE=ferraloom cargo test -p ferraloom-parity --locked -- --ignored --test-threads=1
```

Keep feature inventory rows scoped to the behavior actually asserted. The
starter cases cover join/configuration and a valid spawn, say/gamemode/tp
results, paced spectator movement with destination chunk load and old chunk
unload, unsigned chat delivery to a second bot with exact decoration fields, and a stopped/saved/restarted player
position. They do not prove the entire corresponding vanilla feature family.

The harness branch intentionally does not include the in-progress commands
foundation. `parity_commands` currently fails on Ferraloom's unsigned `/say`
packet variant: it emits `PlayerChat` where vanilla emits `DisguisedChat`.
The server fix is `e52040c`, which requires foundation `b120a6f`; the coordinator
will integrate those separately. This assertion is executed, never silently
skipped. The signed-command branch runs first so it is still exercised. Remove
this note once the integrated server passes both branches.
