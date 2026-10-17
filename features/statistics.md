# Statistics

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| statistics.counter.increment | Increment typed statistics and synchronize dirty values to the owning player | `ServerStatsCounter; StatType; ServerPlayer#awardStat` | | - | todo |
| statistics.counter.persist | Save namespaced stat values to player JSON and datafix older statistic formats | `ServerStatsCounter` | | - | todo |
| statistics.counter.scoreboard | Update linked scoreboard objectives when statistics change | `ServerPlayer#awardStat; ServerScoreboard` | | - | todo |
| statistics.movement.distance | Count movement distances by locomotion type and vehicle type using vanilla rounding | `ServerPlayer#checkMovementStatistics; ServerPlayer#checkRidingStatistics` | | - | todo |
| statistics.items.blocks | Award mining, crafting, using, breaking, pickup and drop statistics from their behavior hooks | `Stats; ServerPlayer#awardStat` | | - | todo |
| statistics.custom-stat.animals-bred | `minecraft:animals_bred` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.aviate-one-cm | `minecraft:aviate_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.bell-ring | `minecraft:bell_ring` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.boat-one-cm | `minecraft:boat_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.clean-armor | `minecraft:clean_armor` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.clean-banner | `minecraft:clean_banner` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.clean-shulker-box | `minecraft:clean_shulker_box` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.climb-one-cm | `minecraft:climb_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.crouch-one-cm | `minecraft:crouch_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.damage-absorbed | `minecraft:damage_absorbed` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.damage-blocked-by-shield | `minecraft:damage_blocked_by_shield` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.damage-dealt | `minecraft:damage_dealt` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.damage-dealt-absorbed | `minecraft:damage_dealt_absorbed` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.damage-dealt-resisted | `minecraft:damage_dealt_resisted` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.damage-resisted | `minecraft:damage_resisted` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.damage-taken | `minecraft:damage_taken` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.deaths | `minecraft:deaths` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.drop | `minecraft:drop` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.eat-cake-slice | `minecraft:eat_cake_slice` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.enchant-item | `minecraft:enchant_item` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.fall-one-cm | `minecraft:fall_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.fill-cauldron | `minecraft:fill_cauldron` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.fish-caught | `minecraft:fish_caught` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.fly-one-cm | `minecraft:fly_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.happy-ghast-one-cm | `minecraft:happy_ghast_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.horse-one-cm | `minecraft:horse_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.inspect-dispenser | `minecraft:inspect_dispenser` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.inspect-dropper | `minecraft:inspect_dropper` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.inspect-hopper | `minecraft:inspect_hopper` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.interact-with-anvil | `minecraft:interact_with_anvil` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.interact-with-beacon | `minecraft:interact_with_beacon` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.interact-with-blast-furnace | `minecraft:interact_with_blast_furnace` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.interact-with-brewingstand | `minecraft:interact_with_brewingstand` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.interact-with-campfire | `minecraft:interact_with_campfire` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.interact-with-cartography-table | `minecraft:interact_with_cartography_table` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.interact-with-crafting-table | `minecraft:interact_with_crafting_table` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.interact-with-furnace | `minecraft:interact_with_furnace` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.interact-with-grindstone | `minecraft:interact_with_grindstone` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.interact-with-lectern | `minecraft:interact_with_lectern` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.interact-with-loom | `minecraft:interact_with_loom` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.interact-with-smithing-table | `minecraft:interact_with_smithing_table` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.interact-with-smoker | `minecraft:interact_with_smoker` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.interact-with-stonecutter | `minecraft:interact_with_stonecutter` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.jump | `minecraft:jump` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.leave-game | `minecraft:leave_game` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.minecart-one-cm | `minecraft:minecart_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.mob-kills | `minecraft:mob_kills` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.nautilus-one-cm | `minecraft:nautilus_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.open-barrel | `minecraft:open_barrel` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.open-chest | `minecraft:open_chest` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.open-enderchest | `minecraft:open_enderchest` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.open-shulker-box | `minecraft:open_shulker_box` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.pig-one-cm | `minecraft:pig_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.play-noteblock | `minecraft:play_noteblock` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.play-record | `minecraft:play_record` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.play-time | `minecraft:play_time` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.player-kills | `minecraft:player_kills` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.pot-flower | `minecraft:pot_flower` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.raid-trigger | `minecraft:raid_trigger` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.raid-win | `minecraft:raid_win` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.sleep-in-bed | `minecraft:sleep_in_bed` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.sleep-in-straw-bed | `minecraft:sleep_in_straw_bed` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.sneak-time | `minecraft:sneak_time` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.sprint-one-cm | `minecraft:sprint_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.strider-one-cm | `minecraft:strider_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.swim-one-cm | `minecraft:swim_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.talked-to-villager | `minecraft:talked_to_villager` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.target-hit | `minecraft:target_hit` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.time-since-death | `minecraft:time_since_death` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.time-since-rest | `minecraft:time_since_rest` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.total-world-time | `minecraft:total_world_time` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.traded-with-villager | `minecraft:traded_with_villager` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.trigger-trapped-chest | `minecraft:trigger_trapped_chest` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.tune-noteblock | `minecraft:tune_noteblock` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.use-cauldron | `minecraft:use_cauldron` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.walk-on-water-one-cm | `minecraft:walk_on_water_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.walk-one-cm | `minecraft:walk_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.custom-stat.walk-under-water-one-cm | `minecraft:walk_under_water_one_cm` (custom stat) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.stat-type.broken | `minecraft:broken` (stat type) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.stat-type.crafted | `minecraft:crafted` (stat type) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.stat-type.custom | `minecraft:custom` (stat type) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.stat-type.dropped | `minecraft:dropped` (stat type) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.stat-type.killed | `minecraft:killed` (stat type) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.stat-type.killed-by | `minecraft:killed_by` (stat type) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.stat-type.mined | `minecraft:mined` (stat type) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.stat-type.picked-up | `minecraft:picked_up` (stat type) | `net.minecraft.stats.Stats#register` | | - | todo |
| statistics.stat-type.used | `minecraft:used` (stat type) | `net.minecraft.stats.Stats#register` | | - | todo |
