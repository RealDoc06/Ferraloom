# Weather, time and environment

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| weather-time.time.tick-time | Advance shared gameTime at the overworld tickTime slot, independently of advance_time | `ServerLevel#tickTime; MinecraftServer#tickChildren` | [weather_time::stepped_time_and_daylight_rule_packets_match_vanilla](../crates/ferraloom-parity/tests/weather_time.rs) (Strict both-server, three stepped 40-tick windows) | pass | pass |
| weather-time.time.daylight-rule | Pause world clocks under advance_time while gameTime continues; synchronize effective clock rates on rule changes | `ServerClockManager#tick/createFullSyncPacket; MinecraftServer#onGameRuleChanged` | [weather_time::stepped_time_and_daylight_rule_packets_match_vanilla](../crates/ferraloom-parity/tests/weather_time.rs) (Strict both-server, both built-in clocks, enabled/disabled/enabled) | pass | pass |
| weather-time.time.periodic-packets | Send gameTime packets with an empty clock-update map, including while frozen | `MinecraftServer#forceGameTimeSynchronization` | [weather_time::stepped_time_and_daylight_rule_packets_match_vanilla](../crates/ferraloom-parity/tests/weather_time.rs) (Strict both-server, observed stepped-window packets and frozen endpoint) | pass | pass |
| weather-time.clocks.independent | Maintain multiple registered world clocks with independent tick rate and pause state | `ServerClockManager` | | - | todo |
| weather-time.clocks.markers | Resolve named time markers and move clocks using marker-aware command and sleep operations | `ServerClockManager#moveToTimeMarker; ClockTimeMarkers` | | - | todo |
| weather-time.clocks.persist-sync | Persist total ticks, rate and paused state and synchronize full clock state to clients | `ServerClockManager#packState; ServerClockManager#createFullSyncPacket` | | - | todo |
| weather-time.weather.cycles | Advance clear, rain and thunder durations only under the weather-cycle rule | `ServerLevel#advanceWeatherCycle` | [weather_time::stepped_weather_levels_transitions_and_rule_match_vanilla](../crates/ferraloom-parity/tests/weather_time.rs) (Strict stepped weather packets; unrun) | - | partial |
| weather-time.weather.strength | Interpolate rain and thunder strength and broadcast transitions | `ServerLevel#advanceWeatherCycle` | [weather_time::stepped_weather_levels_transitions_and_rule_match_vanilla](../crates/ferraloom-parity/tests/weather_time.rs) (Strict stepped weather packets; unrun) | - | partial |
| weather-time.weather.lightning | Choose lightning targets from rods and eligible entities with skeleton-trap conditions | `ServerLevel#tickThunder; ServerLevel#findLightningTargetAround` | | - | todo |
| weather-time.weather.precipitation | Freeze water and accumulate or melt snow under biome and environment conditions | `ServerLevel#tickPrecipitation` | [weather_time::cold_precipitation_freezes_water_and_caps_snow_layers](../crates/ferraloom-parity/tests/weather_time.rs) (Strict real-biome edge water and capped layers; unrun) | - | partial |
| weather-time.weather.biome-exposure | Require local precipitation eligibility and sky exposure for rain interactions | `Level#isRainingAt; Biome` | | - | todo |
| weather-time.environment.layers | Combine dimension, biome, default and timeline attribute layers in order | `EnvironmentAttributeSystem` | | - | todo |
| weather-time.environment.positional-cache | Evaluate positional and tick-dependent attributes with cache invalidation | `EnvironmentAttributeSystem#invalidateTickCache; EnvironmentAttributeSystem#getValue` | | - | todo |
| weather-time.environment.timeline | Interpolate timeline values across clock keyframes and periods | `Timeline` | | - | todo |
| weather-time.difficulty.regional | Compute local difficulty from world age, inhabited time, moon and base difficulty | `DifficultyInstance; ServerLevel#getCurrentDifficultyAt` | | - | todo |
| weather-time.border.interpolation | Interpolate border resizing over time and notify listeners of size, center and warning changes | `WorldBorder#lerpSizeBetween; WorldBorder#tick; BorderChangeListener` | | - | todo |
| weather-time.attribute-type.activity | `minecraft:activity` (attribute type) | `net.minecraft.world.attribute.AttributeTypes#bootstrap` | | - | todo |
| weather-time.attribute-type.ambient-particles | `minecraft:ambient_particles` (attribute type) | `net.minecraft.world.attribute.AttributeTypes#bootstrap` | | - | todo |
| weather-time.attribute-type.ambient-sounds | `minecraft:ambient_sounds` (attribute type) | `net.minecraft.world.attribute.AttributeTypes#bootstrap` | | - | todo |
| weather-time.attribute-type.angle-degrees | `minecraft:angle_degrees` (attribute type) | `net.minecraft.world.attribute.AttributeTypes#bootstrap` | | - | todo |
| weather-time.attribute-type.argb-color | `minecraft:argb_color` (attribute type) | `net.minecraft.world.attribute.AttributeTypes#bootstrap` | | - | todo |
| weather-time.attribute-type.background-music | `minecraft:background_music` (attribute type) | `net.minecraft.world.attribute.AttributeTypes#bootstrap` | | - | todo |
| weather-time.attribute-type.bed-rule | `minecraft:bed_rule` (attribute type) | `net.minecraft.world.attribute.AttributeTypes#bootstrap` | | - | todo |
| weather-time.attribute-type.boolean | `minecraft:boolean` (attribute type) | `net.minecraft.world.attribute.AttributeTypes#bootstrap` | | - | todo |
| weather-time.attribute-type.float | `minecraft:float` (attribute type) | `net.minecraft.world.attribute.AttributeTypes#bootstrap` | | - | todo |
| weather-time.attribute-type.integer | `minecraft:integer` (attribute type) | `net.minecraft.world.attribute.AttributeTypes#bootstrap` | | - | todo |
| weather-time.attribute-type.mob-spawn-settings | `minecraft:mob_spawn_settings` (attribute type) | `net.minecraft.world.attribute.AttributeTypes#bootstrap` | | - | todo |
| weather-time.attribute-type.moon-phase | `minecraft:moon_phase` (attribute type) | `net.minecraft.world.attribute.AttributeTypes#bootstrap` | | - | todo |
| weather-time.attribute-type.particle | `minecraft:particle` (attribute type) | `net.minecraft.world.attribute.AttributeTypes#bootstrap` | | - | todo |
| weather-time.attribute-type.rgb-color | `minecraft:rgb_color` (attribute type) | `net.minecraft.world.attribute.AttributeTypes#bootstrap` | | - | todo |
| weather-time.attribute-type.tri-state | `minecraft:tri_state` (attribute type) | `net.minecraft.world.attribute.AttributeTypes#bootstrap` | | - | todo |
| weather-time.environment-attribute.audio.ambient-sounds | `minecraft:audio/ambient_sounds` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.audio.background-music | `minecraft:audio/background_music` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.audio.firefly-bush-sounds | `minecraft:audio/firefly_bush_sounds` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.audio.music-volume | `minecraft:audio/music_volume` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.baby-villager-activity | `minecraft:gameplay/baby_villager_activity` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.bed-rule | `minecraft:gameplay/bed_rule` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.bees-stay-in-hive | `minecraft:gameplay/bees_stay_in_hive` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.can-pillager-patrol-spawn | `minecraft:gameplay/can_pillager_patrol_spawn` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.can-start-raid | `minecraft:gameplay/can_start_raid` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.cat-waking-up-gift-chance | `minecraft:gameplay/cat_waking_up_gift_chance` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.creaking-active | `minecraft:gameplay/creaking_active` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.creature-world-gen-spawn-probability | `minecraft:gameplay/creature_world_gen_spawn_probability` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.eyeblossom-open | `minecraft:gameplay/eyeblossom_open` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.fast-lava | `minecraft:gameplay/fast_lava` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.increased-fire-burnout | `minecraft:gameplay/increased_fire_burnout` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.monsters-burn | `minecraft:gameplay/monsters_burn` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.natural-mob-spawns | `minecraft:gameplay/natural_mob_spawns` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.nether-portal-spawns-piglin | `minecraft:gameplay/nether_portal_spawns_piglin` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.piglins-zombify | `minecraft:gameplay/piglins_zombify` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.respawn-anchor-works | `minecraft:gameplay/respawn_anchor_works` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.sky-light-level | `minecraft:gameplay/sky_light_level` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.snow-golem-melts | `minecraft:gameplay/snow_golem_melts` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.straw-bed-rule | `minecraft:gameplay/straw_bed_rule` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.surface-slime-spawn-chance | `minecraft:gameplay/surface_slime_spawn_chance` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.turtle-egg-hatch-chance | `minecraft:gameplay/turtle_egg_hatch_chance` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.villager-activity | `minecraft:gameplay/villager_activity` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.gameplay.water-evaporates | `minecraft:gameplay/water_evaporates` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.ambient-light-color | `minecraft:visual/ambient_light_color` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.ambient-particles | `minecraft:visual/ambient_particles` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.block-light-tint | `minecraft:visual/block_light_tint` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.cloud-color | `minecraft:visual/cloud_color` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.cloud-fog-end-distance | `minecraft:visual/cloud_fog_end_distance` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.cloud-height | `minecraft:visual/cloud_height` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.default-dripstone-particle | `minecraft:visual/default_dripstone_particle` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.fog-color | `minecraft:visual/fog_color` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.fog-end-distance | `minecraft:visual/fog_end_distance` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.fog-start-distance | `minecraft:visual/fog_start_distance` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.moon-angle | `minecraft:visual/moon_angle` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.moon-phase | `minecraft:visual/moon_phase` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.night-vision-color | `minecraft:visual/night_vision_color` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.sky-color | `minecraft:visual/sky_color` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.sky-fog-end-distance | `minecraft:visual/sky_fog_end_distance` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.sky-light-color | `minecraft:visual/sky_light_color` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.sky-light-factor | `minecraft:visual/sky_light_factor` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.star-angle | `minecraft:visual/star_angle` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.star-brightness | `minecraft:visual/star_brightness` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.sun-angle | `minecraft:visual/sun_angle` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.sunrise-sunset-color | `minecraft:visual/sunrise_sunset_color` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.water-fog-color | `minecraft:visual/water_fog_color` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.water-fog-end-distance | `minecraft:visual/water_fog_end_distance` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.environment-attribute.visual.water-fog-start-distance | `minecraft:visual/water_fog_start_distance` (environment attribute) | `net.minecraft.world.attribute.EnvironmentAttributes#bootstrap` | | - | todo |
| weather-time.timeline.day | `minecraft:day` (timeline) | `data/minecraft/timeline/day.json`; `Timeline#DIRECT_CODEC` | | - | todo |
| weather-time.timeline.early-game | `minecraft:early_game` (timeline) | `data/minecraft/timeline/early_game.json`; `Timeline#DIRECT_CODEC` | | - | todo |
| weather-time.timeline.moon | `minecraft:moon` (timeline) | `data/minecraft/timeline/moon.json`; `Timeline#DIRECT_CODEC` | | - | todo |
| weather-time.timeline.villager-schedule | `minecraft:villager_schedule` (timeline) | `data/minecraft/timeline/villager_schedule.json`; `Timeline#DIRECT_CODEC` | | - | todo |
| weather-time.world-clock.overworld | `minecraft:overworld` (world clock) | `data/minecraft/world_clock/overworld.json`; `WorldClock#DIRECT_CODEC` | | - | todo |
| weather-time.world-clock.the-end | `minecraft:the_end` (world clock) | `data/minecraft/world_clock/the_end.json`; `WorldClock#DIRECT_CODEC` | | - | todo |
