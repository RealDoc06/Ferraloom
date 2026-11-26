# Ferraloom

A Minecraft: Java Edition **26.3** server written in Rust (protocol 777). No JVM is needed to run it, vanilla 26.3 clients join it as they join Mojang's server, and Spigot plugins run on it through an optional bridge.

Ferraloom is a fork of Horizon, the Rust rewrite of Minecraft 26.3 by Ludovic Marie and the Horizon contributors. Horizon was a whole game: client, launcher and server in one. Ferraloom is **the server only**: it keeps Horizon's vanilla logic and its differential harnesses, removes the client, and adapts and optimizes the rest for one job, hosting servers. Horizon's own repository is no longer public; this one carries its full history from v0.2.93.

> Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft. Ferraloom contains no Minecraft code, assets or data: you prepare the game files from your own legally obtained copy of Minecraft: Java Edition 26.3.

## What Ferraloom changes

**Server adaptation**

- The dedicated server is the whole product. The client, the launcher, rendering, audio, input and the client mods are gone from the tree (about 340,000 lines), and the engine links no graphics, sound or controller library.
- Vanilla clients are first-class: behavior a vanilla client can observe is checked against Mojang's server at the packet level and through a real 26.3 client ([`docs/ferraloom/client-rig.md`](docs/ferraloom/client-rig.md)).
- Spigot plugins run on the real Bukkit API through an in-process JVM that loads only when `plugins/` holds a jar ([`docs/ferraloom/plugins.md`](docs/ferraloom/plugins.md)). Horizon's sandboxed WebAssembly server mods still work ([`docs/mods.md`](docs/mods.md)).
- Releases, updates and the install script follow this repository.

**Optimizations**

Measured with 500 bot players (300 walking, 200 exploring) on a 4-core Xeon, with output identical to the serial code:

| | Mean tick | P99 |
|---|---|---|
| Horizon v0.2.93 base | 204.7 ms | |
| Ferraloom | 64.8 ms | ~88 ms |

- Chunk integration is budgeted per tick and generation runs ahead of moving players, so exploring does not stall the tick.
- Mobs tick speculatively on a worker pool and commit in vanilla order; a mob whose inputs changed reruns serially.
- The network tick builds packets on the pool and serves sockets on epoll lanes.
- Per-player rescans, repeated block lookups and per-tick rebuilds are gone from the hot paths.
- Plugins cost nothing until one listens: no JVM without a plugin jar, and an event without a listener is one branch.

Every optimization must pass the parity harnesses unchanged. Design notes: [`docs/ferraloom/`](docs/ferraloom/).

## Status

- **Vanilla parity:** inherited from Horizon and checked by about 300 differential harnesses (`python3 tools/parity.py`), which run the same scenario against the real 26.3 jars and against Ferraloom. Known differences are tracked in one list: [`docs/ferraloom/vanilla-client-gaps.md`](docs/ferraloom/vanilla-client-gaps.md).
- **Plugins:** early. Lifecycle, configuration, commands, the scheduler and the common player, block, world, entity and inventory events and calls work; about half of the methods of the API's main interfaces are implemented, and a call to one that is not fails loudly ([what a sample of popular plugins needs](docs/ferraloom/plugin-usage.md)). Server internals (NMS, CraftBukkit classes) are not provided. Test plugins before relying on them.
- **Multi-core ticking:** mobs and the network tick are parallel. Region-parallel ticking of a dimension is designed ([`docs/ferraloom/game-core.md`](docs/ferraloom/game-core.md)), not finished.
- **Releases:** none published yet. Build from source.

## Build

```sh
cargo build --release --locked -p horizon-client --no-default-features -p horizon-server
```

Hosting, `server.toml` and the game files the server needs: [`docs/hosting.md`](docs/hosting.md). Crate and binary names still carry the `horizon-` prefix: `horizon-server` is the supervisor and `horizon-client` is the server engine it runs.

Plugins need a JDK 25 and the bridge, built once with `python3 crates/ferraloom-plugins/java/build.py --install <server folder>`.

## Develop

Read [`AGENTS.md`](AGENTS.md) first. Vanilla is the specification: port from the decompiled 26.3 server, and when behavior is unclear, run vanilla and observe it.

```sh
python3 tools/pr_ready.py --quick     # fmt, clippy, touched crates' tests
python3 tools/parity.py --changed origin/main
python3 tools/plugins_e2e.py          # the plugin bridge, end to end
```

## License

MIT, as Horizon. See [`LICENSE`](LICENSE) and [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md). The plugin bridge is MIT too; the Bukkit API it implements is GPL-3.0 and is never redistributed here ([licensing notes](docs/ferraloom/plugins.md#licensing)). Thanks to Ludovic Marie and the Horizon contributors.
