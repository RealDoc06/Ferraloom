# Statistics

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| statistics.counter.increment | Increment typed statistics and synchronize dirty values to the owning player | `ServerStatsCounter; StatType; ServerPlayer#awardStat` | `stats::tests::vanilla_counter_arithmetic_same_value_writes_and_requests` | pass | partial |
| statistics.counter.persist | Save namespaced stat values to player JSON and datafix older statistic formats | `ServerStatsCounter` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.counter.scoreboard | Update linked scoreboard objectives when statistics change | `ServerPlayer#awardStat; ServerScoreboard` | `stats::tests::live_stat_counter_and_scoreboard_update_in_native_order` | pass | partial |
| statistics.movement.distance | Count movement distances by locomotion type and vehicle type using vanilla rounding | `ServerPlayer#checkMovementStatistics; ServerPlayer#checkRidingStatistics` | | - | todo |
| statistics.items.blocks | Award mining, crafting, using, breaking, pickup and drop statistics from their behavior hooks | `Stats; ServerPlayer#awardStat` | | - | todo |
| statistics.custom-stat.animals-bred | `minecraft:animals_bred` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.aviate-one-cm | `minecraft:aviate_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.bell-ring | `minecraft:bell_ring` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.boat-one-cm | `minecraft:boat_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.clean-armor | `minecraft:clean_armor` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.clean-banner | `minecraft:clean_banner` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.clean-shulker-box | `minecraft:clean_shulker_box` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.climb-one-cm | `minecraft:climb_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.crouch-one-cm | `minecraft:crouch_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.damage-absorbed | `minecraft:damage_absorbed` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.damage-blocked-by-shield | `minecraft:damage_blocked_by_shield` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.damage-dealt | `minecraft:damage_dealt` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.damage-dealt-absorbed | `minecraft:damage_dealt_absorbed` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.damage-dealt-resisted | `minecraft:damage_dealt_resisted` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.damage-resisted | `minecraft:damage_resisted` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.damage-taken | `minecraft:damage_taken` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.deaths | `minecraft:deaths` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.drop | `minecraft:drop` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.eat-cake-slice | `minecraft:eat_cake_slice` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.enchant-item | `minecraft:enchant_item` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.fall-one-cm | `minecraft:fall_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.fill-cauldron | `minecraft:fill_cauldron` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.fish-caught | `minecraft:fish_caught` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.fly-one-cm | `minecraft:fly_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.happy-ghast-one-cm | `minecraft:happy_ghast_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.horse-one-cm | `minecraft:horse_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.inspect-dispenser | `minecraft:inspect_dispenser` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.inspect-dropper | `minecraft:inspect_dropper` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.inspect-hopper | `minecraft:inspect_hopper` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.interact-with-anvil | `minecraft:interact_with_anvil` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.interact-with-beacon | `minecraft:interact_with_beacon` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.interact-with-blast-furnace | `minecraft:interact_with_blast_furnace` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.interact-with-brewingstand | `minecraft:interact_with_brewingstand` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.interact-with-campfire | `minecraft:interact_with_campfire` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.interact-with-cartography-table | `minecraft:interact_with_cartography_table` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.interact-with-crafting-table | `minecraft:interact_with_crafting_table` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.interact-with-furnace | `minecraft:interact_with_furnace` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.interact-with-grindstone | `minecraft:interact_with_grindstone` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.interact-with-lectern | `minecraft:interact_with_lectern` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.interact-with-loom | `minecraft:interact_with_loom` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.interact-with-smithing-table | `minecraft:interact_with_smithing_table` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.interact-with-smoker | `minecraft:interact_with_smoker` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.interact-with-stonecutter | `minecraft:interact_with_stonecutter` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.jump | `minecraft:jump` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.leave-game | `minecraft:leave_game` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.minecart-one-cm | `minecraft:minecart_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.mob-kills | `minecraft:mob_kills` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.nautilus-one-cm | `minecraft:nautilus_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.open-barrel | `minecraft:open_barrel` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.open-chest | `minecraft:open_chest` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.open-enderchest | `minecraft:open_enderchest` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.open-shulker-box | `minecraft:open_shulker_box` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.pig-one-cm | `minecraft:pig_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.play-noteblock | `minecraft:play_noteblock` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.play-record | `minecraft:play_record` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.play-time | `minecraft:play_time` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.player-kills | `minecraft:player_kills` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.pot-flower | `minecraft:pot_flower` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.raid-trigger | `minecraft:raid_trigger` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.raid-win | `minecraft:raid_win` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.sleep-in-bed | `minecraft:sleep_in_bed` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.sleep-in-straw-bed | `minecraft:sleep_in_straw_bed` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.sneak-time | `minecraft:sneak_time` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.sprint-one-cm | `minecraft:sprint_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.strider-one-cm | `minecraft:strider_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.swim-one-cm | `minecraft:swim_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.talked-to-villager | `minecraft:talked_to_villager` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.target-hit | `minecraft:target_hit` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.time-since-death | `minecraft:time_since_death` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.time-since-rest | `minecraft:time_since_rest` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.total-world-time | `minecraft:total_world_time` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.traded-with-villager | `minecraft:traded_with_villager` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.trigger-trapped-chest | `minecraft:trigger_trapped_chest` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.tune-noteblock | `minecraft:tune_noteblock` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.use-cauldron | `minecraft:use_cauldron` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.walk-on-water-one-cm | `minecraft:walk_on_water_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.walk-one-cm | `minecraft:walk_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.custom-stat.walk-under-water-one-cm | `minecraft:walk_under_water_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.stat-type.broken | `minecraft:broken` (stat type) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.stat-type.crafted | `minecraft:crafted` (stat type) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.stat-type.custom | `minecraft:custom` (stat type) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.stat-type.dropped | `minecraft:dropped` (stat type) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.stat-type.killed | `minecraft:killed` (stat type) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.stat-type.killed-by | `minecraft:killed_by` (stat type) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.stat-type.mined | `minecraft:mined` (stat type) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.stat-type.picked-up | `minecraft:picked_up` (stat type) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.stat-type.used | `minecraft:used` (stat type) | `net.minecraft.stats.Stats#register` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |

| statistics.counter.request-dirty | REQUEST_STATS drains only dirty values, including unchanged writes, and still sends an empty packet | `ServerStatsCounter#sendStats; ServerGamePacketListenerImpl#handleClientCommand` | `stats::tests::vanilla_counter_arithmetic_same_value_writes_and_requests` | pass | partial |
| statistics.counter.partial-json | Keep valid siblings when statistic names or values fail codec parsing | `ServerStatsCounter.STATS_CODEC` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | pass |
| statistics.counter.jdk25-copy-order | Preserve JDK 25 HashSet copy capacities when draining 12, 24 or all custom dirty statistics into the packet map | `ServerStatsCounter#getDirty; HashSet(Collection)` | `stats::tests::vanilla_dirty_packets_follow_jdk25_copy_capacity_boundaries` | pass | pass |
| statistics.counter.loaded-map-order | Preserve JDK 25 per-group putAll and fastutil destination sizing when loading saved statistics | `ServerStatsCounter.STATS_CODEC; ServerStatsCounter#parse` | `stats::tests::vanilla_dirty_packets_follow_jdk25_copy_capacity_boundaries` | pass | pass |

| statistics.request-stats.live | REQUEST_STATS sends and clears the canonical player dirty set through ordered player output | `ServerGamePacketListenerImpl#handleClientCommand` | `stats::tests` | pass | partial |

| statistics.live-counter-scoreboard-order | Player award/reset updates the canonical counter before matching objectives synchronously; counter saturation and score wrapping retain native delta semantics | `ServerPlayer#awardStat; ServerPlayer#resetStat` | `stats::tests::live_stat_counter_and_scoreboard_update_in_native_order` | pass | partial |

| statistics.parity.consumed-chest-open | Bedrock flat Strict chest interaction awards OPEN_CHEST and REQUEST_STATS clears its dirty entry | `ChestBlock#useWithoutItem; ServerPlayer#awardStat` | `ferraloom-parity::opening_chest_awards_and_flushes_statistics` | pass | todo |

| statistics.join-dirty-loaded | Joining marks all loaded statistics dirty before initial recipe packets, for the first REQUEST_STATS response | `PlayerList#placeNewPlayer` | `stats::tests::vanilla_stat_registries_json_and_dirty_packets` | pass | partial |
| statistics.persistence.legacy-name-file | Native locateStatsFile renames valid name-based statistics files to UUID paths and retains the source path if moving fails; deferred with pre-UUID save/datafix compatibility | `PlayerList#locateStatsFile` | | - | todo |
| statistics.parity.loaded-save-request | Bedrock flat Strict scenario sends a loaded statistic once per join and retains its exact value through save/restart | `PlayerList#getPlayerStats; PlayerList#save; ServerGamePacketListenerImpl#handleClientCommand` | `ferraloom-parity::loaded_statistics_and_advancement_tab_survive_save` | pass | pass |
| statistics.persistence.level-resource | UUID saves use players/stats and the shared player-progress path helper | `LevelResource.PLAYER_STATS_DIR` | `advancements::persistence::tests::captured_save_uses_each_instant_offset_across_dst` | pass | pass |
| statistics.persistence.old-layout-migration | Migrate older statistics layouts through the filefix workstream; no fallback reader | `PlayerStorageFileFix` | pending | - | todo |
