# Feature checklist

Minecraft Java **26.3**. 200 feature families, generated from [the evidence ledger](../compatibility/features.json).

**Counts are selected recorded observations, not full feature coverage.** Vanilla and Rust columns show **pass/fail/skip** counts. `0/0/0` means no recorded observations. Retained observations can refer to different builds; consult their build IDs and artifacts in the ledger. A passing observation does not verify the whole family.

Concurrency and performance rows are additional Ferraloom requirements. Plugin integration is deferred until vanilla server integration is complete.

Regenerate with `python3 tools/report_features.py`.

## Networking

| Feature ID | Expected behavior | Vanilla P/F/S | Rust P/F/S | Rust status |
| --- | --- | --- | --- | --- |
| `network.handshake` | Decode exact target protocol handshake and route status or login | 1/0/0 | 1/0/0 | partial |
| `network.status` | Return vanilla-compatible status JSON and ping payload | 3/0/0 | 3/0/0 | partial |
| `network.framing` | Decode length-prefixed packets across arbitrary stream boundaries | 1/0/0 | 1/0/0 | partial |
| `network.serialization` | Match VarInt, strings, identifiers, positions and registry codecs | 1/0/0 | 1/0/0 | partial |
| `network.login` | Follow target-version login sequence and disconnect reasons | 1/0/0 | 1/0/0 | partial |
| `network.online_auth` | Perform the reference server's session-service authentication | 0/0/0 | 0/0/0 | unverified |
| `network.encryption` | Negotiate and apply vanilla encryption to the complete stream | 0/0/0 | 0/0/0 | unverified |
| `network.compression` | Apply target compression threshold and validate compressed frames | 0/0/0 | 0/0/0 | unverified |
| `network.configuration` | Complete registry, tags and feature negotiation before play | 0/0/0 | 0/0/0 | unverified |
| `network.registries` | Send the target version's synchronized registries and identifiers | 0/0/0 | 0/0/0 | unverified |
| `network.keepalive` | Match keepalive timing, IDs and timeout behavior | 0/0/0 | 0/0/0 | unverified |
| `network.signed_chat` | Enforce target chat signing and acknowledgement rules | 0/0/0 | 0/0/0 | unverified |
| `network.chat_delivery` | Match chat visibility, message decoration and system messages | 1/0/0 | 1/0/0 | partial |
| `network.secure_profile` | Honor secure-profile settings and certificate validation | 0/0/0 | 0/0/0 | unverified |
| `network.transfer` | Match target transfer packets and accepted-transfer policy | 0/0/0 | 0/0/0 | unverified |
| `network.movement` | Apply vanilla movement limits and correction packets | 2/0/0 | 2/0/0 | partial |
| `network.interaction` | Validate reach, sequence and interaction target | 0/0/0 | 0/0/0 | unverified |
| `network.custom_payload` | Handle built-in plugin-channel messages without loading plugins | 0/0/0 | 0/0/0 | unverified |
| `network.resource_pack` | Send packs and process target-version acceptance and rejection states | 0/0/0 | 0/0/0 | unverified |
| `network.cookies` | Match target cookie request, storage and transfer use where present | 0/0/0 | 0/0/0 | unverified |
| `network.chunk_stream` | Send vanilla chunk, light, batch and unload messages | 1/0/0 | 1/0/0 | partial |
| `network.entity_stream` | Synchronize spawn, metadata, movement and removal per viewer | 0/0/0 | 0/0/0 | unverified |
| `network.inventory_stream` | Match slot, cursor, state ID and transaction recovery messages | 0/0/0 | 0/0/0 | unverified |
| `network.disconnect` | Deliver appropriate reasons and release connection-owned state | 0/0/0 | 0/0/0 | unverified |

## World generation and data packs

