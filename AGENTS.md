# Ferraloom development

Ferraloom is a Minecraft Java Edition 26.3 server in Rust (protocol 777, data
version 5023), built on the dedicated server of
[Horizon](https://github.com/horizon-rust/horizon). The goal is unchanged:
everything vanilla does works, the world ticks across many cores, chunk
generation and loading never cost TPS, and plugins run on top. Java `.jar`
plugin support still comes only after vanilla parity.

## Horizon base

- This branch starts from Horizon v0.2.93 (`b6c936f8`) with its full history.
  The `horizon` remote is upstream: merge its releases, never rebase over them,
  and name the upstream tag in the merge commit.
- Horizon is MIT. Keep its `LICENSE` and `THIRD_PARTY_NOTICES.md`.
- Ferraloom is the server only. Horizon's dedicated server is its game binary
  run headless: the `horizon-server` supervisor starts `horizon-client
  --dedicated`, and the one `Game` in `crates/horizon-client/src/game.rs` holds
  client and server state together. Carving the server out of the client is a
  standing goal: remove or feature-gate client-only crates and code
  (rendering, audio, input, HUD, launcher, shaders, client mods) and never add
  new client coupling. Until that is done, build the engine with
  `--no-default-features` (no audio or controller libraries).
- Where things are: `docs/horizon-agents.md` (upstream's agent map: crates,
  files, harnesses), `docs/server.md`, `docs/server-levels.md`, `docs/mods.md`.
- The pre-fork Ferraloom port (branch `restart`, crates `ferraloom-*`) is
  reference material. Port a piece from it only where Horizon lacks a feature
  or gets it wrong, and bring a harness with it. Its design notes on threading
  and chunk scheduling are in `docs/ferraloom/`; they name the old crates, but
  their analysis of vanilla still holds.

## What Ferraloom adds

- **Chunk generation and loading that keep 20 TPS.** Horizon generates on a
  worker pool but integrates every finished chunk on the tick thread with no
  time budget (`crates/horizon-client/src/streaming.rs`, `Streaming::update`),
  so exploring players drop it under 10 TPS. Budget main-thread work, move what
  can move off it, and generate ahead of where players travel.
- **Multi-core ticking.** Horizon ticks every dimension on one thread by
  swapping each level into the one `Game` (`game/levels.rs`, `tick_server`).
  Ferraloom's design: independent regions of a dimension tick in parallel
  between vanilla's phase barriers, anything crossing regions parks for an
  exclusive lane, and single-region mode stays the exact vanilla reference
  (`docs/ferraloom/game-core.md`).
- **Plugins.** Horizon has sandboxed WebAssembly server mods (API 0.3,
  `docs/mods.md`). Ferraloom's plugin API grows from there.
- **Server performance in general**, measured, never guessed.

## Vanilla is the spec

- Port behavior from the decompiled server in `.cache/decompiled/26.3/`
  (`tools/decompile.sh` on branch `restart` regenerates it; Horizon's
  `tools/rea_vanilla.py` queries the bytecode). The 26.3 jar is unobfuscated,
  so class and method names are Mojang's own. Use REA (`npx -y rea-agents`)
  when decompiled code is not enough.
- Never commit decompiled Java, game jars, prepared resources (`.local/`) or
  other Mojang files. `tools/check_public_tree.py` rejects them.
- Mirror vanilla's maths and RNG with `horizon_core::jmath` and
  `horizon_core::random`. Keep vanilla's names for ported concepts and put a
  short doc comment on ported items naming the source, for example
  `/// Port of ServerLevel#tickChunk.`
- When behavior is unclear, run the vanilla server and observe it. Do not guess.
- Shared caches live only in the main checkout, never in worktrees: decompiled
  source `/home/doc/code/Ferraloom/.cache/decompiled/26.3/`, vanilla jar,
  libraries and reports `/home/doc/code/Ferraloom/.cache/vanilla/26.3/`, JDK 25
  `/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1/`.

## Tests

- Horizon's differential harnesses are the parity suite: `python3
  tools/parity.py <name>`, and `tools/parity.py --changed-files <list> --list`
  names the harnesses a change can affect. A change that can alter gameplay
  passes them before it merges. Multi-core work passes them in single-region
  and parallel-region modes.
- Unit tests live next to the code. Tests must be focused: no smoke-test spam,
  receipts, evidence ledgers or hash-pinned logs. A test passes or it fails.

## Performance

- Measure with `tools/server_bench.py` and `horizon-bots` against vanilla and
  Paper on equal work, with the bots on another machine. Report MSPT mean, P95
  and P99, TPS, chunks delivered, fully generated chunks (region files), players
  pulled back, CPU and the busiest thread. Compare against the latest baseline,
  not against memory.
- The bench server is `vm-minecraft` (10.10.10.101, user `doc`,
  `~/horizon-bench`, servers on port 25570). Its Velocity proxy on port 25565 is
  live: never touch it or other users' processes. Bots run on Doc's Windows
  laptop through the relay in `~/horizon-bench/relay_*.py`.

## Builds

Devbox is for editing and light work. Run Rust builds and tests on the
dedicated builder through the wrapper, from a clean, committed checkout:

```sh
ferraloom-build -- cargo build --release --locked -p horizon-client --no-default-features -p horizon-server
ferraloom-build -- cargo test --locked -p <crate>
```

The wrapper (`/home/doc/.local/bin/ferraloom-build`, config
`/home/doc/.config/ferraloom-build.json`) syncs your exact `HEAD` to the builder
(`ferraloom-builder`, CT107 on nodo-3) and runs the command there. Jobs queue
and run one at a time with a shared `target` directory. Commit before building,
because uncommitted edits are never sent. Do not create per-worktree target
directories, and do not edit the builder checkout directly.

Do not alter Proxmox, other containers or live services.

## Git

- Conventional commits, for example `perf(streaming): budget chunk integration per tick`.
- Keep commits small and focused. Work on your own branch; the parent agent
  merges it.
- Keep upstream merges cheap: prefer new modules over rewrites of busy upstream
  files, and keep edits to upstream code small and commented.
- Add external dependencies in your crate's own `Cargo.toml`, not in
  `[workspace.dependencies]`, so parallel branches merge cleanly.
- Stay inside the paths your brief assigns you. Ask the parent before touching
  anything else.

## Delegation

Delegated agents send implementation, architecture and scope questions to their
parent agent through agent or thread messages. Do not open user-question UI for
these decisions. The parent answers pending child requests and owns routine
project decisions; only the parent may escalate essential questions to Doc.
Repeat this routing rule in every delegation brief.
