# Vanilla gaps

The one tracked list of known differences between Ferraloom and the vanilla 26.3 dedicated server, as a
vanilla client, a Spigot plugin, an operator or a server admin sees them. Anything that only concerns
Horizon's own client is out of scope.

Verified against `ferraloom/next` at `5d5371c7b` (2026-11-26): every **Fixed** row's commit is an ancestor of
that head, and every **Open** row was checked in the code unless it says otherwise.

**Adding a gap.** Give it the next free number (the first free one is **#128**), put it in its area, ranked by how
visible it is to players, and fill every column: what is seen and how vanilla differs, the vanilla
reference (`Class#method` from `.cache/decompiled/26.3/`), where it lives in our code, a size and who found
it. When it is fixed, move it to **Fixed** with the commit and the harness case that proves it. Numbers are
never reused: #1 to #32 are the original audit's (`9b35c1726`), so old references still resolve.

Sizes: S under a day, M a few days, L a week or more. Sources: *audit* (the original audit), *combat*
(sword cooldown), *A*, *B*, *C*, *D* (the fix clusters), *fperm* (function permission level), *found*
(plugin foundation), *p-server*, *p-players*, *p-blocks*, *p-entities*, *p-inv* (the plugin areas), *orch*
(the orchestrator's integration), *rig* (live checks through the vanilla client rig,
`docs/ferraloom/client-rig.md`), *harness* (the command_exec, save and loot harnesses ported to the dedicated server), *review* (code review of the branch), *docs* (an existing doc).

## Open

### Combat and entities

| # | Gap | Vanilla | Ferraloom | Size | Source |
|---|---|---|---|---|---|
| 33 | `/damage <player> ...` on a server player fails with "Target is invulnerable to the given damage type" (console and players). Harnesses work around it by having the player damage itself | `DamageCommand#damage` → `Entity#hurtServer` | `game/commands/world_cmds.rs:130` `cmd_damage`: no `Target::Guest` arm, falls to `_ => false` | S | B, A |
| 34 | A shield-blocked hit plays no hurt sound to others | `LivingEntity#hurtServer` (blocked branch) | `health.rs` plays it only with the damage broadcast. `pvp-shield` in `KNOWN` of `tools/test_server_knockback_parity.py` | S | B |
| 35 | A mace smash on a player runs none of the mace's effects (the attacker's fall goes on) | `MaceItem#hurtEnemy` | PvP path in `game/lan_pvp.rs`. `pvp-mace` in `KNOWN` of `test_server_knockback_parity.py` | S | combat, B |
| 36 | The velocity packet after fall damage differs from vanilla's (0.42 up) | `ServerPlayer` `hurtMarked` → `ClientboundSetEntityMotionPacket` | Guest hurt path, `game/lan_actions.rs` | S | B |
| 37 | Donkeys and mules never rear | `AbstractHorse#canPerformRearing` (true; only `Llama` and `Camel` override it) | `mobs/equine.rs:74` `can_rear` lists horse, skeleton and zombie horse only | S | B |
| 38 | A parrot that drops off a server player's shoulder never lands again; vanilla re-lands it within seconds | `LandOnOwnersShoulderGoal`, `ShoulderRidingEntity` | Mobs see only the nearest player, not their owner (`game/shoulders.rs`, mob AI). Not re-verified | M | A |
| 39 | Attacks are not checked like vanilla's: a PvP hit has no `isWithinAttackRange` check, no hit checks the world border, and attacking an item, an XP orb or oneself doesn't kick with `multiplayer.disconnect.invalid_entity_attacked` (mob hits do check interaction range) | `ServerGamePacketListenerImpl#handleAttack` | `game/lan_actions.rs:2151` `guest_attack` | S | audit |
| 40 | `DamageEvent` for players carries no causing or direct entity (the client's flash works; its damage-direction tilt and anything reading the ids don't) | `ClientboundDamageEventPacket` (`sourceCauseId`, `sourceDirectId`) | `game/lan_actions.rs:3187` passes `None, None` | S | B |
| 41 | Duplicate UUIDs: summoning with the UUID of an entity still in its death animation succeeds; vanilla refuses | `SummonCommand#createEntity` (`commands.summon.failed.uuid`), `ServerLevel#tryAddFreshEntityWithPassengers` | Summon path in `game/commands`. Not re-verified | S | D |
| 42 | TNT and falling blocks are not command targets, so `/tp`, `/kill`, `/data` can't reach them | `EntitySelector#findEntities` | `Target` has no variant for them (`game/commands.rs`) | M | D |
| 43 | Two mobs riding one boat push each other | `Entity#isPassengerOfSameVehicle` in `LivingEntity#pushEntities` | `Seen` carries no vehicle id; the same-vehicle check covers only a mob's own rider and mount (`mobs/ai.rs`) | S | D |
| 44 | `NoAI` mobs skip their species' own `aiStep` (the generic part now runs) | Each species' `aiStep` override runs without AI too | `mobs/ai.rs` `tick_entity`, after `fca21704f` | M | D |
| 45 | A bee's anger target is not loaded from saved data | `Bee#readAdditionalSaveData` → `NeutralMob#readPersistentAngerSaveData` | `entity_nbt.rs` reads no anger target; `mobs/bee.rs` `angry_at` | S | D |
| 46 | A spectator's camera isn't reset by `/tp`, and a camera entity changing dimension resets the camera instead of taking the spectator along | `ServerPlayer#teleport` (`setCamera(this)`), `ServerPlayer#tick` camera follow | `game/lan_spectate.rs`; teleports in `game/commands.rs` | S | B |
| 47 | A camel or nautilus dash emits no `ENTITY_ACTION` game event, so sculk doesn't hear it | `Camel#handleStartJump` → `gameEvent(GameEvent.ENTITY_ACTION)` | `game/mounts.rs` riding-jump handler (`bfb5b4a89`) | S | B |
| 48 | A player newly joined sees players standing still only after they send a move (hidden in practice: clients send one every 20 ticks) | `ChunkMap#playerLoadedChunk`, `ChunkMap.TrackedEntity#updatePlayer` | Pairing in `lan/hub.rs` `track_entities` runs on moves | S | A |
| 125 | The sweep check reads the last move of the tick's whole packet pass, so a move the client sent after the attack in the same pass decides whether that hit sweeps; vanilla reads the movement known when the attack packet is handled, and zeroes it at `ClientTickEnd` when no move came. Off by one client tick while the player's speed crosses the sweep threshold | `ServerGamePacketListenerImpl#handleMovePlayer`, `#handleClientTickEnd`, `Player#isSweepAttack` | `game/lan_actions.rs` `guest_attack` (`known_movement` from `guests[gi].moves.last()`) | S | review |