| Feature ID | Expected behavior | Vanilla P/F/S | Rust P/F/S | Rust status |
| --- | --- | --- | --- | --- |
| `world.seed` | Use target seed interpretation and world-generation settings | 0/0/0 | 0/0/0 | unverified |
| `world.biomes` | Match reference biome placement and climate selection | 0/0/0 | 0/0/0 | unverified |
| `world.terrain` | Match density functions, noise and surface shaping | 0/0/0 | 0/0/0 | unverified |
| `world.surface` | Match surface materials, caves and aquifers | 0/0/0 | 0/0/0 | unverified |
| `world.features` | Match ores, vegetation and feature placement | 0/0/0 | 0/0/0 | unverified |
| `world.structures` | Match structure positions, pieces and placement rules | 0/0/0 | 0/0/0 | unverified |
| `world.world_presets` | Support vanilla world presets and custom generator settings | 0/0/0 | 0/0/0 | unverified |
| `world.height` | Respect per-dimension height and coordinate bounds | 0/0/0 | 0/0/0 | unverified |
| `world.overworld` | Match overworld-specific environment and generation | 0/0/0 | 0/0/0 | unverified |
| `world.nether` | Match Nether generation, travel scale and environment | 0/0/0 | 0/0/0 | unverified |
| `world.end` | Match End islands, entry platform and gateway generation | 0/0/0 | 0/0/0 | unverified |
| `world.portals` | Match portal creation, search, placement and entity transfer | 0/0/0 | 0/0/0 | unverified |
| `world.chunk_tickets` | Honor player, spawn, portal and forced chunk ticket lifetimes | 2/0/0 | 2/0/0 | partial |
| `world.simulation_distance` | Apply separate loading, tracking and ticking distances | 0/0/0 | 0/0/0 | unverified |
| `world.spawn_location` | Match initial spawn search and world spawn constraints | 0/0/0 | 0/0/0 | unverified |
| `world.world_border_gen` | Handle world borders and global coordinate limits consistently | 0/0/0 | 0/0/0 | unverified |
| `world.data_packs` | Load vanilla and custom pack resources in correct precedence | 0/0/0 | 0/0/0 | unverified |
| `world.data_reload` | Apply supported runtime data reload atomically | 0/0/0 | 0/0/0 | unverified |
| `world.feature_flags` | Honor saved feature configuration and reference compatibility behavior | 0/0/0 | 0/0/0 | unverified |
| `world.functions` | Execute functions, tags, scheduling and target macro semantics | 0/0/0 | 0/0/0 | unverified |

## Persistence

| Feature ID | Expected behavior | Vanilla P/F/S | Rust P/F/S | Rust status |
| --- | --- | --- | --- | --- |
| `persistence.nbt` | Read and write target binary NBT types and limits | 0/0/0 | 0/0/0 | unverified |
| `persistence.region` | Read and write target region storage and compression formats | 0/0/0 | 0/0/0 | unverified |
| `persistence.chunk_data` | Persist section palettes, biomes, heightmaps and block states | 1/0/0 | 1/0/0 | partial |
| `persistence.entity_data` | Persist entity types, UUIDs, components and relationships | 2/0/0 | 2/0/0 | partial |
| `persistence.block_entities` | Persist container, command and other block entity state | 0/0/0 | 0/0/0 | unverified |
| `persistence.player_data` | Persist inventory, position, abilities and dimension | 0/0/0 | 0/0/0 | unverified |
| `persistence.level_data` | Maintain level.dat settings, seed and world metadata | 1/0/0 | 1/0/0 | partial |
| `persistence.poi` | Persist and reload village and portal points of interest | 0/0/0 | 0/0/0 | unverified |
| `persistence.scheduled_ticks` | Preserve block and fluid scheduled ticks across unloads | 0/0/0 | 0/0/0 | unverified |
| `persistence.scoreboard_data` | Preserve objectives, scores, teams and display slots | 3/0/0 | 2/1/0 | partial |
| `persistence.advancement_stats` | Save player progression and target file formats | 0/0/0 | 0/0/0 | unverified |
| `persistence.map_data` | Preserve maps, markers and other vanilla saved data | 0/0/0 | 0/0/0 | unverified |
| `persistence.upgrade` | Handle supported older data versions as the reference does | 0/0/0 | 0/0/0 | unverified |
| `persistence.locking` | Prevent conflicting writers and release locks on shutdown | 1/0/0 | 1/0/0 | partial |
| `persistence.atomic_save` | Match reference recovery behavior and avoid partial committed saves | 0/0/0 | 0/0/0 | unverified |
| `persistence.imports` | Open a vanilla-created world and reopen Rust saves in vanilla | 0/0/0 | 0/0/0 | unverified |

