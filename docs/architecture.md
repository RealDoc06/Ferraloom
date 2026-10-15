# Architecture and completion contract

Ferraloom builds on a pinned Pumpkin core, with independent tests against the official Minecraft Java 26.3 server. Core, world, and command modules are maintained locally so observed compatibility defects can be fixed in this repository. Other engine dependencies remain pinned to the shared upstream revision. Engine attribution stays visible in startup logs, dependency metadata, and this repository.

The runtime initializes named Tokio network workers and Rayon CPU workers. The engine parallelizes world, player, entity, and chunk work, with a ticker coordinating simulation phases. Chunk generation has additional engine-owned pools. `--workers` describes the two configurable pools, not a process-wide thread limit. Correctness under different schedules and measured throughput remain separate acceptance gates.

Each process owns its data directory. The wrapper reads configuration strictly, keeps online authentication enabled by default, disables telemetry, and supplies no plugin loaders. It handles SIGINT/SIGTERM through the engine's graceful shutdown, and propagates engine panics as failed process exits. It does not translate vanilla `server.properties`; operational configuration uses `pumpkin.toml`.

The official artifact is pinned by metadata URL, SHA-1, size, release, Java major, and protocol. The test tool verifies the download before launching it. Registry identifiers and datapack paths come from the exact official artifact and its built-in data generator. REA artifact/process evidence and supplementary JVM codec inspection are documented separately.

Reference tests use identical scenario inputs, fixed seeds, isolated worlds, offline authentication, and loopback listeners. A scenario must pass on both servers. Tests use explicit world predicates, packet observations, and restart checks. Results identify the tested Rust binary by SHA-256. These observations cover selected behavior, not every property in a feature family. Feature completion requires paired evidence for every required scenario and the strict ledger gate.

Plugin implementation starts after vanilla compatibility. Java plugin compatibility must be specified against concrete APIs and versions, such as Bukkit or Paper; `.jar` is a file format rather than a complete plugin contract. A future JVM bridge must preserve synchronous event ordering and world ownership while accommodating Java plugin threading assumptions. No plugin bridge, API compatibility claim, or experimental loader is included now.
