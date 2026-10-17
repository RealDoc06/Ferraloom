# Player mechanics

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| player.join.initial-state | Send login, dimension, permissions, abilities, recipes, effects, inventory and scoreboard state | `PlayerList#placeNewPlayer` | | - | todo |
| player.movement.invalid-values | Reject NaN positions or nonfinite rotation and clamp packet coordinates | `ServerGamePacketListenerImpl#containsInvalidValues; ServerGamePacketListenerImpl#clampHorizontal; ServerGamePacketListenerImpl#clampVertical` | | - | todo |
| player.movement.too-quickly | Check packet movement against allowed velocity with elytra and movement-rule branches | `ServerGamePacketListenerImpl#handlePlayerPositionChange` | | - | todo |
| player.movement.collision-correction | Detect moved-wrongly and newly introduced collisions and correct authoritative position | `ServerGamePacketListenerImpl#handlePlayerPositionChange` | | - | todo |
| player.movement.teleport-ack | Match pending teleport ids and positions and resend corrections after the timeout | `ServerGamePacketListenerImpl#handleAcceptTeleportPacket; ServerGamePacketListenerImpl#updateAwaitingTeleport` | | - | todo |
| player.movement.floating | Track floating players and vehicles and disconnect after allowed flight ticks | `ServerGamePacketListenerImpl#tick` | | - | todo |
| player.movement.vehicle | Validate controlling passenger vehicle movement and correct invalid moves | `ServerGamePacketListenerImpl#handleMoveVehicle` | | - | todo |
| player.movement.client-tick-end | Handle client tick-end movement tracking and known movement state | `ServerGamePacketListenerImpl#handleClientTickEnd; ServerGamePacketListenerImpl#handlePlayerKnownMovement` | | - | todo |
| player.movement.pose | Update standing, crouching, swimming, gliding, crawling and sleeping poses against fit checks | `Player#updatePlayerPose` | | - | todo |
| player.movement.sneak-edge | Clamp sneaking horizontal movement to avoid stepping off unsupported edges | `Player#maybeBackOffFromEdge` | | - | todo |
| player.movement.sprint-exhaustion | Apply sprinting, swimming, jumping and movement exhaustion and statistics | `ServerPlayer#checkMovementStatistics; LivingEntity#jumpFromGround` | | - | todo |
| player.interaction.reach | Validate block-use hit position, world border, build height and interaction range | `ServerGamePacketListenerImpl#handleUseItemOn` | | - | todo |
| player.interaction.break-progress | Apply start, abort and stop dig states, tool speed and authoritative block acknowledgements | `ServerPlayerGameMode#handleBlockBreakAction; ServerPlayerGameMode#tick` | | - | todo |
| player.interaction.break-restrictions | Respect game mode, tool suitability, protected blocks and player action restrictions | `ServerPlayerGameMode#destroyBlock` | | - | todo |
| player.interaction.item-sequence | Process item-use prediction sequence acknowledgements and server corrections | `ServerGamePacketListenerImpl#handleUseItem; ServerGamePacketListenerImpl#ackBlockChangesUpTo` | | - | todo |
| player.interaction.entity | Apply reach, spectator, hand and target checks for entity interaction | `ServerGamePacketListenerImpl#handleInteract` | | - | todo |
| player.game-mode.survival | Use survival health, hunger, item consumption and tool durability | `GameType#updatePlayerAbilities; ServerPlayerGameMode` | | - | todo |
| player.game-mode.creative | Apply instant breaking, unlimited materials, creative slot validation and flight | `GameType#updatePlayerAbilities; ServerGamePacketListenerImpl#handleSetCreativeModeSlot` | | - | todo |
| player.game-mode.adventure | Restrict block placement and breaking using item adventure predicates | `ServerPlayerGameMode; AdventureModePredicate` | | - | todo |
| player.game-mode.spectator | Apply no-clip, restricted interactions, spectator camera and teleport behavior | `ServerPlayer#setCamera; ServerGamePacketListenerImpl#handleSpectatorAction` | | - | todo |
| player.abilities.synchronization | Synchronize invulnerability, flying, allowed flight, instant building and movement speeds | `Abilities; ServerPlayer#onUpdateAbilities` | | - | todo |
| player.hunger.exhaustion | Spend exhaustion above four on saturation first, then food outside peaceful | `FoodData#tick` | | - | todo |
| player.hunger.fast-regeneration | Regenerate every 10 ticks at full food with saturation-dependent healing and exhaustion | `FoodData#tick` | | - | todo |
| player.hunger.slow-regeneration | Regenerate one health every 80 ticks at food level at least 18 | `FoodData#tick` | | - | todo |
| player.hunger.starvation | Apply starvation every 80 ticks with difficulty-dependent minimum health | `FoodData#tick` | | - | todo |
| player.hunger.food-caps | Clamp food to 20 and saturation to current food level | `FoodData#add` | | - | todo |
| player.xp.level-curve | Apply level-dependent experience point requirements and progress carry | `Player#getXpNeededForNextLevel; Player#giveExperiencePoints` | | - | todo |
| player.xp.orb-pickup | Merge experience orbs, apply pickup delay and repair mending equipment before XP gain | `ExperienceOrb` | | - | todo |
| player.xp.death | Drop experience and reset progress subject to keep-inventory and death rules | `ServerPlayer; Player` | | - | todo |
| player.sleep.entry | Validate bed range, danger, dimension conditions and sleep eligibility | `ServerPlayer#startSleepInBed` | | - | todo |
| player.sleep.threshold | Require enough sleeping and deeply sleeping eligible players under the percentage rule | `SleepStatus; ServerLevel#tick` | | - | todo |
| player.sleep.wake | Wake players and advance clock to the wake marker with optional weather reset | `ServerLevel#tick` | | - | todo |
| player.respawn.bed-anchor | Resolve valid bed or charged-anchor respawn and consume anchor charge when required | `ServerPlayer#findRespawnPositionAndUseSpawnBlock` | | - | todo |
| player.respawn.fallback | Use world spawn and spawn-radius search when personal respawn is absent or invalid | `PlayerList#respawn; PlayerSpawnFinder` | | - | todo |
| player.respawn.retained-data | Copy retained player state and apply keep-inventory, effects and death-reset behavior | `ServerPlayer#restoreFrom; PlayerList#respawn` | | - | todo |
| player.respawn.client-load | Gate player actions on client-loaded state and apply client-load timeout after respawn | `ServerGamePacketListenerImpl#tickClientLoadTimeout; ServerGamePacketListenerImpl#handleAcceptPlayerLoad` | | - | todo |
| player.spectator.camera-reset | Follow spectator target and restore camera when target becomes invalid | `ServerPlayer#tick; ServerPlayer#setCamera` | | - | todo |
| player.death-screen | Send death notification, combat killer data and immediate-respawn rule state | `ServerPlayer#die` | | - | todo |
| player.locator-bar | Track eligible waypoints with dimension, range, invisibility and locator rule checks | `ServerWaypointManager` | | - | todo |
| player.border.damage | Apply world-border warning state, safe distance and damage amount | `WorldBorder; Player` | | - | todo |
| player.sleep.straw-bed | Sleep at the lower straw-bed height and record the distinct slept-in-straw-bed statistic | `StrawBedBlock#getSleepHeight; StrawBedBlock#getSleptInBedStatType` | | - | todo |
| player.sleep.environment-rules | Evaluate sleeping and respawn-setting rules independently for normal and straw beds | `AbstractBedBlock; BedRule` | | - | todo |