## Blocks and simulation

| Feature ID | Expected behavior | Vanilla P/F/S | Rust P/F/S | Rust status |
| --- | --- | --- | --- | --- |
| `blocks.registry` | Implement every target block state and valid property combination | 3/0/0 | 3/0/0 | partial |
| `blocks.placement` | Match placement restrictions, hardness and breaking results | 0/0/0 | 0/0/0 | unverified |
| `blocks.neighbors` | Match update propagation, ordering and recursion limits | 0/0/0 | 0/0/0 | unverified |
| `blocks.scheduled` | Match priority, timing and duplicate scheduling behavior | 0/0/0 | 0/0/0 | unverified |
| `blocks.random_ticks` | Match eligible blocks and randomTickSpeed behavior | 0/0/0 | 0/0/0 | unverified |
| `blocks.redstone_power` | Match weak and strong power, wire levels and update order | 0/0/0 | 0/0/0 | unverified |
| `blocks.redstone_logic` | Match repeaters, comparators, observers and torches | 0/0/0 | 0/0/0 | unverified |
| `blocks.pistons` | Match push limits, adhesion, block entities and update timing | 0/0/0 | 0/0/0 | unverified |
| `blocks.fluids` | Match water, lava, source formation and scheduled flow | 1/0/0 | 1/0/0 | partial |
| `blocks.waterlogging` | Match contained fluid state and interacting block behavior | 0/0/0 | 0/0/0 | unverified |
| `blocks.explosions` | Match block destruction, exposure, knockback and source rules | 0/0/0 | 0/0/0 | unverified |
| `blocks.lighting` | Match light propagation and updates across chunks | 0/0/0 | 0/0/0 | unverified |
| `blocks.fire` | Match ignition, spread, extinguishing and fire gamerules | 0/0/0 | 0/0/0 | unverified |
| `blocks.falling` | Match gravity activation, landing, damage and drops | 0/0/0 | 0/0/0 | unverified |
| `blocks.plants` | Match crop, sapling, stem and vegetation growth | 0/0/0 | 0/0/0 | unverified |
| `blocks.sculk` | Match sculk growth, vibration propagation and shrieker behavior | 0/0/0 | 0/0/0 | unverified |
| `blocks.portals` | Match portal shape validation and block-side interactions | 0/0/0 | 0/0/0 | unverified |
| `blocks.rails` | Match rail placement, slopes, power and detector behavior | 0/0/0 | 0/0/0 | unverified |
| `blocks.containers` | Match hopper transfer, sided access, locks and container rules | 0/0/0 | 0/0/0 | unverified |
| `blocks.processing` | Match furnaces, brewing, campfires and other timed processors | 0/0/0 | 0/0/0 | unverified |
| `blocks.signals` | Match buttons, plates, levers, tripwire and sensors | 0/0/0 | 0/0/0 | unverified |
| `blocks.interactives` | Match doors, beds, respawn anchors, bells and utility interactions | 0/0/0 | 0/0/0 | unverified |
| `blocks.weathering` | Match oxidation, waxing, melting, freezing and snow accumulation | 0/0/0 | 0/0/0 | unverified |
| `blocks.block_events` | Match queued block events, sound and client notification | 0/0/0 | 0/0/0 | unverified |
| `blocks.commands` | Match modes, conditional execution, chains and permissions | 0/0/0 | 0/0/0 | unverified |
| `blocks.structure_tools` | Match save, load, integrity and generation tooling | 0/0/0 | 0/0/0 | unverified |
| `blocks.new_version` | Cover all additional block families discovered in target registries | 0/0/0 | 0/0/0 | unverified |

