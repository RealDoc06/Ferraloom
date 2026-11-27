# Ferraloom documentation

Ferraloom is a Minecraft Java Edition 26.3 server in Rust, built on Horizon's dedicated server. These pages are for
developers and hosts.

## Ferraloom

- [Server core](ferraloom/core.md), [game core](ferraloom/game-core.md): the server loop, ownership and multi-core
  ticking design.
- Chunks: [synchronous acquisition](ferraloom/synchronous-chunk-acquisition.md),
  [parallel generation](ferraloom/parallel-chunk-generation.md).
- Plugins: [Bukkit plugins](ferraloom/plugins.md), [blocks and worlds](ferraloom/plugins-blocks-worlds.md),
  [what real plugins use](ferraloom/plugin-usage.md).
- [Vanilla client gaps](ferraloom/vanilla-client-gaps.md): what a vanilla client sees differently.

## Build and run

- [AGENTS.md](../AGENTS.md) and [Architecture](architecture.md): where each part of the code lives, how `Game` is
  organised, which harness checks which area.
- [Host a server](hosting.md): install `horizon-server`, prepare the game files, first run, `server.toml`, the
  commands, the web panel, backups, services and Docker.
- [Hosting a server: design](server.md): `horizon-server`'s design and plan, and the bot benchmark against vanilla.
- [Server levels](server-levels.md): one `ServerLevel` per dimension, and how players move between them.
- [Mods](mods.md): sandboxed WebAssembly mods; server mods are API 0.3.
- [Modpacks](modpacks.md): porting the data-driven content of Java packs (`horizon-port-pack`).
- [Horizon APIs](api/README.md): mods, servers and data formats, each with its guide, generated reference,
  stability and changelog, and the [stability and versioning policy](api/stability.md).
- [Release flow](release-flow.md) and [versioning](versioning.md): how a change gets from a pull request to a release.
- [Profiling](profiling.md).

## Parity

- [Rust rewrite status](rust-parity.md): completion criteria, how to reproduce the validation, and per-area scope.
- [Parity workflow](parity-workflow.md): `tools/parity.py`, the gate, other machines, merging.
- Area notes: [block behaviours and ticking](parity/blocks.md).
- [Version ports](version-ports.md): how a new Minecraft release is detected, diffed against the pinned one and
  ported, and what stays manual.
- [Decompilation and research lab](research-lab.md) and [decompilation status](decompilation-status.md): the local
  game inspection workspace the harnesses build on.

## Architecture: world generation

- [World generation](world-generation.md): the pipeline from noise to features.
- [Pregeneration](pregeneration.md): `/pregen`, generating, lighting and saving an area ahead of the players.
- [Noise generation](noise-generation.md), [compiled density functions](density-functions.md),
  [structure density](structure-density.md), [terrain filling](terrain-filling.md).
- Biomes: [lookup](biome-lookup.md), [temperatures](biome-temperatures.md).
- Surfaces: [material surfaces](material-surfaces.md), [material conditions](material-conditions.md),
  [rule libraries](material-rule-libraries.md), [vertical anchors](vertical-anchors.md),
  [value providers](value-providers.md).
- [Carving](carving.md), [heightmaps](heightmaps.md), [feature placement](feature-placement.md).

## Architecture: world data and simulation

- [World data](world-data.md): chunk storage, palettes, region files.
- [Block-state codecs](block-state-codecs.md), [scheduled ticks](scheduled-ticks.md).
- [Collision geometry](collision-geometry.md): shapes, collision, movement and fluids.
