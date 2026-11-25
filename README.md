# Ferraloom

A Minecraft: Java Edition **26.3** server written in Rust (protocol 777). No JVM is needed to run it, and vanilla 26.3 clients join it as they join Mojang's server.

Ferraloom is a fork of [Horizon](https://github.com/horizon-rust/horizon), the Rust rewrite of Minecraft 26.3. Horizon is a whole game: client, launcher and server in one. Ferraloom keeps its vanilla logic and its differential harnesses, and adapts and optimizes it for one job: hosting servers.

> Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft. Ferraloom contains no Minecraft code, assets or data: you prepare the game files from your own legally obtained copy of Minecraft: Java Edition 26.3.

## What Ferraloom changes

**Server adaptation**

- The dedicated server is the product. The engine builds without rendering, audio or controller libraries (`--no-default-features`), and carving the remaining client code out of the server is ongoing.
- Vanilla clients are first-class: behavior a vanilla client can observe is checked against Mojang's server, not only against Horizon's own client.
- Spigot plugin compatibility is in progress (see [Status](#status)). Horizon's sandboxed WebAssembly server mods still work.
- Releases, updates and the install script follow this repository, not upstream.

**Optimizations**

Measured with 500 bot players (300 walking, 200 exploring) on a 4-core Xeon, with output identical to the serial code:

| | Mean tick | P99 |
|---|---|---|
| Horizon v0.2.93 base | 204.7 ms | |
| Ferraloom `main` | 64.8 ms | ~88 ms |

- Chunk integration is budgeted per tick and generation runs ahead of moving players, so exploring does not stall the tick.
- Mobs tick speculatively on a worker pool and commit in vanilla order; a mob whose inputs changed reruns serially.
- The network tick builds packets on the pool and serves sockets on epoll lanes.
- Per-player rescans, repeated block lookups and per-tick rebuilds are gone from the hot paths.

Every optimization must pass the parity harnesses unchanged. Design notes: [`docs/ferraloom/`](docs/ferraloom/).

## Status

- **Vanilla parity:** inherited from Horizon and checked by its differential harnesses (`python3 tools/parity.py`), which run the same scenario against the real 26.3 jars and against Ferraloom. Gaps seen from vanilla clients are being fixed.
- **Multi-core ticking:** mobs and the network tick are parallel. Region-parallel ticking of a dimension is designed ([`docs/ferraloom/game-core.md`](docs/ferraloom/game-core.md)), not finished.
- **Plugins:** Spigot plugin support is under development and not usable yet.
- **Releases:** none published yet. Build from source.

## Build

```sh
cargo build --release --locked -p horizon-client --no-default-features -p horizon-server
```

Hosting, `server.toml` and the game files the server needs: [`docs/hosting.md`](docs/hosting.md). Crate and binary names still carry the `horizon-` prefix to keep upstream merges cheap.

## Develop

Read [`AGENTS.md`](AGENTS.md) first. Vanilla is the specification: port from the decompiled 26.3 server, and when behavior is unclear, run vanilla and observe it.

```sh
python3 tools/pr_ready.py --quick     # fmt, clippy, touched crates' tests
python3 tools/parity.py --changed origin/main
```

## Upstream

Ferraloom started from Horizon v0.2.93 and merges upstream releases. Horizon's own guide for its code is [`docs/horizon-agents.md`](docs/horizon-agents.md). Thanks to Ludovic Marie and the Horizon contributors.

## License

MIT, as Horizon. See [`LICENSE`](LICENSE) and [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