## Entities

| Feature ID | Expected behavior | Vanilla P/F/S | Rust P/F/S | Rust status |
| --- | --- | --- | --- | --- |
| `entities.registry` | Implement all target entity types and metadata defaults | 0/0/0 | 0/0/0 | unverified |
| `entities.lifecycle` | Match creation, removal, despawn and tick eligibility | 1/0/0 | 1/0/0 | partial |
| `entities.physics` | Match movement, bounding boxes, stepping and collision shapes | 0/0/0 | 0/0/0 | unverified |
| `entities.gravity` | Match acceleration, terminal behavior and landing damage | 0/0/0 | 0/0/0 | unverified |
| `entities.attributes` | Match base values, modifiers and synchronized attributes | 0/0/0 | 0/0/0 | unverified |
| `entities.ai_goals` | Match target acquisition, priorities and behavior transitions | 0/0/0 | 0/0/0 | unverified |
| `entities.navigation` | Match traversability and movement for ground, flying and aquatic mobs | 0/0/0 | 0/0/0 | unverified |
| `entities.spawning` | Match category caps, distances, biome conditions and spawn checks | 0/0/0 | 0/0/0 | unverified |
| `entities.spawners` | Match activation, delays, spawn potentials and target trial behavior | 0/0/0 | 0/0/0 | unverified |
| `entities.breeding` | Match food, cooldowns, inheritance and ownership | 0/0/0 | 0/0/0 | unverified |
| `entities.villagers` | Match schedules, workstations, professions and restocking | 0/0/0 | 0/0/0 | unverified |
| `entities.raids` | Match raid triggers, waves, difficulty and victory effects | 0/0/0 | 0/0/0 | unverified |
| `entities.combat` | Match damage calculation, cooldowns, armor and attack effects | 0/0/0 | 0/0/0 | unverified |
| `entities.projectiles` | Match projectile motion, collision, damage and ownership | 0/0/0 | 0/0/0 | unverified |
| `entities.status_effects` | Match effect duration, amplifiers, stacking and immunity | 0/0/0 | 0/0/0 | unverified |
| `entities.potions` | Match drinkable, splash and lingering potion application | 0/0/0 | 0/0/0 | unverified |
| `entities.death_loot` | Match death events, loot context, equipment drops and experience | 0/0/0 | 0/0/0 | unverified |
| `entities.item_entities` | Match merging, pickup, despawn and environmental damage | 0/0/0 | 0/0/0 | unverified |
| `entities.experience` | Match orb values, merging, pickup and mending | 0/0/0 | 0/0/0 | unverified |
| `entities.vehicles` | Match boats, minecarts, horses and passenger synchronization | 0/0/0 | 0/0/0 | unverified |
| `entities.boss_dragon` | Match dragon phases, crystals, death, respawn and gateways | 0/0/0 | 0/0/0 | unverified |
| `entities.boss_wither` | Match construction, spawn charge, targeting and destruction | 0/0/0 | 0/0/0 | unverified |
| `entities.warden` | Match vibration targeting, anger, sonic attacks and emergence | 0/0/0 | 0/0/0 | unverified |
| `entities.utility_mobs` | Match golems, allays and other specialized mob systems | 0/0/0 | 0/0/0 | unverified |
| `entities.displays` | Match transformations, interpolation, hit handling and persistence | 0/0/0 | 0/0/0 | unverified |
| `entities.leashes` | Match links, riding graphs, ownership and disconnect rules | 0/0/0 | 0/0/0 | unverified |
| `entities.new_version` | Cover additional entity families discovered in target registries | 0/0/0 | 0/0/0 | unverified |

## Items and inventories

