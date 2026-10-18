# Running Ferraloom on Linux

This is the first playable terrain slice for **Minecraft Java 26.3**, protocol
777. It uses a single simulation region. Structures, terrain, carvers and biome features use the native chunk status graph.
Initial mob spawning awaits NaturalSpawner. Block interactions and the rest of vanilla
simulation are still being ported. This runtime joins players into the overworld;
other dimensions, chunk eviction and full ticket propagation remain incomplete.
Player movement currently covers ordinary terrain, without vehicle, pose or
floating checks. Recipe and advancement contents are placeholders.

Block families are still being ported. The main binary temporarily defaults to
`--unported-behavior warn`: each unsupported block/method pair is logged once,
and every skipped call, including shape updates, increments the server's
`unported_skipped_calls` statistic. Only that call is skipped; queries requiring
a value still fail explicitly. Use `--unported-behavior strict` to fail on every
unsupported call. Library callers, integration tests and parity runs default to
Strict. Worldgen unit skips share these counters and log once per feature or
structure piece and behavior. Pending block-entity activation is also counted and
logged per block and factory in Warn; raw NBT is retained for saving and later
activation. Drained generation callbacks do not imply live block-entity readiness,
and required block-entity queries remain unsupported in both modes. Strict fails
on any worldgen skip or pending activation. This temporary mode will be removed
when the block families are ported.

Generation uses the same policy. Strict fails the status step when a placed
feature, structure piece or structure afterPlace callback requires unported behavior.
Warn abandons only that unit, retaining partial writes and continuing to FULL.
Each (unit, behavior) is logged once, and every abandonment increments
the shared skip counter. Warn worlds therefore do not have vanilla generation
parity. Lit proto mutation still needs the relight bridge; block-entity factories,
UpgradeData and initial mob spawning remain pending. Generation jobs own their
neighborhoods and integrate in request order. Overlapping feature jobs follow
vanilla's X-major layer traversal; vanilla's concurrent overlapping writes and
JVM identity-based cross-structure Beardifier iteration can themselves vary.

Thunder strike targeting, lightning rods, difficulty and lightning/skeleton-trap
spawns remain pending. Strict fails at the required target query after vanilla's
strike gate and position draw. Warn counts and logs the remaining strike as one
skipped effect and continues; its RNG stream is consequently not vanilla parity.
Precipitation selections consume their vanilla RNG slots. Live biome zoom,
climate, sky light and heightmaps drive rain, freeze and snow predicates. Snow
survival remains a required query: Strict propagates Unsupported; Warn skips the
whole accumulation unit and logs and counts it without inventing survival.
Entity pushing and cauldron precipitation callbacks remain explicit unit hooks.
Rain queries require integrated light and biome context and fail explicitly if
that context is absent.

Level's canonical random source follows vanilla's nondeterministic unique-seed
construction, independently of the world seed. Controlled tests install a fixed
stream and block-position seed.

Live block and fluid reads currently require an owned chunk. A query that reaches
an absent chunk returns `MissingChunk`; it never substitutes air or silently
skips work. Synchronous loading at those exact read points is a separate follow-up.

On devbox, build a committed checkout on the dedicated builder:

```sh
ferraloom-build -- cargo build --release -p ferraloom-server --locked
```

The executable is `/home/doc/code/Ferraloom/target/release/ferraloom` on
`ferraloom-builder`, not on devbox. Copy that executable to the Linux host where
you want to run the server. Use a fresh directory for the server files:

```sh
mkdir -p "$HOME/ferraloom-world"
cd "$HOME/ferraloom-world"
/path/to/ferraloom --regions off nogui
```

The first run writes `server.properties` and `eula.txt`, then stops. Read the
linked Minecraft EULA and, if you accept it, change `eula=false` to `eula=true`.
Run the command again to create the world. The official 26.3 server jar supplies
vanilla data. Ferraloom downloads it into `.cache/vanilla/26.3/server.jar` and
verifies its pinned SHA-1. You can reuse an existing verified bundle:

```sh
export FERRALOOM_VANILLA_JAR=/absolute/path/to/server.jar
/path/to/ferraloom --regions off nogui
```

For a small, isolated LAN test, set these properties while the server is stopped:

```properties
server-ip=0.0.0.0
server-port=25565
online-mode=false
white-list=false
enforce-secure-profile=false
view-distance=2
simulation-distance=2
```

Offline mode accepts names without Mojang authentication. Use it only on the
trusted test LAN. The default whitelist is enabled in vanilla 26.3; either add
your profile to `whitelist.json` or explicitly disable it as above. Keep online
mode enabled for an authenticated server. Signed chat certificates, message
chains and acknowledgments are validated; online client parity is still unverified.

Connect a 26.3 client to **the server host's LAN IP** and port 25565. If you run
it on devbox, that is `10.10.10.106:25565`. If you copy it elsewhere, use that
machine's address. The client must advertise the built-in `minecraft:core`
26.3 pack; direct registry-data encoding for clients declining that pack is
still being ported.

The console supports `say`, `tp`, `gamemode`, `save-all [flush]`, and `stop`.
Player commands use the permission levels in `ops.json`; a typical entry is:

```json
[{"uuid":"YOUR-AUTHENTICATED-OR-OFFLINE-UUID","name":"YourName","level":4,"bypassesPlayerLimit":false}]
```

Use `save-all flush` to wait for durable chunk writes, then `stop` for an orderly
shutdown. The save directory is `world/`, including `session.lock`, `level.dat`,
`players/data/`, and `dimensions/minecraft/overworld/region/`. Never open the
same world in two servers simultaneously. Keep existing valuable saves separate
from this development slice.

Run the focused checks from a clean committed checkout:

```sh
ferraloom-build -- cargo test -p ferraloom-server -p ferraloom-game --locked
ferraloom-build -- cargo test -p ferraloom-server --test join --locked -- --ignored --nocapture
ferraloom-build -- cargo clippy -p ferraloom-server -p ferraloom-game --all-targets --locked -- -D warnings
ferraloom-build -- cargo fmt --all -- --check
```

The ignored comparison starts vanilla and Ferraloom in temporary directories on
loopback, drives the same protocol client through join, chunk batches, a
100-block walk, chat, saving and stopping, then compares configuration and join
packet types. Java runs only in that explicit comparison; the builder provides
the jar and JDK.
