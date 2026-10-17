# Wandering traders

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| wandering-traders.scheduler.persisted-chance | Persist and update wandering-trader spawn delay, chance and existing trader identity | `WanderingTraderSpawner#tick; WanderingTraderSpawner#getTraderData` | | - | todo |
| wandering-traders.placement.meeting-point | Choose an eligible player or meeting-point POI and validate the spawn space | `WanderingTraderSpawner#spawn` | | - | todo |
| wandering-traders.llamas.leashes | Spawn trader llamas and attach them to the wandering trader | `WanderingTraderSpawner#tryToSpawnLlamaFor` | | - | todo |
| wandering-traders.offers.resources | Select wandering-trader trade sets and produce limited-use offers | `WanderingTrader; TradeSet; VillagerTrade` | | - | todo |
| wandering-traders.lifetime.invisibility | Apply trader despawn delay, trading pause and night invisibility behavior | `WanderingTrader` | | - | todo |