| Feature ID | Expected behavior | Vanilla P/F/S | Rust P/F/S | Rust status |
| --- | --- | --- | --- | --- |
| `items.registry` | Implement target items, defaults and data components | 0/0/0 | 0/0/0 | unverified |
| `items.stacking` | Match maximum counts, component equality and stack merging | 0/0/0 | 0/0/0 | unverified |
| `items.containers` | Match player, block and entity inventories and slot rules | 0/0/0 | 0/0/0 | unverified |
| `items.clicks` | Match click modes, dragging, swaps and transactional validation | 0/0/0 | 0/0/0 | unverified |
| `items.crafting` | Match shaped, shapeless and special crafting recipes | 0/0/0 | 0/0/0 | unverified |
| `items.recipe_book` | Match recipe discovery, placement and unlock persistence | 0/0/0 | 0/0/0 | unverified |
| `items.processing` | Match cooking, stonecutting, smithing and target processors | 0/0/0 | 0/0/0 | unverified |
| `items.brewing` | Match ingredient transforms, fuel and potion components | 0/0/0 | 0/0/0 | unverified |
| `items.anvil` | Match repair cost, rename, combination and limits | 0/0/0 | 0/0/0 | unverified |
| `items.enchanting` | Match enchanting offers, costs, applicability and data definitions | 0/0/0 | 0/0/0 | unverified |
| `items.grindstone` | Match enchantment removal, repair and experience | 0/0/0 | 0/0/0 | unverified |
| `items.loot` | Match loot context, functions, conditions and random sequences | 0/0/0 | 0/0/0 | unverified |
| `items.trades` | Match offers, demand, reputation, discounts and exhaustion | 0/0/0 | 0/0/0 | unverified |
| `items.books` | Match book editing, signing, filtering and text components | 0/0/0 | 0/0/0 | unverified |
| `items.maps` | Match map sampling, scaling, locking and explorer data | 0/0/0 | 0/0/0 | unverified |
| `items.buckets` | Match fluid pickup, placement and entity data capture | 0/0/0 | 0/0/0 | unverified |
| `items.food` | Match consumption, nutrition, effects and cooldowns | 0/0/0 | 0/0/0 | unverified |
| `items.equipment` | Match damage, repair, breakage and equipment effects | 0/0/0 | 0/0/0 | unverified |
| `items.special` | Match tools, ignition, pearls, rockets, bundles and target items | 0/0/0 | 0/0/0 | unverified |
| `items.new_version` | Cover target-specific item families and data-driven recipes | 0/0/0 | 0/0/0 | unverified |

## Players and commands

