# Architecture and completion contract

Ferraloom builds on a pinned Pumpkin core, with independent tests against the official Minecraft Java 26.3 server. Core, world, command, data and protocol modules are maintained locally so observed compatibility defects can be fixed in this repository. Other engine dependencies remain pinned to the shared upstream revision. Engine attribution stays visible in startup logs, dependency metadata, and this repository.

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

Supported barrel, chest, trapped-chest, sign and hanging-sign data commands update
retained live instances. Their state gate covers snapshot, edit and publication of
complete persistence NBT; client update tags are separate. Source providers resolve
before target locks, and world callbacks run after those locks are released. Sign
command callbacks use a text snapshot and run after releasing sign locks. Unknown
component forms retain their authored face on disk and refuse data edits before
interpreting paths. Complete block-entity and component support remains unfinished.

Player saves retain a previous `.dat_old`. Corrupt primary data is preserved
before a valid backup is durably restored without rotating it into the backup.
Primary IO errors reject admission rather than roll back to an older file.
Failed recovery ends login before a Player is constructed or published.
Snapshots reserve their sequence before capture; the storage lock rejects a
snapshot older than the last successful write before encoding or backup rotation.

An owned lease reserves each UUID before loading player data, advancements and statistics.
Replacement admission waits for the previous session's leave callbacks and all three final
saves, with a bounded wait and client-close cancellation. Cleanup removes the
exact Player instance. Advancement snapshots and atomic JSON writes currently
share a server-wide gate; a blocking writer retains that gate after cancellation,
and admission waits for pending writes before reading. Some command, chat and
respawn handlers still spawn server-owned work outside the client task tracker.
Complete packet ordering and draining all mutations before final saves remain
unfinished. Per-player parallel saves
and the existing chunk-coordinator lock latency remain performance work.

Statistics use Java's `players/stats/<uuid>.json`, with namespaced identifiers
separate from runtime numeric IDs. Capture and atomic replacement share a manager
gate, and admission waits for outstanding writes. Player NBT, advancements and
statistics each get a final save attempt even when another family fails. Current
DataVersion 5023 is required; unknown named statistics and JSON extensions are
retained without sending unknown numeric IDs to clients. Nonempty or malformed
private `Statistics` NBT from older Ferraloom builds refuses admission and saving.
There is no automatic migration or merging of private and canonical counters.

Fill-destroy commands release chunk guards before invoking gameplay callbacks.
Deferred neighbor notifications have their own hook, separate from immediate
block-entity, POI and multiblock cleanup. Final client updates use the resulting
world state after placement and neighbor callbacks.

Command storage belongs to the server and uses vanilla's namespaced gzip NBT
files. Namespace `.` resolves into the data directory itself. Legacy files are
read only when discovered as files, with modern files taking precedence. Saves
preserve unknown fields, attempt healthy namespaces after another fails, and
continue during save-off alongside other global saved data. Each save currently
rewrites every namespace; dirty tracking and semantic data fixes for older world
versions remain unfinished.

The official artifact is pinned by metadata URL, SHA-1, size, release, Java major, and protocol. The test tool verifies the download before launching it. Registry identifiers and datapack paths come from the exact official artifact and its built-in data generator. REA artifact/process evidence and supplementary JVM codec inspection are documented separately.

Reference tests use identical scenario inputs, fixed seeds, isolated worlds, offline authentication, and loopback listeners. A scenario must pass on both servers. Tests use explicit world predicates, packet observations, and restart checks. Results identify the tested Rust binary by SHA-256. These observations cover selected behavior, not every property in a feature family. Feature completion requires paired evidence for every required scenario and the strict ledger gate.

The separate live block-data check decodes menu content and slot packets, submits
actual sign edits, and checks assigned editors, nonoperator command effects and
fresh chunk state after restart. Supported rich-text clicks and literal submissions
have separate assertions. Watchdogs and process-exit checks turn hangs or forced
cleanup into failed observations.

The furnace continuation check creates an official-server save after exactly
237 stepped ticks, then uses fresh copies for both runtimes. It checks saved
timers before stepping, subsequent cooking progress, and an independent lava
bucket/wet sponge sequence. It records semantic differences separately from
canonical NBT tag widths, since Java accepts numeric tags of different widths.
Fixture hashes protect the original Java save from changes by either runtime.
The zero-step phase records saved-value preservation; its codec acceptance entry
is skipped because chunk-byte changes alone cannot prove re-encoding.

Plugin implementation starts after vanilla compatibility. Java plugin compatibility must be specified against concrete APIs and versions, such as Bukkit or Paper; `.jar` is a file format rather than a complete plugin contract. A future JVM bridge must preserve synchronous event ordering and world ownership while accommodating Java plugin threading assumptions. No plugin bridge, API compatibility claim, or experimental loader is included now.
