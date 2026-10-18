# Ticking, game events and vibrations

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| ticking.server.packet-phase | Process queued packets before the server tick | `MinecraftServer#processPacketsAndTick` | | - | todo |
| ticking.server.rate-phase | Advance the server tick count and tick-rate manager before child ticks | `MinecraftServer#tickServer` | | - | todo |
| ticking.server.function-phase | Tick command functions before clocks and dimensions | `MinecraftServer#tickChildren` | | - | todo |
| ticking.server.clock-phase | Advance global world clocks only when simulation runs normally | `MinecraftServer#tickChildren; ServerClockManager#tick` | | - | todo |
| ticking.server.time-sync | Send periodic world-time synchronization before dimension ticks | `MinecraftServer#tickChildren; MinecraftServer#forceGameTimeSynchronization` | | - | todo |
| ticking.server.dimension-phase | Update effective respawn data and tick dimensions in server iteration order | `MinecraftServer#tickChildren` | | - | todo |
| ticking.server.connection-phase | Tick network connections after dimensions | `MinecraftServer#tickChildren; MinecraftServer#tickConnection` | | - | todo |
| ticking.server.player-phase | Tick player-list maintenance after connections | `MinecraftServer#tickChildren; PlayerList#tick` | | - | todo |
| ticking.server.debug-phase | Tick debug subscribers after player-list maintenance | `MinecraftServer#tickChildren` | | - | todo |
| ticking.server.gametest-phase | Tick GameTests after debug subscribers when simulation is running | `MinecraftServer#tickChildren` | | - | todo |
| ticking.server.gui-phase | Run registered GUI tickables after GameTests | `MinecraftServer#tickChildren` | | - | todo |
| ticking.server.chunk-send-phase | Send pending chunk batches and resume connection flushing after tickables | `MinecraftServer#tickChildren` | | - | todo |
| ticking.server.activity-phase | Tick the server activity monitor after chunk sending | `MinecraftServer#tickChildren` | | - | todo |
| ticking.server.status-autosave-phase | Refresh expired server status and perform due autosave after child ticks | `MinecraftServer#tickServer` | | - | todo |
| ticking.server.task-budget | Run queued tasks with vanilla tick-age and remaining-time rules | `MinecraftServer#shouldRun; MinecraftServer#pollTask` | | - | todo |
| ticking.server.freeze-step-sprint | Apply freeze, stepping, sprinting and changed tick rates to simulation eligibility | `ServerTickRateManager; TickRateManager` | | - | todo |
| ticking.level.environment-cache | Invalidate environment-attribute tick cache before world work | `ServerLevel#tick` | | - | todo |
| ticking.level.border-weather | Tick world border before weather when simulation runs normally | `ServerLevel#tick` | | - | todo |
| ticking.level.sleep | Resolve deep-sleep threshold, clock wake-up marker and weather reset before sky brightness | `ServerLevel#tick` | | - | todo |
| ticking.level.sky-time | Update sky brightness before game time and scheduled functions | `ServerLevel#tick; ServerLevel#tickTime` | | - | todo |
| ticking.level.scheduled-blocks | Run scheduled block ticks before scheduled fluid ticks with a 65536 tick budget each | `ServerLevel#tick` | | - | todo |
| ticking.level.raid-phase | Tick raids after scheduled ticks and before chunk source | `ServerLevel#tick` | | - | todo |
| ticking.level.chunk-phase | Tick the chunk source after raids | `ServerLevel#tick` | | - | todo |
| ticking.level.block-events | Run block events after chunk source and before entity ticking | `ServerLevel#tick; ServerLevel#runBlockEvents` | | - | todo |
| ticking.level.empty-time | Stop entity and block-entity ticking after the empty-time threshold without active tickets | `ServerLevel#tick` | | - | todo |
| ticking.level.dragon-phase | Tick the dragon fight before the world entity tick list | `ServerLevel#tick` | | - | todo |
| ticking.level.entity-order | Check removal, freezing, despawning and entity-ticking range before ticking each nonpassenger | `ServerLevel#tick` | | - | todo |
| ticking.level.passenger-order | Tick passengers recursively after their vehicle and detach invalid riding links | `ServerLevel#tickNonPassenger; ServerLevel#tickPassenger` | | - | todo |
| ticking.level.block-entity-phase | Tick block entities after entities with pending ticker additions deferred | `Level#tickBlockEntities; ServerLevel#tick` | | - | todo |
| ticking.level.entity-management | Tick persistent entity section management after entity and block-entity work | `ServerLevel#tick` | | - | todo |
| ticking.level.debug-gc-phase | Tick debug synchronizers and collect unused random-state cache entries last | `ServerLevel#tick` | | - | todo |
| ticking.chunk.ticket-phase | Purge stale tickets, then run distance-manager updates before chunk work | `ServerChunkCache#tick` | | - | todo |
| ticking.chunk.spawn-state | Count natural spawning state and determine eligible categories before selecting chunks | `ServerChunkCache#tickChunks` | | - | todo |
| ticking.chunk.spawn-shuffle | Collect and shuffle spawning chunks before inhabited-time, thunder and spawn processing | `ServerChunkCache#tickChunks; ServerChunkCache#tickSpawningChunk` | | - | todo |
| ticking.chunk.random-phase | Run random block and fluid ticks in block-ticking chunks after natural spawning | `ServerChunkCache#tickChunks; ServerLevel#tickChunk` | | - | todo |
| ticking.chunk.custom-spawner-phase | Run custom spawners after random ticks when mob spawning is enabled | `ServerChunkCache#tickChunks; ServerLevel#tickCustomSpawners` | | - | todo |
| ticking.chunk.broadcast-phase | Broadcast changed chunks after simulation, including when simulation is frozen | `ServerChunkCache#tickChunks; ServerChunkCache#broadcastChangedChunks` | | - | todo |
| ticking.chunk.map-unload-phase | Tick chunk map and unload work, then clear lookup cache | `ServerChunkCache#tick` | | - | todo |
| ticking.scheduled.order | Order each chunk tick queue by trigger time, priority and sub-tick order | `ScheduledTick#DRAIN_ORDER; LevelChunkTicks` | `ferraloom-game: ticks::tests::vanilla_container_drain_and_deduplication` | pass | todo |
| ticking.scheduled.deduplicate | Deduplicate scheduled ticks for the same position and type | `LevelChunkTicks#schedule` | `ferraloom-game: ticks::tests::vanilla_container_drain_and_deduplication` | pass | todo |
| ticking.scheduled.loaded-only | Run ticks only for eligible ticking chunks and defer unloaded containers | `LevelTicks#tick; LevelTicks#sortContainersToTick` | `ferraloom-game: ticks::tests::ineligible_containers_keep_ticks_and_unregistered_chunks_reject_them` | - | todo |
| ticking.scheduled.stale-type | Skip a scheduled tick if the block or fluid at its position no longer matches | `ServerLevel#tickBlock; ServerLevel#tickFluid` | | - | todo |
| ticking.scheduled.persist-delay | Preserve scheduled delays and relative ordering across chunk unload and reload | `LevelChunkTicks#pack; LevelChunkTicks#unpack; SavedTick` | `ferraloom-game: ticks::tests::vanilla_saved_ticks_pack_and_negative_unpack_order; level::tests::block_and_fluid_ticks_survive_unload_nbt_and_reload_with_their_remaining_delay` | pass | todo |
| ticking.scheduled.copy-clear | Copy and clear pending ticks with block-area clone operations and preserve ordering | `LevelTicks#copyAreaFrom; LevelTicks#clearArea` | | - | todo |
| ticking.block-events.queue | Deduplicate and process queued block events, including events queued by another event | `ServerLevel#blockEvent; ServerLevel#runBlockEvents` | `ferraloom-game: level::tests::block_events_drain_reentrant_additions_deduplicate_and_retry_non_ticking_chunks` | - | todo |
| ticking.block-events.retry | Defer events outside ticking chunks and reject events whose block has changed | `ServerLevel#runBlockEvents; ServerLevel#doBlockEvent` | `ferraloom-game: level::tests::block_events_drain_reentrant_additions_deduplicate_and_retry_non_ticking_chunks` | - | todo |
| ticking.random.section-selection | Sample positions per randomly ticking section using the level random-position sequence | `ServerLevel#tickChunk; Level#getBlockRandomPos` | `ferraloom-game: level::tests::random_position_lcg_matches_signed_java_vector (position sequence; phase integration pending)` | pass | todo |
| ticking.random.block-fluid-order | Invoke the sampled block random tick before its fluid random tick | `ServerLevel#tickChunk` | | - | todo |
| ticking.game-event.dispatch | Dispatch events to section listeners within event notification radius | `GameEventDispatcher#post; EuclideanGameEventListenerRegistry` | | - | todo |
| ticking.game-event.distance-order | Process distance-ordered listeners using source-to-listener distance | `GameEventDispatcher#post; GameEvent.ListenerInfo#compareTo` | | - | todo |
| ticking.vibration.frequency | Map game events and resonance events to vibration frequencies | `VibrationSystem#getGameEventFrequency` | | - | todo |
| ticking.vibration.selection | Choose pending vibrations using tick, distance and frequency tie-breaking | `VibrationSelector` | | - | todo |
| ticking.vibration.occlusion | Reject vibrations occluded by tagged blocks using the directional ray checks | `VibrationSystem.Listener#isOccluded` | | - | todo |
| ticking.vibration.sneaking | Filter tagged sneaking events and dampened source entities | `VibrationSystem.User#isValidVibration` | | - | todo |
| ticking.vibration.travel | Deliver vibrations after floored travel distance and restore travel state after reload | `VibrationSystem.Ticker#tick; VibrationSystem.Data` | | - | todo |
| ticking.vibration.adjacent-chunks | Defer delivery when a listener requires surrounding chunks to be ticking | `VibrationSystem.Ticker#areAdjacentChunksTicking` | | - | todo |
| ticking.vibration.signal-distance | Compute listener redstone strength from distance with minimum power one | `VibrationSystem#getRedstoneStrengthForDistance` | | - | todo |
| ticking.scheduled.cross-chunk-drain | Merge due chunk queues by priority and sub-tick order, excluding trigger time from the cross-chunk comparator | `ScheduledTick#INTRA_TICK_DRAIN_ORDER; LevelTicks#drainFromCurrentContainer` | `ferraloom-game: ticks::tests::vanilla_container_drain_and_deduplication` | pass | todo |
| ticking.game-event.block-activate | `minecraft:block_activate` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.block-attach | `minecraft:block_attach` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.block-change | `minecraft:block_change` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.block-close | `minecraft:block_close` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.block-deactivate | `minecraft:block_deactivate` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.block-destroy | `minecraft:block_destroy` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.block-detach | `minecraft:block_detach` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.block-open | `minecraft:block_open` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.block-place | `minecraft:block_place` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.bounce | `minecraft:bounce` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.container-close | `minecraft:container_close` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.container-open | `minecraft:container_open` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.drink | `minecraft:drink` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.eat | `minecraft:eat` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.elytra-glide | `minecraft:elytra_glide` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.entity-action | `minecraft:entity_action` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.entity-damage | `minecraft:entity_damage` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.entity-die | `minecraft:entity_die` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.entity-dismount | `minecraft:entity_dismount` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.entity-interact | `minecraft:entity_interact` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.entity-mount | `minecraft:entity_mount` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.entity-place | `minecraft:entity_place` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.equip | `minecraft:equip` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.explode | `minecraft:explode` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.flap | `minecraft:flap` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.fluid-pickup | `minecraft:fluid_pickup` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.fluid-place | `minecraft:fluid_place` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.hit-ground | `minecraft:hit_ground` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.instrument-play | `minecraft:instrument_play` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.item-interact-finish | `minecraft:item_interact_finish` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.item-interact-start | `minecraft:item_interact_start` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.jukebox-play | `minecraft:jukebox_play` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.jukebox-stop-play | `minecraft:jukebox_stop_play` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.lightning-strike | `minecraft:lightning_strike` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.note-block-play | `minecraft:note_block_play` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.prime-fuse | `minecraft:prime_fuse` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.projectile-land | `minecraft:projectile_land` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.projectile-shoot | `minecraft:projectile_shoot` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.resonate-1 | `minecraft:resonate_1` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.resonate-10 | `minecraft:resonate_10` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.resonate-11 | `minecraft:resonate_11` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.resonate-12 | `minecraft:resonate_12` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.resonate-13 | `minecraft:resonate_13` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.resonate-14 | `minecraft:resonate_14` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.resonate-15 | `minecraft:resonate_15` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.resonate-2 | `minecraft:resonate_2` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.resonate-3 | `minecraft:resonate_3` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.resonate-4 | `minecraft:resonate_4` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.resonate-5 | `minecraft:resonate_5` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.resonate-6 | `minecraft:resonate_6` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.resonate-7 | `minecraft:resonate_7` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.resonate-8 | `minecraft:resonate_8` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.resonate-9 | `minecraft:resonate_9` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.sculk-sensor-tendrils-clicking | `minecraft:sculk_sensor_tendrils_clicking` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.shear | `minecraft:shear` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.shriek | `minecraft:shriek` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.splash | `minecraft:splash` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.step | `minecraft:step` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.swim | `minecraft:swim` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.teleport | `minecraft:teleport` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.game-event.unequip | `minecraft:unequip` (game event) | `net.minecraft.world.level.gameevent.GameEvent#bootstrap; net.minecraft.world.level.gameevent.GameEvent` | | - | todo |
| ticking.position-source-type.block | `minecraft:block` (position source type) | `net.minecraft.world.level.gameevent.PositionSourceType#register` | | - | todo |
| ticking.position-source-type.entity | `minecraft:entity` (position source type) | `net.minecraft.world.level.gameevent.PositionSourceType#register` | | - | todo |