| Feature ID | Expected behavior | Vanilla P/F/S | Rust P/F/S | Rust status |
| --- | --- | --- | --- | --- |
| `players.gamemodes` | Match survival, creative, adventure and spectator restrictions | 0/0/0 | 0/0/0 | unverified |
| `players.abilities` | Match flight, invulnerability, movement speeds and synchronization | 0/0/0 | 0/0/0 | unverified |
| `players.hunger` | Match activity costs, starvation and regeneration | 0/0/0 | 0/0/0 | unverified |
| `players.health` | Match health, absorption, damage sources and invulnerability frames | 0/0/0 | 0/0/0 | unverified |
| `players.respawn` | Match death screen, spawn selection, items and dimension state | 0/0/0 | 0/0/0 | unverified |
| `players.sleep` | Match bed rules, sleep percentage and night skipping | 0/0/0 | 0/0/0 | unverified |
| `players.swimming` | Match movement modes, collision size and stamina interactions | 0/0/0 | 0/0/0 | unverified |
| `players.oxygen` | Match air supply, powder snow and environmental damage | 0/0/0 | 0/0/0 | unverified |
| `players.interaction` | Match spawn protection, adventure restrictions and operator bypass | 0/0/0 | 0/0/0 | unverified |
| `players.commands` | Match complete target command tree and execution semantics | 8/0/0 | 8/0/0 | partial |
| `players.command_context` | Match selectors, positions, rotations, dimensions and result storage | 0/0/0 | 0/0/0 | unverified |
| `players.permissions` | Match permission levels and operator configuration | 0/0/0 | 0/0/0 | unverified |
| `players.gamerules` | Match all target gamerules, types, defaults and effects | 0/0/0 | 0/0/0 | unverified |
| `players.scoreboards` | Match criteria, operations, display slots and triggers | 5/0/0 | 5/0/0 | partial |
| `players.teams` | Match team membership, visibility, collision and friendly fire | 1/0/0 | 0/1/0 | unverified |
| `players.advancements` | Match trigger conditions, rewards, progress and revocation | 0/0/0 | 0/0/0 | unverified |
| `players.statistics` | Match per-player stat increments and queries | 0/0/0 | 0/0/0 | unverified |
| `players.recipes` | Match criteria-driven unlocks and recipe commands | 0/0/0 | 0/0/0 | unverified |
| `players.bossbars` | Match custom boss bars, visibility, value and persistence | 0/0/0 | 0/0/0 | unverified |
| `players.titles` | Match text components, timing and target player delivery | 0/0/0 | 0/0/0 | unverified |
| `players.sounds_particles` | Match commands, event packets and targeting | 0/0/0 | 0/0/0 | unverified |
| `players.teleport` | Match command teleport, rotation, safety and acknowledgments | 0/0/0 | 0/0/0 | unverified |
| `players.spectator` | Match entity spectating, controls and target removal | 0/0/0 | 0/0/0 | unverified |
| `players.social` | Match tab-list, latency, game mode and display-name state | 0/0/0 | 0/0/0 | unverified |
| `players.attributes` | Match player-specific effects, modifiers and attribute commands | 0/0/0 | 0/0/0 | unverified |
| `players.progression` | Match levels, point costs, drops and XP commands | 0/0/0 | 0/0/0 | unverified |
| `players.new_version` | Cover additional player and command systems in the pinned release | 0/0/0 | 0/0/0 | unverified |

## Administration

| Feature ID | Expected behavior | Vanilla P/F/S | Rust P/F/S | Rust status |
| --- | --- | --- | --- | --- |
| `admin.properties` | Honor all vanilla server.properties settings and defaults | 0/0/0 | 0/0/0 | unverified |
| `admin.difficulty` | Match difficulty setting, locking and gameplay effects | 0/0/0 | 0/0/0 | unverified |
| `admin.whitelist` | Match allowlist persistence, enforcement and commands | 0/0/0 | 0/0/0 | unverified |
| `admin.bans` | Match player and address bans, expiry and reasons | 0/0/0 | 0/0/0 | unverified |
| `admin.operators` | Match ops.json identity, level and bypass behavior | 0/0/0 | 0/0/0 | unverified |
| `admin.max_players` | Match player capacity, reserved operators and status counts | 0/0/0 | 0/0/0 | unverified |
| `admin.rcon` | Match authentication, framing, command execution and limits | 0/0/0 | 0/0/0 | unverified |
| `admin.query` | Match challenge, basic and full status responses | 0/0/0 | 0/0/0 | unverified |
| `admin.console` | Match console command context, feedback and shutdown input | 0/0/0 | 0/0/0 | unverified |
| `admin.logging` | Provide equivalent operational events and crash diagnostics | 0/0/0 | 0/0/0 | unverified |
| `admin.resourcepacks` | Honor pack URLs, hashes, prompts and required-pack configuration | 0/0/0 | 0/0/0 | unverified |
| `admin.world_border` | Match center, size interpolation, damage and warnings | 0/0/0 | 0/0/0 | unverified |
| `admin.time` | Match day time, game time, daylight rule and time commands | 0/0/0 | 0/0/0 | unverified |
| `admin.weather` | Match weather duration, clear weather and dimension rules | 0/0/0 | 0/0/0 | unverified |
| `admin.spawn` | Match world spawn, spawn radius and spawn protection settings | 0/0/0 | 0/0/0 | unverified |
| `admin.forced_chunks` | Match forceload command, limits and restart behavior | 0/0/0 | 0/0/0 | unverified |
| `admin.save_commands` | Match save-all, save-off and save-on behavior | 0/0/0 | 0/0/0 | unverified |
| `admin.idle` | Match idle timeout, rate limits and reference proxy settings | 0/0/0 | 0/0/0 | unverified |
| `admin.motd` | Match MOTD, icon, hide-player settings and player samples | 0/0/0 | 0/0/0 | unverified |
| `admin.management` | Match any management protocol and monitoring interfaces shipped in target | 0/0/0 | 0/0/0 | unverified |
| `admin.debug` | Match supported vanilla debug output and diagnostics | 0/0/0 | 0/0/0 | unverified |
| `admin.new_version` | Cover additional properties and administrative interfaces in target | 0/0/0 | 0/0/0 | unverified |