### Commands

| # | Gap | Vanilla | Ferraloom | Size | Source |
|---|---|---|---|---|---|
| 49 | Absolute `/tp` keeps a non-player entity's velocity; vanilla zeroes it. The packet already says zero, so the client and server disagree until the next sync | `Entity#teleportSetPosition` (`PositionMoveRotation.calculateAbsolute`) | `game/commands.rs:1266` `teleport_to_in_level`: only `Target::Player` zeroes `velocity` | S | D |
| 50 | `/kick`'s success message names the player as plain text; vanilla uses the display name (hover, click to suggest, insertion) | `KickCommand#kickPlayers` (`player.getDisplayName()`) | `game/commands/extra.rs:752` `Text::literal(name)` | S | C |
| 51 | `/ride <player> mount <vehicle>` fails for server players | `RideCommand#mount` | `game/commands/world_cmds.rs:1120` `cmd_ride` | S | D |
| 52 | `say` from a function or the server logs `System chat: [Server] ...`; vanilla logs `[Not Secure] [Server] ...` (the console's own `say` is right) | `PlayerList#broadcastChatMessage`, `MinecraftServer#logChatMessage` | `game/commands.rs:3125` `cmd_say` → the system-chat log in `game/dedicated.rs:917` | S | fperm |
| 53 | `/perf` answers "Unknown or incomplete command" though the dedicated tree offers it to level-4 players | `PerfCommand` (`MinecraftServer#startRecordingMetrics`) | Left out of `horizon-command/src/tree_server.rs`; still listed in `lan/commands.rs:32` `DEDICATED_ONLY`. No metrics recorder | M | C |
| 54 | A player's command keeps the permission level they had when it was queued, so a `/deop` in the same tick can come too late | `Commands#performCommand` checks at execution | Guest command queue, `game/lan_actions.rs` `guest_command` | S | C |
| 55 | `/help` inside a function lists commands at the function's compile level, not the caller's | `HelpCommand` (source's permissions) | `game/functions.rs` runs lines at the compiled level | S | fperm |
| 118 | From the console, `attribute <player> <attribute> get` answers "is not a valid entity for this command" and `execute as <player> run attribute @s ...` prints nothing; vanilla returns the value. Mobs work, and so does the player typing it in the client | `AttributeCommand#getLivingEntity` (a `ServerPlayer` is a `LivingEntity`) | The `attribute` command's target lookup for server players when the source is not that player, `game/commands*`. Seen live with the client rig, not traced | S | rig |
| 120 | A player's command that ends in `say`, `me` or `msg`/`tell`/`w` runs as that player's own chat: whatever `execute` put before it is ignored (`as`, `on`, `in`, forks that match nothing, an unknown dimension), selectors in the message stay as typed and no result is reported. `/execute as @e[tag=h] run say mob` prints `[Player] mob`, vanilla `[Daisy] mob` | `SayCommand`, `EmoteCommands`, `MsgCommand` under `ExecuteCommand` (`MessageArgument#resolveChatMessage`) | The chat-command path for signed message commands (`game/lan_actions.rs` `guest_command`, `game/commands.rs` `cmd_say`); 32 records `KNOWN` in `tools/test_command_exec_parity.py` | M | harness |
| 121 | `tick step N` advances fewer than N ticks after a dimension change or chunk loading (after a portal trip, `tick step 5` moved 3) | `ServerTickRateManager#stepGameIfPaused`, `MinecraftServer#tickServer` | `game/tick_rate*`; noted and stepped over by `CommandExecClient` | S | harness |
| 122 | A mob's new waypoint transmit range counts at once in `/waypoint list`; vanilla waits for the mob's next tick | `ServerWaypointManager`, `LivingEntity#tick` | waypoints in `game/`; `KNOWN` in `tools/test_command_exec_parity.py` | S | harness |
| 56 | `@e` without a distance or volume reads only the source's level; vanilla reads every level | `EntitySelector#findEntities` | Selectors in `game/commands.rs`. From `docs/server-levels.md`, not re-verified | M | docs |

### World and blocks

| # | Gap | Vanilla | Ferraloom | Size | Source |
|---|---|---|---|---|---|
| 21 | Structure, jigsaw and test blocks and command-block minecarts can't be edited: Save, Load, Detect and Done change nothing. Ferraloom has no structure, jigsaw or test block entity, and command minecarts hold no command | `handleSetStructureBlock`, `handleSetJigsawBlock`, `handleJigsawGenerate`, `handleSetTestBlock`, `handleTestInstanceBlockAction`, `handleSetCommandMinecart` | No serverbound variants (dropped by `lan_guest_packet`'s last arm). Needs block-entity variants and saves in `chunk/block_entity.rs`, `save.rs`, `game/minecarts.rs:33` | L (audit said M) | audit, B |
| 57 | Advancement reward functions don't exist | `AdvancementRewards#grant` (runs the function at level 2) | `game/advancements.rs`; should go through `run_function` (`game/functions.rs`) | S | fperm |
| 123 | `execute if items` (and any command read) counts an untouched loot chest's rolled items but never unpacks them into the chest, so its `Items` stay empty until a player opens it | `RandomizableContainer#unpackLootTable` (called by `getItem`) | block-entity loot in `game/`; `KNOWN` in `tools/test_spawn_loot_parity.py` | S | harness |
| 124 | `save-all` rewrites the player file of a player who never joined this session, and the rewritten file loses its `Pos` | `PlayerList#saveAll` (only online players) | player saves in `game/dedicated.rs`; `KNOWN` in `tools/test_save_roundtrip_parity.py` | S | harness |

### Inventory and items

| # | Gap | Vanilla | Ferraloom | Size | Source |
|---|---|---|---|---|---|
| 58 | `attribute_modifiers` set on an individual stack are ignored; only the item type's defaults apply | `ItemStack#forEachModifier` (`DataComponents.ATTRIBUTE_MODIFIERS`) | `game/lan_attributes.rs` `apply_equipment` | S | combat |
| 59 | A broken item (`isBroken`) still applies its modifiers | `LivingEntity#collectEquipmentChanges` (`ItemStack#isBroken`) | `game/lan_attributes.rs` | S | combat |
| 126 | A `damage_resistant` component that names damage types directly (one id or a list) is not sent to clients; only the tag form is, which is all vanilla's items use. The server still applies it | `DamageResistant.STREAM_CODEC` (`ByteBufCodecs.holderSet`: size plus one, then registry ids) | `lan/item_extras.rs` `network` (needs the damage type registry's ids) | S | review |

