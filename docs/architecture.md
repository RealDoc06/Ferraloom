# Architecture and completion contract

Ferraloom builds on a pinned Pumpkin core, with independent tests against the official Minecraft Java 26.3 server. Core, world, and command modules are maintained locally so observed compatibility defects can be fixed in this repository. Other engine dependencies remain pinned to the shared upstream revision. Engine attribution stays visible in startup logs, dependency metadata, and this repository.

The runtime initializes named Tokio network workers and Rayon CPU workers. The engine parallelizes world, player, entity, and chunk work, with a ticker coordinating simulation phases. Chunk generation has additional engine-owned pools. `--workers` describes the two configurable pools, not a process-wide thread limit. Correctness under different schedules and measured throughput remain separate acceptance gates.

Each process owns its data directory and holds the world's `session.lock`; Unix record locks interoperate with Java's world lock. The wrapper reads configuration strictly, keeps online authentication enabled by default, disables telemetry, and supplies no plugin loaders. It handles SIGINT/SIGTERM/SIGHUP through the engine's graceful shutdown, and propagates engine panics as failed process exits. It does not translate vanilla `server.properties`; operational configuration uses `pumpkin.toml`.

Live block entities belong to World, while their last serialized NBT belongs to
the terrain chunk. Before the scheduler can evict that chunk, a coordinator
requests a World snapshot tied to its exact publication. World serializes and
removes block entities only after external handles are gone. A later getter or
mutation invalidates the prepared snapshot; the scheduler checks current
tickets again before unpublishing. Serialization runs outside the publication
write lock. Watch references alone do not guarantee terrain residency, which
matters for spectators receiving existing chunks without generation tickets.
Ordinary terrain eviction pauses during save-off; explicit saves and shutdown
still persist world state. Entity storage processes its unloads independently.

Player saves retain a previous `.dat_old`. A failed primary load can recover
from that backup only after preserving the failed bytes and durably restoring
the valid copy without rotating it into the backup. If recovery fails, login
ends before a Player is constructed or published, preventing a default player
from overwriting unreadable data. The player storage lock orders file
replacements; ordering previously captured asynchronous player snapshots is
separate work.

The official artifact is pinned by metadata URL, SHA-1, size, release, Java major, and protocol. The test tool verifies the download before launching it. Registry identifiers and datapack paths come from the exact official artifact and its built-in data generator. REA artifact/process evidence and supplementary JVM codec inspection are documented separately.

Reference tests use identical scenario inputs, fixed seeds, isolated worlds, offline authentication, and loopback listeners. A scenario must pass on both servers. Tests use explicit world predicates, packet observations, and restart checks. Results identify the tested Rust binary by SHA-256. These observations cover selected behavior, not every property in a feature family. Feature completion requires paired evidence for every required scenario and the strict ledger gate.

The furnace continuation check creates an official-server save after exactly
237 stepped ticks, then uses fresh copies for both runtimes. It checks saved
timers before stepping, subsequent cooking progress, and an independent lava
bucket/wet sponge sequence. It records semantic differences separately from
canonical NBT tag widths, since Java accepts numeric tags of different widths.
Fixture hashes protect the original Java save from changes by either runtime.

Plugin implementation starts after vanilla compatibility. Java plugin compatibility must be specified against concrete APIs and versions, such as Bukkit or Paper; `.jar` is a file format rather than a complete plugin contract. A future JVM bridge must preserve synchronous event ordering and world ownership while accommodating Java plugin threading assumptions. No plugin bridge, API compatibility claim, or experimental loader is included now.