| ticking.scheduled.reentrant-collection | Collect before execution and expose only remaining ticks through willTickThisTick | `LevelTicks#collectTicks; LevelTicks#willTickThisTick` | `ferraloom-game: ticks::tests::vanilla_reentrant_collection_and_will_tick` | pass | todo |

| ticking.level.set-block-flags | Apply 26.3 setBlock flags, sand onPlace scheduling, section emptiness and live heightmaps | `Level#setBlock; LevelChunk#setBlockState` | `ferraloom-game: level::tests::vanilla_set_block_flags_schedule_sand_notify_and_update_all_heightmaps` | pass | todo |
| ticking.level.block-broadcasts | Deduplicate changed positions and capture block or section packets at the broadcast phase | `ChunkHolder#blockChanged; ChunkHolder#broadcastChanges` | `ferraloom-game: level::tests::block_broadcasts_deduplicate_and_capture_final_states_at_broadcast_time (server audience drain pending)` | - | todo |

| ticking.level.synchronous-chunk-read | Load or merge missing FULL chunks at the synchronous read call site | `Level#getChunk; ServerChunkCache#getChunk` | Known limitation: live core returns explicit `MissingChunk`; synchronous continuation belongs to the next adapter port. No air substitution or callback retry. | - | todo |

| ticking.simulation-chunk-order | Propagate player simulation ticket levels through fixed-point queues and retain vanilla primitive-map iteration order | `SimulationChunkTracker; ChunkTracker; DynamicGraphMinFixedPoint` | `ferraloom-game: level::simulation::tests::simulation_tracker_map_order_and_levels_match_vanilla_ticket_changes` | pass | todo |
| ticking.unported-behavior-policy | Strict by default; optional Warn skips count every call including shape updates and retain required-query errors | `BlockBehaviour virtual dispatch (temporary port policy)` | `ferraloom-game: level::tests::strict_is_default_and_warn_records_each_skipped_call_without_query_fallbacks` | - | todo |
| ticking.falling.waterlogged-support | Keep sand above waterlogged solid blocks; free space uses the liquid fact rather than contained fluid | `FallingBlock#isFree` | `ferraloom-game: level::tests::falling_sand_stays_on_a_waterlogged_solid_block` | pass | todo |
| ticking.weather.cycle-and-packets | Advance rain/thunder timers in canonical RNG order, smooth levels, emit transition packets and preserve 26.3 WeatherData | `ServerLevel#advanceWeatherCycle; WeatherData` | `ferraloom-game: level::weather::tests (server persistence wiring pending)` | - | todo |
| ticking.block.shared-shape-dispatch | Share native collision, outline and support queries through concrete contexts; preserve base full outline, hasCollision delegation and Air override | `BlockStateBase#getCollisionShape; BlockBehaviour#getShape; AirBlock#getShape; SupportType.FULL` | `ferraloom-game: level::tests::shared_collision_queries_use_native_faces_and_liquid_context_without_warn_fallbacks; reflected_method_ownership_ports_inherited_defaults_without_masking_overrides` | - | todo |
| ticking.tick-rate.freeze-step | Freeze simulation, run the final stepped tick, and retain player/player-passenger entity exceptions | `TickRateManager; ServerTickRateManager#stepGameIfPaused` | `ferraloom-game: level::tick_rate::tests::rate_math_and_freeze_step_state_match_real_vanilla` | pass | todo |
| ticking.tick-rate.sprint | Count sprint work, restore the previous freeze state, report throughput and emit state changes in order | `ServerTickRateManager#finishTickSprint` | `ferraloom-game: level::tick_rate::tests::sprint_reports_match_java_formatter_and_restore_frozen_state_after_final_work` | pass | todo |