### Networking and protocol

| # | Gap | Vanilla | Ferraloom | Size | Source |
|---|---|---|---|---|---|
| 26 | Entity-bound sounds play at a point: riptide, goat horns while walking, shearing, frog, goat ram, allay, breeze deflect, copper golem, sniffer and about 15 more call sites stay where they started. Audible only for long sounds on fast movers | `ServerLevel#playSeededSound` (entity form) → `ClientboundSoundEntityPacket` | Every sound goes out positional (`lan/containers.rs`). `SOUND_ENTITY` never sent. Needs an entity-aware capture in `audio.rs` and tagging each site | M | audit, D |
| 28 | `/rotate` and `tp ... facing` on a player send a full position teleport, so the client snaps position and loses velocity | `ServerPlayer#forceSetRotation` (`ClientboundPlayerRotationPacket`), `ServerPlayer#lookAt` (`ClientboundPlayerLookAtPacket`) | `game/commands/world_cmds.rs:210` `cmd_rotate` → `teleport_to`. `PLAYER_ROTATION` and `PLAYER_LOOK_AT` never sent | S | audit |
| 27 | Teleports other than `/tp` (ender pearls, portals, `/rotate` on entities) send no `ClientboundTeleportEntityPacket`, so items, projectiles and ridden vehicles glide or get pulled back. `/tp` is fixed (`6385ae8ea`) | `Entity#sendTeleportTransitionToPlayers` (riders always; trackers for no-update-interval types and `SERVER_AND_CLIENT` movers) | `Hub::teleport_entity` (`lan/containers.rs`) is called only from `game/commands.rs` `teleport_to_in_level` | S | audit, D |
| 60 | Trackers get attribute changes but not vanilla's identical re-sends of a dirty instance (a hit wearing the sword). The client ends in the same state; the snapshot is also rebuilt for every guest every tick | `ServerEntity#sendDirtyEntityData` | `lan/hub.rs`, `lan/entity.rs` diff full snapshots | M | combat |
| 61 | Equipment changes apply at once, not in the next player tick, so two changes in one tick split into packets differently | `LivingEntity#detectEquipmentUpdates` (in `tick`) | `game/lan_attributes.rs` `apply_equipment` on each guest turn | S | combat |
| 62 | Invisibility doesn't apply its `waypoint_transmit_range` modifier, so an invisible player still shows on the locator bar at full range | `MobEffects.INVISIBILITY` attribute modifier | `effects.rs`, `lan/waypoints.rs` | S | combat |
| 63 | Never sent, low impact: `TEST_INSTANCE_BLOCK_STATUS` (with #21), `TAB_LIST` (no vanilla trigger), `CUSTOM_CHAT_COMPLETIONS`, `DELETE_CHAT`, `PLAYER_COMBAT_ENTER`/`END` (narration, telemetry), `DEBUG_*`, `GAME_TEST_HIGHLIGHT_POS`, `LOW_DISK_SPACE_WARNING`, `CUSTOM_REPORT_DETAILS`, `MOVE_MINECART_ALONG_TRACK` (experimental minecarts), `START_CONFIGURATION`, `COOKIE_REQUEST`, `STORE_COOKIE`, `RESOURCE_PACK_POP`, `PING`, `SET_PLAYER_INVENTORY` (its slots go out as `ContainerSetSlot` on window 0). Serverbound `DebugSubscriptionRequest` is dropped (debug renderers only) | | | S each | audit |

### Server administration

| # | Gap | Vanilla | Ferraloom | Size | Source |
|---|---|---|---|---|---|
| 30 | RCON, Query and the management server are missing: `enable-rcon`, `rcon.*`, `enable-query`, `query.port`, `management-server-*` and `status-heartbeat-interval` do nothing, so owners' tools can't connect | `RconThread`, `QueryThreadGs4`, `ManagementServer` | `horizon-server-files/src/properties.rs` fields with no reader outside the crate (only `examples/probe/server_files_probe.rs`) | L | audit |
| 31 | Other ignored properties: `rate-limit` (packet-rate kick), `max-world-size`, `max-chained-neighbor-updates`, `max-tick-time` (watchdog), `broadcast-rcon-to-ops`, `initial-enabled-packs` and `initial-disabled-packs`, `region-file-compression`, `sync-chunk-writes`, `enable-jmx-monitoring`, `text-filtering-config`/`-version`. `function-permission-level` is fixed (`8ec12d527`) | `DedicatedServerProperties` readers | `properties.rs` fields with no reader | M | audit, fperm |
| 119 | The console prints command feedback with a `System chat: ` prefix, and debug lines (`mobs: ... speculative ticks ...`, `dropped items: N`) on stdout; vanilla prints the feedback bare and no such lines | `MinecraftServer#sendSystemMessage` (logs the plain text) | `game/dedicated.rs:917` (the system-chat log) and the debug prints. Seen live with the client rig | S | rig |
| 64 | Data packs load on the first tick, so their load errors are logged after `Done`; vanilla logs them before. Content is identical | `MinecraftServer` loads `ReloadableServerResources` before the server starts | `game/dedicated.rs`, `datapack.rs` `DataStore::load` | S | fperm |
| 127 | With `save-off` the periodic save is skipped whole, players included; vanilla's autosave still saves the online players and skips only the chunks. (The periodic save does not save online players even with `save-on`: they save when they leave and at stop) | `MinecraftServer#autoSave` → `PlayerList#saveAll`, chunks skipped by `ServerLevel.noSave` | `game.rs` `autosaving` in the tick | S | review |
| 65 | The floating kick departs from vanilla in two small ways: a block in a chunk the server doesn't have counts as solid (nobody is kicked over chunks the server can't see), and the fly counter also resets on the hub's correction teleports (vanilla only in `teleportTo`) | `ServerGamePacketListenerImpl#tickPlayer`, `#noBlocksAround` | `lan/hub/player_meta.rs:315` `update_floating` | S | A |

