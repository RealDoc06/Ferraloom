# Ferraloom: a map for agents

A Minecraft: Java Edition 26.3 dedicated server in Rust, built on Horizon's engine. Vanilla is the specification.
Rules and workflow: [AGENTS.md](../AGENTS.md) and [CONTRIBUTING.md](../CONTRIBUTING.md); this page says where things
are. Feature → files → harness for the busiest areas: [architecture.md](architecture.md).

## Rules that bite

- **Never commit Mojang's files**: game jars, assets, decompiled source, prepared resources (`.local/`).
  `tools/check_public_tree.py` rejects them; `git config core.hooksPath .githooks` runs it on commit.
- **Keep runs away from real player data.** Every engine a tool starts gets fresh scratch folders in
  `HORIZON_HOME` (the engine's home: logs, `horizon-extras.txt`), `XDG_DATA_HOME` and `XDG_CONFIG_HOME`
  through `lab.game_env` (`tests/test_game_home.py` enforces it). A dedicated server writes its world, logs and
  vanilla files in its working directory.
- The names are Horizon's: crates `horizon-*`, the engine binary `horizon-client`, env vars `HORIZON_*`.

## Crates

One workspace (root `Cargo.toml`).

| Crate | Owns |
|---|---|
| `horizon-client` | The engine: a lib (`src/lib.rs`, flat modules) and the bin (`src/main.rs` → `game.rs` + `game/`). Runs only as `--dedicated`, or as a component fixture (`HORIZON_DISPENSE_FIXTURE`, `HORIZON_SCULK_FIXTURE`) |
| `horizon-server` | `horizon-server`: `server.toml`, the CLI, the web panel (`panel/`), backups, updates, and the supervisor that runs the engine ([hosting.md](hosting.md)) |
| `horizon-server-files` | `server.properties`, `eula.txt`, ops/whitelist/bans, the user cache. Re-exported as `horizon_client::server` |
| `horizon-world` | Vanilla world data and rules below the game loop: NBT, regions, chunks, `datafix/`, `worldgen/`, `level/` (block behaviour, ticks, redstone), entity physics, items, effects, food, damage |
| `horizon-core` | Simulation primitives: Java `Random`s, `jmath` (HotSpot-exact `Math`), positions, noise, density, paletted storage, `version.rs` (the pinned version) |
| `horizon-protocol` | Network protocol 777: codecs, framing, encryption, `packets/{handshake,status,login,configuration,play}.rs`, `entity_data.rs` |
| `horizon-command` | Command parsing (Brigadier): the vanilla tree, argument types, selectors, suggestions, the tree's network form. Re-exported as `horizon_client::command` |
| `horizon-items` | Block facts, item stacks and data components, enchantments, cooldowns, bundles, items in NBT. Re-exported as `horizon_client::items` |
| `horizon-client-core` | The lib's bottom layer: `text.rs`, `lang.rs`, `snbt.rs`, `nbt_path.rs`, `component.rs`, `scoreboard.rs`, `bossbar.rs`, `gamerules.rs`, `block_entity_nbt.rs`, registry tables |
| `horizon-assets` | Block-state and item model decoding from the prepared resources |
| `horizon-prepare` | Builds `.local/horizon-client/26.3` from the host's own Minecraft files (runs the Java exporters) |
| `horizon-threads` | Thread priorities and affinity the forbid-unsafe crates can't set |
| `ferraloom-core`, `ferraloom-plugins` | Ferraloom's own: server execution policies (chunk integration limits), and Bukkit plugins through an in-process JVM ([ferraloom/plugins.md](ferraloom/plugins.md)) |
| `horizon-mods`, `horizon-mod-sdk`, `cargo-horizon-mod` | Sandboxed WebAssembly mods: runtime, SDK + WIT, `cargo horizon-mod` ([mods.md](mods.md)) |
| `horizon-port-pack` | Porting the data-driven content of Java modpacks ([modpacks.md](modpacks.md)) |
| `horizon-shader-api` | Horizon's WGSL shader-pack format and its check; only the mod index's optional `shader-packs` feature uses it |
| `horizon-bench`, `horizon-telemetry` | The benchmark report format; `horizon-server` uses its process CPU and RSS readers |
| `horizon-bots`, `horizon-release-sign`, `xtask` | Load-test bots, signed release checksums, `package-server` |

Other top-level folders: `tools/` (harnesses and dev scripts), `tests/` (Python unit tests: `python3 -m unittest
discover -s tests`), `mods/horizon/src/test/java/dev/horizonlab/parity/` (Java oracles), `packaging/server/`
(installers, Dockerfile), `benchmarks/`, `docs/`, `.github/workflows/`.

## Build and test

Builds run on the builder through `ferraloom-build` from a committed checkout ([AGENTS.md](../AGENTS.md)).

```sh
cargo build --release -p horizon-client -p horizon-server   # the engine and its supervisor
cargo test -p horizon-client --lib --bins --tests          # skips the probes in examples/
python3 tools/pr_ready.py --quick                          # fmt, clippy -D warnings, touched crates' tests; lists harnesses
python3 tools/parity.py --changed-files paths.txt --list   # which harnesses cover these files
python3 tools/parity.py dedicated_server mob_ai            # named harnesses
python3 tools/plugins_e2e.py                               # the plugin bridge with a scripted player
```

- Harnesses need the vanilla data in `.local/` (`minecraft/`, `horizon-client/26.3`, `datagen/`): see
  [parity-workflow.md](parity-workflow.md).
- A harness = `tools/test_<name>_parity.py` (about 300) + a Java oracle
  (`mods/horizon/src/test/java/dev/horizonlab/parity/Vanilla*Oracle.java`) + a Rust probe: a module of
  `crates/horizon-world/examples/probe/` (the `world_probe` example) or `crates/horizon-client/examples/probe/`
  (`client_probe`), or the engine itself: `--dedicated` against vanilla's server with a protocol client
  (`dedicated_server`, `server_*`, `command_exec`), or a fixture env var (`game/dispense_fixture.rs`,
  `game/sculk_fixture.rs`). Copy a nearby harness for a new one.
- Probes build into `.local/parity-target/`; harnesses find binaries through `lab.cargo_target()`, never `target/`.
- Rust integration tests: `crates/horizon-client/tests/` (`lan_bot.rs`, `generation_saves.rs`, `wide_states.rs`).
- `tools/client_rig/rig` runs the real vanilla client headless on the builder against a Ferraloom server
  ([ferraloom/client-rig.md](ferraloom/client-rig.md)).

## Vanilla reference

- Decompiled sources and the vanilla jar live in the main checkout's `.cache/` (paths in [AGENTS.md](../AGENTS.md)).
- Bytecode queries (callers through lambdas, every overload):
  `python3 tools/rea_vanilla.py method net.minecraft.world.level.block.DispenserBlock dispenseFrom`
  ([decompilation-status.md](decompilation-status.md)).
- A harness runs the oracle against the real 26.3 jar and the Rust probe on the same inputs, then compares their
  JSON. Oracle runs are cached by `tools/parity_shim.py`; `--fresh` reruns them.
- Mirror vanilla's `Math` with `horizon_core::jmath` and its RNG with `horizon_core::random`: draws happen in
  vanilla's order.

## horizon-client: where things live

`L:` = lib (`crates/horizon-client/src/`), `G:` = bin (`crates/horizon-client/src/game/`). The lib can't see
`Game`; bin code reaches the lib as `horizon_client::<module>`. Some lib modules still carry client-side state
the carve left (`hud.rs`, `mesher.rs`, `particles.rs`...); the server does not draw them.

| Subsystem | Files |
|---|---|
| Start, options, server loop | `main.rs` (src/) → game.rs `fn run`, `Options::read`; G: `dedicated.rs` (`fn run_dedicated`, server.properties, console), `dedicated/status.rs`; `levels.rs` (`fn tick_server`: one level per dimension); game.rs `fn tick`, `fn tick_world`; L: `clock.rs`, `spans.rs`, `profiler.rs` |
| Players and connections | L: `lan/` (`conn.rs` handshake and login, `hub.rs` + `hub/` play state, `io_pool.rs`, `chunks.rs`, `entity.rs`, `tracking.rs`, `guest.rs`, `playerdata.rs`); G: `lan.rs` (`LanHost`), `lan_actions.rs` (`fn as_guest`, `fn tick_guests`), `lan_*.rs` per feature, `lan_entities.rs` |
| Chunks, streaming, light | L: `streaming.rs` (worker pool: generate, light), `world.rs`, `chunk.rs`, `chunk_map.rs`, `light.rs`, `vanillagen.rs` (binds horizon-world's generator), `ticket_levels.rs`, `pregen.rs`; G: `world_loading.rs`, `postprocess.rs`; game.rs `fn update_world` |
| World generation | `crates/horizon-world/src/worldgen/` (`generator*.rs`, `feature_graph.rs`, `structure/`, `carver/`) |
| Block behaviour | `crates/horizon-world/src/level/` (`mod.rs`, `tick.rs`, `hooks.rs`, `redstone.rs`, `place.rs`, `interact.rs`); game.rs `fn tick_blocks`, `fn process_level_events` |
| Mobs | L: `mobs.rs`, `mobs/` (`ai.rs`, `goals.rs`, `brain.rs`, `*_brain.rs`, `nav.rs`, `path.rs`, `species.rs`, `spawning.rs`); G: `creatures.rs`, one file per mob family (`dragon.rs`, `wither.rs`, `villagers.rs`, `piglins.rs`, `golems.rs`...), `natural_spawning.rs` |
| Other entities | L: `entities.rs`, `projectiles.rs`, `arrows.rs`; G: `boats.rs`, `minecarts.rs`, `mounts.rs`, `armor_stands.rs`, `paintings.rs`, `frames.rs`, `fireworks.rs`, `tnt.rs`, `explosion.rs` |
| Player actions | game.rs (`fn use_hand`, `fn use_item`, `fn attack`, `fn tick_mining`, `fn break_block`), run for a player through `as_guest`; G: `hand.rs`, `combat.rs`, `consume.rs`, `potions.rs`, `shield.rs`, `crossbow.rs`, `trident.rs`, `fishing.rs`; L: `motion.rs` + `motion/`, `collisions.rs` |
| Inventory and containers | L: `menu.rs` (`AbstractContainerMenu.clicked`), `containers.rs`, `crafting.rs`, `recipe_book.rs`, `stations.rs`, `merchant.rs`; `lan/menu.rs`, `lan/containers.rs`; G: `stations.rs`, `trading.rs`, `recipe_book.rs`, `machines.rs` |
| Commands | parse: crate `horizon-command`; run: G: `commands.rs` + `commands/*.rs` (`admin`, `execute`, `items`, `players`, `server_cmds`, `world_cmds`...), `functions.rs`, `command_blocks.rs`; L: `scoreboard_cmd.rs`, `data_cmd.rs` |
| Save and load | L: `save.rs` (level.dat, regions, playerdata), `entity_nbt.rs`; G: `persist.rs` (entities, scheduled ticks); game.rs `fn save_level`; `crates/horizon-world/src/{region.rs,nbt.rs,datafix/}` |
| Plugins and mods | crate `ferraloom-plugins`; G: `plugins.rs` + `plugins/`; crate `horizon-mods`; G: `mods.rs`, `server_mods.rs`; L: `mod_content.rs`, `mod_list.rs` |
| Supervisor link | L: `control.rs` (the control socket `--control-socket`); G: `agent_settings.rs` |

## How `Game` is organised

- `struct Game` (game.rs) holds everything. Its methods are spread over ~180 files: game.rs keeps the core (tick,
  inventory, world open and close) and each `game/<feature>.rs` adds an `impl Game<'_>` block for one feature,
  with its own state types. Fields are private to the bin.
- Find a feature's code: `grep -rn "fn tick_<thing>\|fn <verb>_<thing>" crates/horizon-client/src/game*`; the file
  is usually `game/<thing>s.rs`. Who calls it: grep the name in game.rs's `fn tick` and `fn tick_world`.
- A dedicated server is one `Game` with no local player, configured from server.properties. Its players are
  guests of the play-state hub (`lan/hub.rs`): `as_guest` swaps a guest's state (position, inventory, mode,
  mining, item use) into `Game` for one call (`game/lan_actions.rs`), and `levels.rs` swaps whole dimensions the
  same way. Changes reach clients through the block log, level events and the entity tracker (`lan/tracking.rs`,
  `game/lan_entities.rs`).
- Shared read-only data: `Resources` (`res`: blocks, items, recipes, mob types, biomes) and `PackResources`
  (`pack`), both in game.rs.

## Docs worth knowing

[parity-workflow.md](parity-workflow.md) (runner, gate, shards, scopes) ·
[server-levels.md](server-levels.md) (levels per dimension) · [hosting.md](hosting.md) ·
[rust-parity.md](rust-parity.md) (what is ported) · [known-issues.md](known-issues.md) ·
[ferraloom/game-core.md](ferraloom/game-core.md) · [README.md](README.md) (index).
