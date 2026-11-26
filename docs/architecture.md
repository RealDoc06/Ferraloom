# Architecture: feature → files → checks

The server's busiest areas, the files that hold them and the checks that cover them. The crate map, how `Game`
is organised and how to build and test are in [horizon-agents.md](horizon-agents.md).

Paths: `L/` = `crates/horizon-client/src/`, `G/` = `crates/horizon-client/src/game/`, `W/` =
`crates/horizon-world/src/`, `K/` = `crates/horizon-client-core/src/`, `C/` = `crates/horizon-command/src/`,
`I/` = `crates/horizon-items/src/`.
Checks: a bare name is a harness (`python3 tools/parity.py <name>`, script `tools/test_<name>_parity.py`).

For the full set of harnesses a file can affect, ask the runner:
`printf '%s\n' <paths> > /tmp/p.txt && python3 tools/parity.py --changed-files /tmp/p.txt --list`.

| Area | Files | Checks |
|---|---|---|
| Server loop and tick order | `L/game.rs` (`fn run`, `fn tick`, `fn tick_world`), `G/levels.rs` (`fn tick_server`), `G/dedicated.rs`, `L/clock.rs`, `L/spans.rs` | game_loop, tick, server_dimension_players |
| Mobs in the loop, spawning | `G/creatures.rs`, `G/natural_spawning.rs`, `L/mobs/spawning.rs`, `L/mobs/spawn_rules.rs`, `L/mobs/spawn_selection.rs`, `L/mobs/finalize.rs`, `G/spawner.rs`, `G/trial_spawner.rs` | natural_spawn, spawn_rules, finalize_spawn, spawner, trial_spawner, mob_ai |
| World save and load | `L/save.rs`, `G/persist.rs` (entities, scheduled ticks), `L/entity_nbt.rs`, `K/block_entity_nbt.rs`, `W/region.rs`, `W/nbt.rs`; game.rs `fn save_level` | save_roundtrip, world_reopen, world, datafix |
| World generation | `W/worldgen.rs`, `W/worldgen/` (`generator*.rs`, `feature_graph.rs`, `structure/`, `placement/`), `L/vanillagen.rs` | chunk_generation, worldgen, server_world_gen, world_spawn, feature_graph, feature_placement, decoration and the other worldgen harnesses |
| Chunk streaming, light, tickets | `L/streaming.rs`, `L/world.rs`, `L/chunk.rs`, `L/chunk_map.rs`, `L/light.rs`, `L/ticket_levels.rs`, `L/tick_chunk_membership.rs`, `G/world_loading.rs` | light, chunk_status, random_tick_sections; `cargo test -p horizon-client --test generation_saves` |
| Connections and play state | `L/lan/` (`conn.rs`, `hub.rs` + `hub/`, `io_pool.rs`, `chunks.rs`, `entity.rs`, `tracking.rs`, `guest.rs`, `playerdata.rs`), `G/lan.rs` | dedicated_server, server_entities, server_entity_data, hub_sync; `cargo test -p horizon-client --test lan_bot` |
| Players' actions on the server | `G/lan_actions.rs` (`fn as_guest`, `fn tick_guests`), `G/lan_{attributes,pvp,chat,crafting,stations,trading,mount_menu,waypoints}.rs` | server_player_combat, server_player_damage, server_player_targets, server_player_handlers, server_knockback |
| Protocol and packets | `crates/horizon-protocol/src/` (`packets/play.rs`, `entity_data.rs`, `connection.rs`) | dedicated_server, secure_chat, item_wire, stop_sound_wire; `cargo test -p horizon-protocol` |
| Dedicated server files, horizon-server | `G/dedicated.rs`, `G/dedicated/status.rs`, `crates/horizon-server/`, `crates/horizon-server-files/` | dedicated_server, server_files, user_cache, server_ops_feedback, server_resource_pack |
| Mob types and species | `L/mobs.rs`, `L/mobs/species.rs`, `L/mobs/data.rs`, `L/mobs/living.rs`, `L/mobs/drops.rs`, `L/mobs/gear.rs` | mob_attributes, entity_attribute, entity_death_loot, mob_ai |
| Mob AI | `L/mobs/ai.rs`, `goals.rs`, `brain.rs`, `*_brain.rs`, `nav.rs`, `path.rs`, `control.rs` | mob_ai |
| Items and item use | `I/items.rs`, `I/item_data.rs`, `G/hand.rs`, `G/consume.rs`, `G/potions.rs`, `L/effects.rs`; game.rs `fn use_hand`, `fn use_item` | use_item_on, item_use, item_use_lifecycle, consumable, consume_effect, status_effect, server_items |
| Command execution | `G/commands.rs`, `G/commands/*.rs`, `L/scoreboard_cmd.rs`, `L/data_cmd.rs`, `K/scoreboard.rs`, `G/functions.rs` | command_exec (corpus `tools/command_exec_corpus.py`), scoreboard, server_ops_feedback, server_function_permission |
| Command parsing | `C/` (`args.rs`, `selector.rs`, `tree_*.rs`, `vanilla.rs`) | command_parse (corpus `tools/command_parse_corpus.py`); `cargo test -p horizon-command` |
| Block behaviour | `W/level/` (`mod.rs`, `tick.rs`, `hooks.rs`, `data.rs`, `redstone.rs`, `place.rs`), `G/block_entity_ticks.rs` | block_update, redstone, random_tick, simple_block, piston_movement, block_entity_tickers |
| Player movement and collision | `L/motion.rs` + `L/motion/`, `L/collisions.rs`, `W/entity_move.rs`, `W/player_travel.rs`, `W/collision.rs` | collision, entity_move, player_travel, living_motion, climbing, swimming |
| Container menus and inventory | `L/menu.rs`, `L/containers.rs`, `L/lan/menu.rs`, `L/lan/containers.rs`; game.rs `fn menu_ops`, `fn click_inventory` | menu_click, player_inventory, simple_container |
| Workstations, crafting | `L/stations.rs`, `G/stations.rs`, `I/enchant.rs`, `L/brewing.rs`, `L/crafting.rs`, `L/recipe_book.rs`, `G/recipe_book.rs`, `G/machines.rs`, `G/machines/crafter.rs` | enchanting, recipe, recipe_book, place_recipe, crafter |
| Vehicles and mounts | `G/boats.rs`, `G/boat_passengers.rs`, `G/minecarts.rs`, `G/mounts.rs`, `G/mount_menu.rs`, `G/llamas.rs`, `G/passengers.rs` | server_entities |
| Villagers, trading, raids | `L/mobs/villager*.rs`, `L/merchant.rs`, `G/trading.rs`, `G/villagers.rs`, `L/raid.rs`, `G/raids.rs`, `L/gossip.rs` | trading, raid, mob_ai |
| Bosses and the End | `G/dragon.rs`, `G/wither.rs`, `G/end.rs` | server_dragon, forced_end, end_podium, end_gateway |
| Dispensers | `G/dispenser.rs`, `G/dispense_fixture.rs` (the harness side) | dispenser |
| Explosions and TNT | `G/explosion.rs`, `G/tnt.rs` | no dedicated harness yet |
| Plugins | `crates/ferraloom-plugins/`, `G/plugins.rs` + `G/plugins/` | `python3 tools/plugins_e2e.py` |
| Mods, modpack porting | `crates/horizon-mods/`, `G/mods.rs`, `G/server_mods.rs`, `L/mod_content.rs`, `crates/horizon-port-pack/`, `tools/port_*.py` | `cargo test -p horizon-mods`, `cargo test -p horizon-port-pack`; `python3 tools/mod_port_harness.py` |
| CI and release | `.github/workflows/` (`ci.yml`, `parity.yml`, `release.yml`, `game-build.yml`), `tools/ci/`, `tools/parity.py`, `tools/pr_ready.py` | `python3 -m unittest discover -s tests`; actionlint (`.github/actionlint.yaml`) |
