# Patrols

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| patrols.scheduler.delay | Apply patrol spawn cooldown, world-age and daytime eligibility | `PatrolSpawner#tick` | | - | todo |
| patrols.placement.players | Select an eligible player and test village distance, biome and spawn positions | `PatrolSpawner#tick` | | - | todo |
| patrols.group.leader | Spawn a captain and patrol followers with patrol target and difficulty-scaled count | `PatrolSpawner#spawnPatrolMember` | | - | todo |
| patrols.movement.target | Follow patrol leader destinations and choose new patrol targets | `PatrollingMonster` | | - | todo |
