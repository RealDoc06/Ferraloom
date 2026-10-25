# Vanilla feature inventory

Minecraft Java dedicated server 26.3 (protocol 777, data version 5023). Use the linked files as the porting checklist: stable IDs identify concrete behaviors or registry entries, Vanilla source identifies the implementation or built-in resource, and Test links the focused parity scenario. An empty Test cell, Vanilla `-` and Ferraloom `todo` mean untested, regardless of existing Rust code. Run each scenario against vanilla before recording the Ferraloom result; use `pass`, `partial` or `fail` for measured results. Ferraloom passes require agreement in single-region and parallel-region modes. Optional bundled datapacks and integrated-only command entries are labeled. Registry rows were extracted from the official reports and bootstrapped runtime; resource rows enumerate the bundled datapack; mechanics rows come from reading the decompiled source. Run `python3 features/counts.py` after edits, or add `--check` to validate the index.

| Area | Rows | Ferraloom todo | Ferraloom partial | Ferraloom pass | Ferraloom fail |
|---|---:|---:|---:|---:|---:|
| [Advancements](features/advancements.md) | 1957 | 4 | 1942 | 11 | 0 |
| [Block entities](features/block-entities.md) | 132 | 89 | 41 | 2 | 0 |
| [Blocks](features/blocks.md) | 434 | 269 | 120 | 45 | 0 |
| [Chat and text components](features/chat.md) | 23 | 21 | 2 | 0 | 0 |
| [Chunks and tickets](features/chunks.md) | 54 | 20 | 33 | 1 | 0 |
| [Combat and damage](features/combat-damage.md) | 204 | 204 | 0 | 0 | 0 |
| [Commands and argument types](features/commands.md) | 2671 | 2385 | 35 | 251 | 0 |
| [Crafting and recipes](features/crafting-recipes.md) | 2137 | 2 | 29 | 2106 | 0 |
| [Datapacks and resource reload](features/datapacks-reload.md) | 57 | 57 | 0 | 0 | 0 |
| [Enchantments](features/enchantments.md) | 137 | 108 | 29 | 0 | 0 |
| [Entities and AI](features/entities.md) | 770 | 670 | 88 | 12 | 0 |
| [Fluids](features/fluids.md) | 32 | 25 | 6 | 1 | 0 |
| [Binary data, geometry and randomness](features/foundations.md) | 64 | 18 | 18 | 28 | 0 |
| [Functions and macros](features/functions-macros.md) | 10 | 10 | 0 | 0 | 0 |
| [Game events and vibrations](features/game-events.md) | 7 | 2 | 0 | 5 | 0 |
| [Game rules](features/game-rules.md) | 75 | 1 | 5 | 69 | 0 |
| [GameTest](features/gametest.md) | 23 | 23 | 0 | 0 | 0 |
| [Inventory and menus](features/inventory-menus.md) | 63 | 12 | 51 | 0 | 0 |
| [Item modifiers](features/item-modifiers.md) | 4 | 1 | 3 | 0 | 0 |
| [Items and data components](features/items.md) | 254 | 105 | 146 | 3 | 0 |
| [Lighting](features/lighting.md) | 15 | 2 | 2 | 11 | 0 |
| [Loot](features/loot.md) | 1671 | 158 | 1513 | 0 | 0 |
| [Management JSON-RPC](features/management.md) | 101 | 101 | 0 | 0 | 0 |
| [Maps, decoration and variants](features/maps-decoration.md) | 255 | 255 | 0 | 0 | 0 |
| [Networking and login](features/networking-login.md) | 46 | 10 | 36 | 0 | 0 |
| [Patrols](features/patrols.md) | 4 | 4 | 0 | 0 | 0 |
| [Permissions](features/permissions.md) | 7 | 7 | 0 | 0 | 0 |
| [Phantoms](features/phantoms.md) | 3 | 3 | 0 | 0 | 0 |
| [Player mechanics](features/player.md) | 80 | 26 | 53 | 1 | 0 |
| [Portals and dimension travel](features/portals-travel.md) | 13 | 13 | 0 | 0 | 0 |
| [Predicates](features/predicates.md) | 101 | 79 | 22 | 0 | 0 |
| [Protocol packets](features/protocol.md) | 271 | 47 | 1 | 223 | 0 |
| [Raids](features/raids.md) | 8 | 8 | 0 | 0 | 0 |
| [Redstone mechanics](features/redstone.md) | 66 | 12 | 51 | 3 | 0 |
| [Scoreboard, teams and bossbars](features/scoreboard-teams-bossbars.md) | 78 | 4 | 17 | 57 | 0 |
| [Server operations](features/server-operations.md) | 111 | 96 | 15 | 0 | 0 |
| [Sounds and particles](features/sounds-particles.md) | 2146 | 5 | 0 | 2141 | 0 |
| [Spawning, trial spawners and vaults](features/spawning.md) | 138 | 138 | 0 | 0 | 0 |
| [Statistics](features/statistics.md) | 104 | 5 | 94 | 5 | 0 |
| [Tags](features/tags.md) | 907 | 7 | 0 | 900 | 0 |
| [Ticking, game events and vibrations](features/ticking.md) | 191 | 181 | 10 | 0 | 0 |
| [Villages, trading and POI](features/villages-trading-poi.md) | 599 | 599 | 0 | 0 | 0 |
| [Wandering traders](features/wandering-traders.md) | 5 | 5 | 0 | 0 | 0 |
| [Weather, time and environment](features/weather-time.md) | 85 | 85 | 0 | 0 | 0 |
| [World storage and data fixing](features/world-storage.md) | 307 | 306 | 1 | 0 | 0 |
| [World generation](features/worldgen.md) | 3114 | 2640 | 174 | 300 | 0 |
| **Total** | **19534** | **8822** | **4537** | **6175** | **0** |