| ticking.level.command-update-modes | Apply setblock replace/destroy/keep/strict and fill replace/destroy/keep/outline/hollow/strict in a frozen world | `SetBlockCommand; FillCommand; BlockInput; ServerLevel#updateNeighboursOnBlockSet` | `ferraloom-parity: parity_setblock_and_fill_update_modes (flat/commands/live integration pending)` | - | todo |
| ticking.falling.sand-column | Settle a sand column through FallingBlockEntity after the synchronous removal hook | `FallingBlock; FallingBlockEntity` | `ferraloom-parity: parity_falling_sand_column (falling entity hook integration pending)` | - | todo |
| ticking.block.method-ownership | Dispatch each virtual to its reflected vanilla base owner or explicitly registered overriding port | `BlockBehaviour; Block (method ownership extracted by exact signature)` | `ferraloom-game: level::tests::reflected_method_ownership_ports_inherited_defaults_without_masking_overrides; ferraloom-data: tests::entity_inside_and_landing_virtual_owners_are_extracted_by_signature` | pass | todo |
| ticking.block.collision-property | Preserve hasCollision independently of empty-context collision geometry | `BlockBehaviour#hasCollision; BlockBehaviour#getCollisionShape` | `ferraloom-data: tests::collision_property_is_independent_of_the_empty_context_shape` | pass | todo |
