# Running Ferraloom on Linux

This is the first playable terrain slice for **Minecraft Java 26.3**, protocol
777. It uses a single simulation region. Structures, carvers, features and mob
spawning are not generated yet. Block interactions and the rest of vanilla
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
Strict. This temporary mode will be removed when the block families are ported.

Level's canonical random source follows vanilla's nondeterministic unique-seed
construction, independently of the world seed. Controlled tests install a fixed
stream and block-position seed.

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
