# Spawning, trial spawners and vaults

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| spawning.natural.global-cap | Count eligible nonpersistent mobs and scale category caps by spawnable chunks | `NaturalSpawner#createState; NaturalSpawner.SpawnState#canSpawnForCategoryGlobal` | | - | todo |
| spawning.natural.local-cap | Apply player-local mob caps for overlapping spawn regions | `LocalMobCapCalculator; NaturalSpawner.SpawnState#canSpawnForCategoryLocal` | | - | todo |
| spawning.natural.distance | Apply player and world-spawn exclusion distance and outer spawn range | `NaturalSpawner#isRightDistanceToPlayerAndSpawnPoint` | | - | todo |
| spawning.natural.category-cadence | Apply persistent-category cadence and hostile or friendly category filtering | `ServerChunkCache#tickChunks; NaturalSpawner#getFilteredSpawningCategories` | | - | todo |
| spawning.natural.environment-table | Choose weighted spawn entries from environment attributes with structure overrides | `NaturalSpawner#getRandomSpawnMobAt; NaturalSpawner#mobsAt` | | - | todo |
| spawning.natural.pack-attempts | Perform pack attempts, per-mob group-size limits and shared spawn data | `NaturalSpawner#spawnCategoryForPosition` | | - | todo |
| spawning.natural.spawn-cost | Apply biome spawn-potential charges and energy budget constraints | `NaturalSpawner.SpawnState#canSpawn; PotentialCalculator` | | - | todo |
| spawning.natural.placement-rules | Enforce species spawn placement, light, collision, fluid and difficulty checks | `SpawnPlacements; NaturalSpawner#isValidSpawnPostitionForType` | | - | todo |
| spawning.natural.chunk-generation | Spawn generation-time passive mobs using the generation spawn list and group bounds | `NaturalSpawner#spawnMobsForChunkGeneration` | | - | todo |
| spawning.despawn.distance-persistence | Apply far-distance immediate despawn, random despawn and persistence exceptions | `Mob#checkDespawn` | | - | todo |
| spawning.spawner.player-activation | Activate ordinary spawners only with a nearby eligible player | `BaseSpawner#isNearPlayer` | | - | todo |
| spawning.spawner.attempts-limits | Apply delay, spawn count, nearby entity cap, spawn range and weighted next potentials | `BaseSpawner#serverTick` | | - | todo |
| spawning.spawner.custom-data | Create spawner entities from NBT and apply custom spawn rules and equipment | `BaseSpawner#serverTick; SpawnData` | | - | todo |
| spawning.trial.player-tracking | Detect players, count participating players and scale required spawns and simultaneous cap | `TrialSpawner#tickServer; TrialSpawnerStateData` | | - | todo |
| spawning.trial.line-of-sight | Require valid detected players and line of sight for trial activation | `TrialSpawner#inLineOfSight` | | - | todo |
| spawning.trial.state-machine | Transition inactive, waiting, active, reward-ejection and cooldown states | `TrialSpawnerState; TrialSpawner#tickServer` | | - | todo |
| spawning.trial.ominous | Consume trial omen conditions, change ominous configuration and spawn ominous items | `TrialSpawner#applyOminous; TrialSpawnerState` | | - | todo |
| spawning.trial.mob-tracking | Track spawned mob UUIDs and discard dead, missing or out-of-range participants | `TrialSpawner#shouldMobBeUntracked` | | - | todo |
| spawning.trial.reward-ejection | Choose reward loot tables and eject rewards with state-specific timing | `TrialSpawner#ejectReward; TrialSpawnerState` | | - | todo |
| spawning.vault.key-validation | Require matching vault key and reject players already rewarded by the vault | `VaultBlockEntity.Server#tryInsertKey; VaultServerData` | | - | todo |
| spawning.vault.player-detection | Activate and deactivate vaults from eligible nearby players | `VaultState#updateStateForConnectedPlayers` | | - | todo |
| spawning.vault.loot-ejection | Resolve loot once, animate unlocking and eject each reward in order | `VaultBlockEntity.Server#unlock; VaultState#ejectResultItem` | | - | todo |
| spawning.vault.display-item | Cycle display loot independently of stored ejection rewards | `VaultBlockEntity.Server#getRandomDisplayItemFromLootTable` | | - | todo |
| spawning.structures.overrides | Use fortress, monument, outpost and other structure spawn overrides where applicable | `Structure.StructureSettings; NaturalSpawner#mobsAt` | | - | todo |
| spawning.structures.special-entities | Spawn structure template entities and structure-specific mobs with the correct spawn reason | `StructureTemplate; EntitySpawnReason` | | - | todo |
| spawning.siege.zombies | Schedule village zombie sieges and validate their spawn region | `VillageSiege#tick` | | - | todo |
| spawning.cats.villages-huts | Spawn cats from village POIs and swamp-hut conditions | `CatSpawner#tick` | | - | todo |
| spawning.placement.armadillo | `minecraft:armadillo` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Armadillo#checkArmadilloSpawnRules` | | - | todo |
| spawning.placement.axolotl | `minecraft:axolotl` spawn placement `IN_WATER`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Axolotl#checkAxolotlSpawnRules` | | - | todo |
| spawning.placement.bat | `minecraft:bat` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Bat#checkBatSpawnRules` | | - | todo |
| spawning.placement.blaze | `minecraft:blaze` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkAnyLightMonsterSpawnRules` | | - | todo |
| spawning.placement.bogged | `minecraft:bogged` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.breeze | `minecraft:breeze` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkAnyLightMonsterSpawnRules` | | - | todo |
| spawning.placement.camel | `minecraft:camel` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Camel#checkCamelSpawnRules` | | - | todo |
| spawning.placement.camel-husk | `minecraft:camel_husk` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkSurfaceMonstersSpawnRules` | | - | todo |
| spawning.placement.cat | `minecraft:cat` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Animal#checkAnimalSpawnRules` | | - | todo |
| spawning.placement.cave-spider | `minecraft:cave_spider` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.chicken | `minecraft:chicken` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Animal#checkAnimalSpawnRules` | | - | todo |
| spawning.placement.cod | `minecraft:cod` spawn placement `IN_WATER`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; WaterAnimal#checkSurfaceWaterAnimalSpawnRules` | | - | todo |
| spawning.placement.cow | `minecraft:cow` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Animal#checkAnimalSpawnRules` | | - | todo |
| spawning.placement.creaking | `minecraft:creaking` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.creeper | `minecraft:creeper` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.dolphin | `minecraft:dolphin` spawn placement `IN_WATER`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; AgeableWaterCreature#checkSurfaceAgeableWaterCreatureSpawnRules` | | - | todo |
| spawning.placement.donkey | `minecraft:donkey` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Animal#checkAnimalSpawnRules` | | - | todo |
| spawning.placement.drowned | `minecraft:drowned` spawn placement `IN_WATER`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Drowned#checkDrownedSpawnRules` | | - | todo |
| spawning.placement.elder-guardian | `minecraft:elder_guardian` spawn placement `IN_WATER`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Guardian#checkGuardianSpawnRules` | | - | todo |
| spawning.placement.ender-dragon | `minecraft:ender_dragon` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Mob#checkMobSpawnRules` | | - | todo |
| spawning.placement.enderman | `minecraft:enderman` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.endermite | `minecraft:endermite` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Endermite#checkEndermiteSpawnRules` | | - | todo |
| spawning.placement.evoker | `minecraft:evoker` spawn placement `NO_RESTRICTIONS`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.fox | `minecraft:fox` spawn placement `NO_RESTRICTIONS`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Fox#checkFoxSpawnRules` | | - | todo |
| spawning.placement.frog | `minecraft:frog` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Frog#checkFrogSpawnRules` | | - | todo |
| spawning.placement.ghast | `minecraft:ghast` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Ghast#checkGhastSpawnRules` | | - | todo |
| spawning.placement.giant | `minecraft:giant` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.glow-squid | `minecraft:glow_squid` spawn placement `IN_WATER`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; GlowSquid#checkGlowSquidSpawnRules` | | - | todo |
| spawning.placement.goat | `minecraft:goat` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Goat#checkGoatSpawnRules` | | - | todo |
| spawning.placement.guardian | `minecraft:guardian` spawn placement `IN_WATER`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Guardian#checkGuardianSpawnRules` | | - | todo |
| spawning.placement.happy-ghast | `minecraft:happy_ghast` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Animal#checkAnimalSpawnRules` | | - | todo |
| spawning.placement.hoglin | `minecraft:hoglin` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Hoglin#checkHoglinSpawnRules` | | - | todo |
| spawning.placement.horse | `minecraft:horse` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Animal#checkAnimalSpawnRules` | | - | todo |
| spawning.placement.husk | `minecraft:husk` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkSurfaceMonstersSpawnRules` | | - | todo |
| spawning.placement.illusioner | `minecraft:illusioner` spawn placement `NO_RESTRICTIONS`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.iron-golem | `minecraft:iron_golem` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Mob#checkMobSpawnRules` | | - | todo |
| spawning.placement.llama | `minecraft:llama` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Animal#checkAnimalSpawnRules` | | - | todo |
| spawning.placement.magma-cube | `minecraft:magma_cube` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; MagmaCube#checkMagmaCubeSpawnRules` | | - | todo |
| spawning.placement.mooshroom | `minecraft:mooshroom` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; MushroomCow#checkMushroomSpawnRules` | | - | todo |
| spawning.placement.mule | `minecraft:mule` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Animal#checkAnimalSpawnRules` | | - | todo |
| spawning.placement.nautilus | `minecraft:nautilus` spawn placement `IN_WATER`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; AbstractNautilus#checkNautilusSpawnRules` | | - | todo |
| spawning.placement.ocelot | `minecraft:ocelot` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING` | `SpawnPlacements#register; Ocelot#checkOcelotSpawnRules` | | - | todo |
| spawning.placement.panda | `minecraft:panda` spawn placement `NO_RESTRICTIONS`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Animal#checkAnimalSpawnRules` | | - | todo |
| spawning.placement.parched | `minecraft:parched` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkSurfaceMonstersSpawnRules` | | - | todo |
| spawning.placement.parrot | `minecraft:parrot` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING` | `SpawnPlacements#register; Parrot#checkParrotSpawnRules` | | - | todo |
| spawning.placement.phantom | `minecraft:phantom` spawn placement `NO_RESTRICTIONS`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Mob#checkMobSpawnRules` | | - | todo |
| spawning.placement.pig | `minecraft:pig` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Animal#checkAnimalSpawnRules` | | - | todo |
| spawning.placement.piglin | `minecraft:piglin` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Piglin#checkPiglinSpawnRules` | | - | todo |
| spawning.placement.pillager | `minecraft:pillager` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; PatrollingMonster#checkPatrollingMonsterSpawnRules` | | - | todo |
| spawning.placement.polar-bear | `minecraft:polar_bear` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; PolarBear#checkPolarBearSpawnRules` | | - | todo |
| spawning.placement.pufferfish | `minecraft:pufferfish` spawn placement `IN_WATER`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; WaterAnimal#checkSurfaceWaterAnimalSpawnRules` | | - | todo |
| spawning.placement.rabbit | `minecraft:rabbit` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Rabbit#checkRabbitSpawnRules` | | - | todo |
| spawning.placement.ravager | `minecraft:ravager` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.salmon | `minecraft:salmon` spawn placement `IN_WATER`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; WaterAnimal#checkSurfaceWaterAnimalSpawnRules` | | - | todo |
| spawning.placement.sheep | `minecraft:sheep` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Animal#checkAnimalSpawnRules` | | - | todo |
| spawning.placement.shulker | `minecraft:shulker` spawn placement `NO_RESTRICTIONS`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Mob#checkMobSpawnRules` | | - | todo |
| spawning.placement.silverfish | `minecraft:silverfish` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Silverfish#checkSilverfishSpawnRules` | | - | todo |
| spawning.placement.skeleton | `minecraft:skeleton` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.skeleton-horse | `minecraft:skeleton_horse` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; SkeletonHorse#checkSkeletonHorseSpawnRules` | | - | todo |
| spawning.placement.slime | `minecraft:slime` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Slime#checkSlimeSpawnRules` | | - | todo |
| spawning.placement.snow-golem | `minecraft:snow_golem` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Mob#checkMobSpawnRules` | | - | todo |
| spawning.placement.spider | `minecraft:spider` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.squid | `minecraft:squid` spawn placement `IN_WATER`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; AgeableWaterCreature#checkSurfaceAgeableWaterCreatureSpawnRules` | | - | todo |
| spawning.placement.stray | `minecraft:stray` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Stray#checkStraySpawnRules` | | - | todo |
| spawning.placement.strider | `minecraft:strider` spawn placement `IN_LAVA`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Strider#checkStriderSpawnRules` | | - | todo |
| spawning.placement.sulfur-cube | `minecraft:sulfur_cube` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; SulfurCube#checkSulfurCubeSpawnRules` | | - | todo |
| spawning.placement.trader-llama | `minecraft:trader_llama` spawn placement `NO_RESTRICTIONS`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Animal#checkAnimalSpawnRules` | | - | todo |
| spawning.placement.tropical-fish | `minecraft:tropical_fish` spawn placement `IN_WATER`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; TropicalFish#checkTropicalFishSpawnRules` | | - | todo |
| spawning.placement.turtle | `minecraft:turtle` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Turtle#checkTurtleSpawnRules` | | - | todo |
| spawning.placement.vex | `minecraft:vex` spawn placement `NO_RESTRICTIONS`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.villager | `minecraft:villager` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Mob#checkMobSpawnRules` | | - | todo |
| spawning.placement.vindicator | `minecraft:vindicator` spawn placement `NO_RESTRICTIONS`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.wandering-trader | `minecraft:wandering_trader` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Mob#checkMobSpawnRules` | | - | todo |
| spawning.placement.warden | `minecraft:warden` spawn placement `NO_RESTRICTIONS`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.witch | `minecraft:witch` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.wither | `minecraft:wither` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.wither-skeleton | `minecraft:wither_skeleton` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.wolf | `minecraft:wolf` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Wolf#checkWolfSpawnRules` | | - | todo |
| spawning.placement.zoglin | `minecraft:zoglin` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkAnyLightMonsterSpawnRules` | | - | todo |
| spawning.placement.zombie | `minecraft:zombie` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.zombie-horse | `minecraft:zombie_horse` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.zombie-villager | `minecraft:zombie_villager` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; Monster#checkMonsterSpawnRules` | | - | todo |
| spawning.placement.zombified-piglin | `minecraft:zombified_piglin` spawn placement `ON_GROUND`; heightmap `MOTION_BLOCKING_NO_LEAVES` | `SpawnPlacements#register; ZombifiedPiglin#checkZombifiedPiglinSpawnRules` | | - | todo |
| spawning.trial-spawner.trial-chamber.breeze.normal | `minecraft:trial_chamber/breeze/normal` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/breeze/normal.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.breeze.ominous | `minecraft:trial_chamber/breeze/ominous` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/breeze/ominous.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.melee.husk.normal | `minecraft:trial_chamber/melee/husk/normal` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/melee/husk/normal.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.melee.husk.ominous | `minecraft:trial_chamber/melee/husk/ominous` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/melee/husk/ominous.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.melee.spider.normal | `minecraft:trial_chamber/melee/spider/normal` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/melee/spider/normal.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.melee.spider.ominous | `minecraft:trial_chamber/melee/spider/ominous` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/melee/spider/ominous.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.melee.zombie.normal | `minecraft:trial_chamber/melee/zombie/normal` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/melee/zombie/normal.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.melee.zombie.ominous | `minecraft:trial_chamber/melee/zombie/ominous` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/melee/zombie/ominous.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.ranged.poison-skeleton.normal | `minecraft:trial_chamber/ranged/poison_skeleton/normal` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/ranged/poison_skeleton/normal.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.ranged.poison-skeleton.ominous | `minecraft:trial_chamber/ranged/poison_skeleton/ominous` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/ranged/poison_skeleton/ominous.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.ranged.skeleton.normal | `minecraft:trial_chamber/ranged/skeleton/normal` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/ranged/skeleton/normal.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.ranged.skeleton.ominous | `minecraft:trial_chamber/ranged/skeleton/ominous` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/ranged/skeleton/ominous.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.ranged.stray.normal | `minecraft:trial_chamber/ranged/stray/normal` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/ranged/stray/normal.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.ranged.stray.ominous | `minecraft:trial_chamber/ranged/stray/ominous` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/ranged/stray/ominous.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.slow-ranged.poison-skeleton.normal | `minecraft:trial_chamber/slow_ranged/poison_skeleton/normal` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/slow_ranged/poison_skeleton/normal.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.slow-ranged.poison-skeleton.ominous | `minecraft:trial_chamber/slow_ranged/poison_skeleton/ominous` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/slow_ranged/poison_skeleton/ominous.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.slow-ranged.skeleton.normal | `minecraft:trial_chamber/slow_ranged/skeleton/normal` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/slow_ranged/skeleton/normal.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.slow-ranged.skeleton.ominous | `minecraft:trial_chamber/slow_ranged/skeleton/ominous` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/slow_ranged/skeleton/ominous.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.slow-ranged.stray.normal | `minecraft:trial_chamber/slow_ranged/stray/normal` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/slow_ranged/stray/normal.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.slow-ranged.stray.ominous | `minecraft:trial_chamber/slow_ranged/stray/ominous` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/slow_ranged/stray/ominous.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.small-melee.baby-zombie.normal | `minecraft:trial_chamber/small_melee/baby_zombie/normal` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/small_melee/baby_zombie/normal.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.small-melee.baby-zombie.ominous | `minecraft:trial_chamber/small_melee/baby_zombie/ominous` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/small_melee/baby_zombie/ominous.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.small-melee.cave-spider.normal | `minecraft:trial_chamber/small_melee/cave_spider/normal` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/small_melee/cave_spider/normal.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.small-melee.cave-spider.ominous | `minecraft:trial_chamber/small_melee/cave_spider/ominous` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/small_melee/cave_spider/ominous.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.small-melee.silverfish.normal | `minecraft:trial_chamber/small_melee/silverfish/normal` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/small_melee/silverfish/normal.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.small-melee.silverfish.ominous | `minecraft:trial_chamber/small_melee/silverfish/ominous` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/small_melee/silverfish/ominous.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.small-melee.slime.normal | `minecraft:trial_chamber/small_melee/slime/normal` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/small_melee/slime/normal.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
| spawning.trial-spawner.trial-chamber.small-melee.slime.ominous | `minecraft:trial_chamber/small_melee/slime/ominous` (trial spawner) | `data/minecraft/trial_spawner/trial_chamber/small_melee/slime/ominous.json`; `TrialSpawnerConfig#DIRECT_CODEC` | | - | todo |