### Plugins API

Everything here is a Spigot API call or event a plugin can observe. Coverage at HEAD by the bridge's own
count was 729 of 1,392 stubbed-interface methods (52%) on the server-area branch before the final merge;
`build.py` prints the current figure.

**Players** (`FerraloomPlayer extends StubPlayer`, so `LivingEntity` methods the entities area implements
don't reach players)

| # | Gap | Size | Source |
|---|---|---|---|
| 66 | `Player.getAttribute`, `getVehicle`, `getArrowsInBody` are stubs (the `FerraloomLivingEntity` versions exist but `FerraloomPlayer` doesn't use them) | S | p-players, p-entities |
| 67 | `PlayerDeathEvent.getDamageSource` and `getDrops` throw ("does not implement ... yet"); keepLevel and new experience from the event are not applied | S | p-players |
| 68 | `PlayerDropItemEvent` not fired (bit to go in the entities overflow word 256-319) | S | p-players, p-entities |
| 69 | `hidePlayer`/`showPlayer` not bridged (`canSee` answers true); needs a per-viewer filter in the tracker's hot loop | M | p-players |
| 70 | `setWalkSpeed`/`setFlySpeed`, `setPlayerTime`/`setPlayerWeather` (getters answer the world's), health scaling, `getStatistic` not bridged | M | p-players |
| 71 | Bucket events, `FoodLevelChangeEvent`, particles with data not bridged | M | p-players |
| 72 | `PlayerTeleportEvent` doesn't fire for portals, spectate and gateways; a pearl's event can't change where it lands | S | p-players |
| 73 | A custom player list name is not sent to players who join later | S | p-players |
| 74 | `PlayerMoveEvent` sees the position from the tick's start; a `PlayerRespawnEvent` place is reached as a teleport at the next tick; `PlayerLoginEvent.getHostname` is the address, not the handshake host | S | p-players |
| 75 | `AsyncPlayerChatEvent` lines go out unsigned; the deprecated sync `PlayerChatEvent` is not fired | M | found |
| 76 | Join and quit messages lose their hover text | S | found |
| 77 | `Player.openBook`, opening another player's inventory (invsee) or an ender chest are not supported | M | p-inv |

**Entities**

| # | Gap | Size | Source |
|---|---|---|---|
| 78 | Only mobs, dropped items and players are entity handles: projectiles, armor stands, boats (and so `Projectile.getShooter`, `strikeLightning`'s returned entity) are not | L | p-entities, p-blocks |
| 79 | Entities in a dimension that isn't in place are unreachable: `World.getEntities` and chunk lists come back empty, `spawnEntity`/`dropItem` there throw. Needs an accessor in `game/levels.rs` for a parked level's mobs and items | M | p-entities |
| 80 | Not bridged: potion effects on mobs, `LivingEntity.getEquipment`, `AttributeInstance`, passengers (always empty), `createExplosion` with a source entity | M | p-entities |
| 81 | Events not fired: `EntityPickupItemEvent`, `InventoryPickupItemEvent`, `EntityRegainHealthEvent`, `ProjectileLaunchEvent`/`ProjectileHitEvent`, `PotionSplashEvent`, `VehicleEnterEvent`/`ExitEvent`, `HangingBreakEvent`, `ExplosionPrimeEvent` | L | p-entities, p-inv |
| 82 | Player `EntityDamageEvent` fires before the hurt cooldown and armor, so plugins see a different amount than on CraftBukkit | S | p-entities |
| 83 | A cancelled `EntityTargetEvent` is undone only after one tick with the new target | S | p-entities |
| 84 | `CreatureSpawnEvent` reasons beyond NATURAL, CHUNK_GEN, CUSTOM and DEFAULT (summons and spawn eggs say DEFAULT) | S | p-entities |
| 85 | Entity UUIDs seen by plugins aren't kept across restarts | M | p-entities |

**Blocks and worlds** (CraftBukkit differences also in [plugins-blocks-worlds.md](plugins-blocks-worlds.md))

| # | Gap | Size | Source |
|---|---|---|---|
| 86 | Tags (`Tag.isTagged`, `Tag.getValues`; 4 of 16 corpus plugins) not bridged | M | p-blocks |
| 87 | Not bridged: `spawnParticle`, `isFlammable`, `isInteractable`, `setBiome`, redstone power queries | M | p-blocks |
| 88 | Events not fired: `BlockDamageEvent`, `BlockFertilizeEvent`, `BlockDispenseEvent`, `StructureGrowEvent`, `PortalCreateEvent`; `ThunderChangeEvent` from the natural weather cycle is untested | M | p-blocks |
| 89 | `BlockExplodeEvent` fires only for `World.createExplosion` (not beds or respawn anchors) and its yield is not applied | S | p-blocks |
| 90 | Chunks load asynchronously: `getChunkAt`, `loadChunk` and plugin tickets load over the next ticks and an unloaded block reads as air (CraftBukkit loads on the spot) | L | p-blocks |
| 91 | A behaviour event's host refuses block writes; a plugin changing `BlockGrowEvent`'s (fade, form, spread) new state is not applied | M | p-blocks |
| 92 | `BlockBreakEvent.setDropItems(false)` also withholds the experience; `BlockPhysicsEvent`'s source block is the updated block itself; `BlockPlaceEvent.getItemInHand` is a plain stack of the placed block's item; multi-block places fire one event | S | p-blocks |
| 93 | `World.getSeaLevel` answers 63 (32 in the Nether) whatever the generator; spawn, time, weather, rules and border are shared by every world | S | p-blocks |
| 94 | Block-entity states other than signs throw on their own methods; `Block.getDrops`, `breakNaturally(ItemStack)` and container state methods are stubs; a block container's `getHolder()` is null | M | p-blocks, p-inv |

**Inventory**

| # | Gap | Size | Source |
|---|---|---|---|
| 95 | Menus the game runs itself (crafting table, anvil, furnace, enchanting, trading, stations) fire no click events, which blocks `CraftItemEvent`, `PrepareItemCraftEvent`, `FurnaceSmeltEvent`, `EnchantItemEvent`, `PrepareAnvilEvent` | L | p-inv |
| 96 | `PlayerItemDamageEvent` not fired: items wear in about eight separate places, with no single site | M | p-inv |
| 97 | `createInventory` for furnace, workbench and similar types, `ItemFactory.createItemStack`, `ItemMeta` component objects (food, tool, equippable) and `ItemType.getCompostChance` not supported | M | p-inv |
| 98 | `InventoryOpenEvent` on a double chest reports one half; `InventoryDragEvent` ignores a cursor the plugin sets; `PlayerItemConsumeEvent` ignores a replacement item | S | p-inv |
| 99 | `Items.isAir` stays a constant check instead of `Material.isAir()` (the block table is now off-thread safe after `2466e3e14`); behaviour is the same | S | p-inv, p-blocks |

**Server and bridge**

| # | Gap | Size | Source |
|---|---|---|---|
| 100 | Not bridged: keyed boss bars and `getBossBar(key)`, the scoreboard API, recipes, `PlayerProfile` textures, advancements, `ProxiedCommandSender`/`BlockCommandSender` (mostly `instanceof`-guarded in the corpus) | L | p-server |
| 101 | Events not fired: `ServerListPingEvent`, `ServerCommandEvent`, `RemoteServerCommandEvent` (with #30), `ServerLoadEvent` | S | p-server |
| 102 | `libraries:` in `plugin.yml` doesn't work (the shaded API lacks commons-lang3 and the Maven resolver); only HuskHomes in the corpus uses it | M | found, p-server |
| 103 | Teleports and vanilla commands for a player other than the one acting run at the next tick's start; a listener registered mid-tick takes effect next tick | M | found |
| 104 | Not isolated: a JVM crash takes the server down, and a plugin looping on the main thread stalls the tick (the watchdog only logs its stack after 10 s) | L | found, p-server |
| 105 | Legacy plugins (no `api-version`) load without legacy-material support | M | docs |
| 106 | Paper and Folia API (region and async schedulers, `teleportAsync`, `getChunkAtAsync`, Adventure `Component`) is not provided; 7 of 16 corpus plugins reach for some of it. Outside Spigot, listed so it isn't lost | L | p-server |
| 107 | Licence of the Java bridge undecided (Bukkit API GPL-3.0, server MIT; options in [plugins.md](plugins.md)). Nothing with the bridge ships before Doc decides | decision | found |

### Test and harness debt

| # | Gap | Where | Size | Source |
|---|---|---|---|---|
| 108 | `riding-jump-horse` and `riding-jump-camel` are still in `KNOWN` though cluster A's forward (`5ef828579`) and B's handler (`bfb5b4a89`) are both in; confirm they match and take them out | `tools/test_server_player_handlers_parity.py:100` | S | B, orch |
| 109 | The `standing` case ("held 3 blocks up") is timing-sensitive: on a loaded builder vanilla kicked the player and Ferraloom didn't, one run in three. Make it deterministic or drop it | `tools/test_hub_sync_parity.py:116` | S | orch |
| 110 | Vanilla reference servers stall in "Saving the game" on the builder's disk and their watchdog kills them, failing harnesses that never reached Ferraloom (`server_ops_feedback`, `server_entity_data`, `server_items`, `server_entities`, `server_combat`, `datafix`, `locator_bar`, `cursor_death`). Workaround: output dirs on `/dev/shm` (cleared on reboot). Durable fix: `sync-chunk-writes=false` on the reference servers (several harnesses set it to `true`) | `tools/test_*_parity.py` | S | orch, A |
| 111 | About half the parity suite can't build on the builder (default features need `libudev-dev` and `libasound2-dev`); `mob_ai` times out at 3600 s, so the `NoAI` change (#44, `fca21704f`) has no `mob_ai` evidence; `save_roundtrip` and `lan_e2e.py` need a window. About 225 harnesses had no result on `5d5371c7b` | builder CT107 | M (Doc's: packages, disk) | orch, D, combat |
| 112 | `tools/parity.py --changed-files` selects about every harness for any lib change, so agents pick harnesses by hand | `tools/parity.py` | M | combat, B, found |
| 113 | `server_player_handlers` takes about 30 minutes, the slowest harness added this run | `tools/test_server_player_handlers_parity.py` | S | orch |
| 114 | `hub_sync` was never shown failing before its fixes (the job never left the queue) | `tools/test_hub_sync_parity.py` | S | A |
| 115 | Plugin e2e timing: it sleeps 1 s between two digs (Horizon's creative destroy delay), and the scheduler's exact tick counts were checked in one run only | `tools/plugins_e2e.py` | S | found |
| 116 | No benchmark with plugins loaded (move, hopper, physics and combust listener costs are estimates or disassembly); no corpus plugin has been started on the server; no live check from a Prism client | `tools/server_bench.py` | M | orch, p-players, p-entities |
| 117 | Stale docs: `docs/server.md:94` "Left:" (guest arrows hit players, guests' kills are theirs); `docs/server-levels.md` "Not done yet" (entities do go through portals, the End poem is handled); the docstring of `tools/test_dedicated_server_parity.py:41` (guests' death messages are Horizon's now); `docs/ferraloom/plugins.md` says `setDropItems` is ignored, `Block.getType` reads only the level in place, `updateCommands` doesn't resend, and death drops are an empty list (all now wrong); `docs/architecture.md` lists none of this run's six new harnesses | docs | S | audit, fperm, orch |

## Fixed in this run

Audit numbers first, then gaps found on the way. Harness names are `tools/test_<name>_parity.py`.

| # | Fixed | Commit | Proof |
|---|---|---|---|
| 1 | Operators run the dedicated-server commands in game at vanilla's levels (3: `op`, `deop`, `ban`, `ban-ip`, `banlist`, `pardon`, `pardon-ip`, `whitelist`, `setidletimeout`, `transfer`; 4: `save-all`, `save-off`, `save-on`, `stop`); `/seed` and `/version` at 2, `/publish` gone. `/perf` stays open as #53 | `cd15d45f4` (branch), `horizon-command/src/tree_server.rs`, `game/commands/server_cmds.rs` | `server_ops_feedback` case `admin`; unit test `dedicated_tree_has_the_server_commands_at_vanilla_levels` |
| 2 | Other players see a player's hurt flash and tilt (`DamageEvent`); the hurt sound leaves out the hurt player, as vanilla. Entity ids open as #40 | `a726d718a`, `fd5824fb4`, `c5752b504` | `server_knockback` damage and fall cases |
| 3 | Player-stat criteria update for every server player; the host's `air` reads the body | `5264ba001` | `hub_sync` `criteria`, `criteria-change` |
| 4 | Spectators hidden from non-spectators | `8d766dc98` | `hub_sync` `spectator*`; unit test `hidden_entities_are_not_paired` |
| 5 | Spectating through entities and `/spectate` for any player. Leftovers #46 | `ae0ab67dd`, `d94fb949b` | `server_player_handlers`, 5 cases |
| 6 | Held-item attribute modifiers synced (sword attack cooldown), both hands, enchantment modifiers | `52ca49fc6` | `server_combat`, 11 of 11 |
| 7 | `allow-flight=false` kicks floating players; plugin-granted flight exempt | `ce841fb18`, `2c013dd9e` | `hub_sync` `standing`, `floating` |
| 8 | Ticks frozen synced to the player and trackers | `d7d3849d2` | `hub_sync` `freeze`, `thaw` |
| 9 | Riding jump: hub forwards actions 3 and 4, horses rear, camels and nautili dash. Leftovers #37, #47, #108 | `5ef828579`, `bfb5b4a89` | `server_player_handlers` (cases still in `KNOWN`, #108) |
| 10 | Chat and command spam kick; ops exempt | `ce841fb18` | `hub_sync` `chat-spam`, `command-spam`; unit throttle test |
| 11 | Stuck arrows and bee stingers on mobs and players; stings use the `sting` damage type and poison mobs | `914d93c8e`, `75c9337a8` | `server_entity_data` `arrow-mob`, `sting-mob`, `arrow-player`; unit `arrows::tests` |
| 12 | Shoulder parrots shown (indices 19, 20) | `d7d3849d2` | `hub_sync` snapshots |
| 13 | `/dialog show` and `/dialog clear` reach remote players | `672b8620b` | `server_player_handlers` |
| 14 | `/fillbiome` sends `ChunksBiomes` | `efe40ad9d` | `server_entity_data` `fillbiome` |
| 15 | Reach (`block_interaction_range` + 1) and click-location checks on block use, reach and height checks on digging | `c68ba828b` | `server_player_handlers`, 4 cases |
| 16 | "Height limit for building" overlay, sent twice on the top face as vanilla | `aecba7beb`, `d31bbcb53` | `server_player_handlers`, 2 cases |
| 17 | Code of conduct shown and accepted during configuration | `1bcb835b7` | `hub_sync` (every join) |
| 18 | Death-screen score | `3bbec62a2`, `6a79890b5` | `hub_sync` snapshots (`18=30`) |
| 19 | "Anonymous Player" in the status sample for unlisted players | `8488bb48e` | `hub_sync` status |
| 20 | Recipe-book highlights clear | `191be6e14`, `80a08878f` | `hub_sync` `recipe-seen`, `alice-rejoins` |
| 22 | F3+I tag queries for entities, players and block entities | `7ebf03205` | `server_player_handlers`, 2 cases |
| 23 | Play-state ping answered | `02e9f2d4f` | `hub_sync` `ping` |
| 24 | Advancements screen keeps its tab | `191be6e14`, `80a08878f` | `hub_sync` `tabs` |
| 25 | Hat layer in the tab list at join and on change | `8488bb48e` | `hub_sync` `bob-joins`, `bob-hat-on` |
| 27 | `/tp` of items, orbs, projectiles, crystals, clouds and player-ridden vehicles sends `TeleportEntity` (the audit's "mobs glide" was wrong). Other teleports open as #27 | `6385ae8ea` | `server_entity_data` `tp-item`, `tp-boat-rider` |
| 29 | A landing falling block sends its block to trackers | `94cc8ff28` | `server_entity_data` `falling-sand` |
| 31 | `function-permission-level` honoured (rest of #31 open) | see below | |
| 32 | Own effect swirls sent to self | `d7d3849d2` | `hub_sync` `after-criteria` |
| | `NoAI` mobs burn in daylight, push, age and tick their love timer; the push skips the mob's own rider and mount | `fca21704f` | `server_entity_data` `noai-sunburn` |
| | Full-strength sword hits by server players sweep | `d55ac126a` | `server_combat` |
| | An axe on a raised shield puts it on cooldown | `c3652796f` | `server_combat` |
| | Speed, Slowness, Haste and Mining Fatigue use vanilla's float-widened amounts | `b99a8e690` | `server_combat` |
| | Item components the server doesn't model (lore, custom data, unbreakable, custom model data, tooltip display, profile, item model, rarity, glint) reach clients, and styled custom names keep their colours in saves, commands and the wire | `5e90d2e68` | `server_items`, 13 of 13; `horizon-items` `item_data` tests |
| | `/transfer` sends the named players, not the sender; an unknown command's error no longer carries an empty context | `eb5a048a8` | `server_ops_feedback` `admin` |
| | A ban's source is the player's name; operators hear about a ban before the kick; a player's `/save-all` saves after their turn | `cd15d45f4` (branch) | `server_ops_feedback` `admin` |
| | Functions compile at `function-permission-level` at load and `/reload`; failing functions and the tags needing them drop with vanilla's messages; macro lines checked at call | `8ec12d527`, `3bf600a37` | `server_function_permission`, 33 lines |
| | `Player.updateCommands` resends command trees (to every player, where vanilla sends only that player's) | players area, `game/plugins/players.rs:607` | `plugins_e2e.py` |

## Protocol coverage at HEAD

The full serverbound and clientbound tables are in the audit commit `9b35c1726`. What is still not handled:

- **Serverbound dropped:** `SetStructureBlock`, `SetJigsawBlock`, `JigsawGenerate`, `SetTestBlock`,
  `TestInstanceBlockAction`, `SetCommandMinecart` (#21), `DebugSubscriptionRequest` (#63). `Pong` and
  `CookieResponse` are ignored as vanilla effectively does (no pings or cookies are sent).
  `ConfigurationAcknowledged` isn't needed (no `StartConfiguration` is sent).
- **Clientbound never sent:** `SOUND_ENTITY` (#26), `PLAYER_ROTATION`, `PLAYER_LOOK_AT` (#28) and the
  low-impact list in #63. `TELEPORT_ENTITY` is sent only from `/tp` (#27).
