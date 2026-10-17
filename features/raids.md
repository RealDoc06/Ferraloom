# Raids

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| raids.omen.entry | Convert bad omen to raid omen and start or extend a raid near a village | `BadOmenMobEffect; RaidOmenMobEffect; Raids#createOrExtendRaid` | | - | todo |
| raids.wave.counts | Choose wave count, bonus wave and raider composition from difficulty and omen level | `Raid#getNumGroups; Raid#getDefaultNumSpawns; Raid#getPotentialBonusSpawns` | | - | todo |
| raids.wave.spawn-position | Find valid wave positions around the raid center and relocate to nearby village sections | `Raid#findRandomSpawnPos; Raid#moveRaidCenterToNearbyVillageSection` | | - | todo |
| raids.wave.preparation | Apply between-wave countdown and horn timing before spawning a wave | `Raid#tick` | | - | todo |
| raids.raider.join-leader | Join existing raids, assign banner leaders and update group bookkeeping | `Raid#joinRaid; Raid#setLeader; Raider` | | - | todo |
| raids.raider.health-bossbar | Update bossbar participants and remaining raider health after damage, death or removal | `Raid#updatePlayers; Raid#updateBossbar; Raid#removeFromRaid` | | - | todo |
| raids.outcome.victory-loss | Resolve victory, village loss, timeout and stopped states and award hero effects | `Raid#tick` | | - | todo |
| raids.persistence.restart | Persist active raids, groups, omen and center and recover across restart | `Raids; Raid` | | - | todo |