## Lifecycle

| Feature ID | Expected behavior | Vanilla P/F/S | Rust P/F/S | Rust status |
| --- | --- | --- | --- | --- |
| `lifecycle.startup` | Match first-run configuration, EULA enforcement and startup failures | 0/0/0 | 0/0/0 | unverified |
| `lifecycle.shutdown` | Finish player saves, chunk saves and resource closure | 0/0/0 | 0/0/0 | unverified |
| `lifecycle.restart` | Continue saved world state without unintended reset or duplication | 0/0/0 | 0/0/0 | unverified |
| `lifecycle.crash_recovery` | Recover supported saves after abrupt interruption | 0/0/0 | 0/0/0 | unverified |
| `lifecycle.errors` | Handle malformed network, pack and save input without corrupting unrelated state | 0/0/0 | 0/0/0 | unverified |
| `lifecycle.resource_limits` | Bound input allocations and handle disk or memory failures safely | 0/0/0 | 0/0/0 | unverified |
| `lifecycle.world_unload` | Save and detach world state while preserving logical ownership | 0/0/0 | 0/0/0 | unverified |
| `lifecycle.reference_corpus` | Account for every extracted registry entry, recipe, command and vanilla pack resource | 0/0/0 | 0/0/0 | unverified |

## Concurrency and performance

| Feature ID | Expected behavior | Vanilla P/F/S | Rust P/F/S | Rust status |
| --- | --- | --- | --- | --- |
| `concurrency.ownership` | Prevent concurrent mutation of world and player state without defined ownership | 0/0/0 | 0/0/0 | unverified |
| `concurrency.determinism` | Preserve observed vanilla ordering where gameplay depends on it | 0/0/0 | 0/0/0 | unverified |
| `concurrency.cross_region` | Resolve multi-region operations without duplication or deadlock | 0/0/0 | 0/0/0 | unverified |
| `concurrency.thread_safety` | Demonstrate safety under thread and scheduler stress | 0/0/0 | 0/0/0 | unverified |
| `concurrency.scaling` | Measure throughput across CPU counts under an identical reproducible workload | 0/0/0 | 0/0/0 | unverified |
| `concurrency.budget` | Record CPU, memory, latency and throughput against pinned vanilla and Rust builds | 0/0/0 | 0/0/0 | unverified |
| `concurrency.replay` | Keep seeds, inputs, environment and traces for independent reproduction | 0/0/0 | 0/0/0 | unverified |

## Plugins (deferred)

| Feature ID | Expected behavior | Vanilla P/F/S | Rust P/F/S | Rust status |
| --- | --- | --- | --- | --- |
| `plugins.java_compatibility` | Defer plugin API, JVM bridge and compatibility work until vanilla server gates pass | 0/0/0 | 0/0/0 | deferred (unverified) |
| `plugins.isolation` | Defer plugin loading, scheduling and lifecycle until vanilla server integration is complete | 0/0/0 | 0/0/0 | deferred (unverified) |
