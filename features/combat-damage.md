# Combat and damage

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| combat-damage.pipeline.invulnerability | Apply damage-source immunity, invulnerability frames and excess-damage handling | `LivingEntity#hurtServer` | | - | todo |
| combat-damage.pipeline.armor | Reduce armor-sensitive damage using armor and toughness | `CombatRules#getDamageAfterAbsorb` | | - | todo |
| combat-damage.pipeline.enchantment-protection | Apply enchantment damage protection and capped magic absorption | `CombatRules#getDamageAfterMagicAbsorb; EnchantmentHelper` | | - | todo |
| combat-damage.pipeline.resistance-absorption | Apply resistance, absorption hearts and health damage in vanilla order | `LivingEntity#actuallyHurt` | | - | todo |
| combat-damage.pipeline.shield | Resolve shield block checks, item durability and attacker disable effects | `LivingEntity#hurtServer; BlocksAttacks` | | - | todo |
| combat-damage.pipeline.knockback | Apply knockback resistance and velocity and hurt markers | `LivingEntity#knockback` | | - | todo |
| combat-damage.pipeline.death-credit | Track damage attribution, kill credit, death messages and last attacker | `CombatTracker; CombatEntry` | | - | todo |
| combat-damage.death.drops-xp | Emit equipment, loot and experience under killer and gamerule conditions | `LivingEntity#die; LivingEntity#dropAllDeathLoot` | | - | todo |
| combat-damage.death.totem | Consume a qualifying totem and apply death-protection effects | `LivingEntity#checkTotemDeathProtection; DeathProtection` | | - | todo |
| combat-damage.attack.cooldown | Scale melee damage by attack strength and reset attack recovery | `Player#attack; Player#getAttackStrengthScale` | | - | todo |
| combat-damage.attack.critical-sweep | Apply critical and sweeping attack conditions and damage to nearby targets | `Player#attack` | | - | todo |
| combat-damage.attack.sprint-fire | Apply sprint knockback, fire aspect and post-attack effects in order | `Player#attack; EnchantmentHelper` | | - | todo |
| combat-damage.attack.reach-validation | Reject attacks against out-of-range or invalid targets | `ServerGamePacketListenerImpl#handleAttack` | | - | todo |
| combat-damage.attack.pvp-team | Respect PVP gamerule, team friendly-fire and player invulnerability | `Player#canHarmPlayer; ServerPlayer#hurtServer` | | - | todo |
| combat-damage.attributes.operations | Evaluate additive, multiplied-base and multiplied-total modifiers in order | `AttributeInstance#calculateValue` | | - | todo |
| combat-damage.attributes.bounds | Clamp and sanitize final attribute values to declared ranges | `RangedAttribute#sanitizeValue` | | - | todo |
| combat-damage.effects.merge | Merge effect amplifiers, durations and hidden effect chains | `MobEffectInstance#update` | | - | todo |
| combat-damage.effects.tick-expire | Tick periodic effects, expire durations and update effect attributes | `MobEffectInstance#tick; LivingEntity#tickEffects` | | - | todo |
| combat-damage.effects.instant | Scale instant effects by potency and inverted heal/harm behavior | `HealOrHarmMobEffect` | | - | todo |
| combat-damage.effects.immunity | Apply species-specific mob-effect immunity and effect removal conditions | `LivingEntity#canBeAffected` | | - | todo |
| combat-damage.potions.brewing | Resolve registered potion and container mixes with fuel and ingredient rules | `BrewingRecipe; RecipeManager` | | - | todo |
| combat-damage.environment.fire-freeze-drown | Apply fire, freezing, drowning, suffocation and fall damage rules at their tick conditions | `LivingEntity#baseTick; Entity#baseTick` | | - | todo |
| combat-damage.attribute.air-drag-modifier | `minecraft:air_drag_modifier` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.armor | `minecraft:armor` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.armor-toughness | `minecraft:armor_toughness` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.attack-damage | `minecraft:attack_damage` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.attack-knockback | `minecraft:attack_knockback` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.attack-speed | `minecraft:attack_speed` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.below-name-distance | `minecraft:below_name_distance` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.block-break-speed | `minecraft:block_break_speed` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.block-interaction-range | `minecraft:block_interaction_range` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.bounciness | `minecraft:bounciness` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.burning-time | `minecraft:burning_time` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.camera-distance | `minecraft:camera_distance` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.entity-interaction-range | `minecraft:entity_interaction_range` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.explosion-knockback-resistance | `minecraft:explosion_knockback_resistance` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.fall-damage-multiplier | `minecraft:fall_damage_multiplier` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.flying-speed | `minecraft:flying_speed` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.follow-range | `minecraft:follow_range` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.friction-modifier | `minecraft:friction_modifier` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.gravity | `minecraft:gravity` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.jump-strength | `minecraft:jump_strength` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.knockback-resistance | `minecraft:knockback_resistance` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.luck | `minecraft:luck` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.max-absorption | `minecraft:max_absorption` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.max-health | `minecraft:max_health` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.mining-efficiency | `minecraft:mining_efficiency` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.movement-efficiency | `minecraft:movement_efficiency` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.movement-speed | `minecraft:movement_speed` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.name-tag-distance | `minecraft:name_tag_distance` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.oxygen-bonus | `minecraft:oxygen_bonus` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.safe-fall-distance | `minecraft:safe_fall_distance` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.scale | `minecraft:scale` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.sneaking-speed | `minecraft:sneaking_speed` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.spawn-reinforcements | `minecraft:spawn_reinforcements` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.step-height | `minecraft:step_height` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.submerged-mining-speed | `minecraft:submerged_mining_speed` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.sweeping-damage-ratio | `minecraft:sweeping_damage_ratio` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.tempt-range | `minecraft:tempt_range` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.water-movement-efficiency | `minecraft:water_movement_efficiency` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.waypoint-receive-range | `minecraft:waypoint_receive_range` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.attribute.waypoint-transmit-range | `minecraft:waypoint_transmit_range` (attribute) | `net.minecraft.world.entity.ai.attributes.Attributes#bootstrap; net.minecraft.world.entity.ai.attributes.RangedAttribute` | | - | todo |
| combat-damage.consume-effect-type.apply-effects | `minecraft:apply_effects` (consume effect type) | `net.minecraft.world.item.consume_effects.ConsumeEffect#register` | | - | todo |
| combat-damage.consume-effect-type.clear-all-effects | `minecraft:clear_all_effects` (consume effect type) | `net.minecraft.world.item.consume_effects.ConsumeEffect#register` | | - | todo |
| combat-damage.consume-effect-type.play-sound | `minecraft:play_sound` (consume effect type) | `net.minecraft.world.item.consume_effects.ConsumeEffect#register` | | - | todo |
| combat-damage.consume-effect-type.remove-effects | `minecraft:remove_effects` (consume effect type) | `net.minecraft.world.item.consume_effects.ConsumeEffect#register` | | - | todo |
| combat-damage.consume-effect-type.teleport-randomly | `minecraft:teleport_randomly` (consume effect type) | `net.minecraft.world.item.consume_effects.ConsumeEffect#register` | | - | todo |
| combat-damage.damage-type.arrow | `minecraft:arrow` (damage type) | `data/minecraft/damage_type/arrow.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.bad-respawn-point | `minecraft:bad_respawn_point` (damage type) | `data/minecraft/damage_type/bad_respawn_point.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.cactus | `minecraft:cactus` (damage type) | `data/minecraft/damage_type/cactus.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.campfire | `minecraft:campfire` (damage type) | `data/minecraft/damage_type/campfire.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.cramming | `minecraft:cramming` (damage type) | `data/minecraft/damage_type/cramming.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.dragon-breath | `minecraft:dragon_breath` (damage type) | `data/minecraft/damage_type/dragon_breath.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.drown | `minecraft:drown` (damage type) | `data/minecraft/damage_type/drown.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.dry-out | `minecraft:dry_out` (damage type) | `data/minecraft/damage_type/dry_out.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.ender-pearl | `minecraft:ender_pearl` (damage type) | `data/minecraft/damage_type/ender_pearl.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.explosion | `minecraft:explosion` (damage type) | `data/minecraft/damage_type/explosion.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.fall | `minecraft:fall` (damage type) | `data/minecraft/damage_type/fall.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.falling-anvil | `minecraft:falling_anvil` (damage type) | `data/minecraft/damage_type/falling_anvil.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.falling-block | `minecraft:falling_block` (damage type) | `data/minecraft/damage_type/falling_block.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.falling-stalactite | `minecraft:falling_stalactite` (damage type) | `data/minecraft/damage_type/falling_stalactite.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.fireball | `minecraft:fireball` (damage type) | `data/minecraft/damage_type/fireball.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.fireworks | `minecraft:fireworks` (damage type) | `data/minecraft/damage_type/fireworks.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.fly-into-wall | `minecraft:fly_into_wall` (damage type) | `data/minecraft/damage_type/fly_into_wall.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.freeze | `minecraft:freeze` (damage type) | `data/minecraft/damage_type/freeze.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.generic | `minecraft:generic` (damage type) | `data/minecraft/damage_type/generic.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.generic-kill | `minecraft:generic_kill` (damage type) | `data/minecraft/damage_type/generic_kill.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.hot-floor | `minecraft:hot_floor` (damage type) | `data/minecraft/damage_type/hot_floor.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.in-fire | `minecraft:in_fire` (damage type) | `data/minecraft/damage_type/in_fire.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.in-wall | `minecraft:in_wall` (damage type) | `data/minecraft/damage_type/in_wall.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.indirect-magic | `minecraft:indirect_magic` (damage type) | `data/minecraft/damage_type/indirect_magic.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.lava | `minecraft:lava` (damage type) | `data/minecraft/damage_type/lava.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.lightning-bolt | `minecraft:lightning_bolt` (damage type) | `data/minecraft/damage_type/lightning_bolt.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.mace-smash | `minecraft:mace_smash` (damage type) | `data/minecraft/damage_type/mace_smash.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.magic | `minecraft:magic` (damage type) | `data/minecraft/damage_type/magic.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.mob-attack | `minecraft:mob_attack` (damage type) | `data/minecraft/damage_type/mob_attack.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.mob-attack-no-aggro | `minecraft:mob_attack_no_aggro` (damage type) | `data/minecraft/damage_type/mob_attack_no_aggro.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.mob-projectile | `minecraft:mob_projectile` (damage type) | `data/minecraft/damage_type/mob_projectile.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.on-fire | `minecraft:on_fire` (damage type) | `data/minecraft/damage_type/on_fire.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.out-of-world | `minecraft:out_of_world` (damage type) | `data/minecraft/damage_type/out_of_world.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.outside-border | `minecraft:outside_border` (damage type) | `data/minecraft/damage_type/outside_border.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.player-attack | `minecraft:player_attack` (damage type) | `data/minecraft/damage_type/player_attack.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.player-explosion | `minecraft:player_explosion` (damage type) | `data/minecraft/damage_type/player_explosion.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.sonic-boom | `minecraft:sonic_boom` (damage type) | `data/minecraft/damage_type/sonic_boom.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.spear | `minecraft:spear` (damage type) | `data/minecraft/damage_type/spear.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.spit | `minecraft:spit` (damage type) | `data/minecraft/damage_type/spit.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.stalagmite | `minecraft:stalagmite` (damage type) | `data/minecraft/damage_type/stalagmite.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.starve | `minecraft:starve` (damage type) | `data/minecraft/damage_type/starve.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.sting | `minecraft:sting` (damage type) | `data/minecraft/damage_type/sting.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.sulfur-cube-hot | `minecraft:sulfur_cube_hot` (damage type) | `data/minecraft/damage_type/sulfur_cube_hot.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.sweet-berry-bush | `minecraft:sweet_berry_bush` (damage type) | `data/minecraft/damage_type/sweet_berry_bush.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.thorns | `minecraft:thorns` (damage type) | `data/minecraft/damage_type/thorns.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.thrown | `minecraft:thrown` (damage type) | `data/minecraft/damage_type/thrown.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.trident | `minecraft:trident` (damage type) | `data/minecraft/damage_type/trident.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.unattributed-fireball | `minecraft:unattributed_fireball` (damage type) | `data/minecraft/damage_type/unattributed_fireball.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.wind-charge | `minecraft:wind_charge` (damage type) | `data/minecraft/damage_type/wind_charge.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.wither | `minecraft:wither` (damage type) | `data/minecraft/damage_type/wither.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.damage-type.wither-skull | `minecraft:wither_skull` (damage type) | `data/minecraft/damage_type/wither_skull.json`; `DamageType#CODEC` | | - | todo |
| combat-damage.mob-effect.absorption | `minecraft:absorption` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.AbsorptionMobEffect` | | - | todo |
| combat-damage.mob-effect.bad-omen | `minecraft:bad_omen` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.BadOmenMobEffect` | | - | todo |
| combat-damage.mob-effect.blindness | `minecraft:blindness` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.breath-of-the-nautilus | `minecraft:breath_of_the_nautilus` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.conduit-power | `minecraft:conduit_power` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.darkness | `minecraft:darkness` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.dolphins-grace | `minecraft:dolphins_grace` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.fire-resistance | `minecraft:fire_resistance` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.glowing | `minecraft:glowing` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.haste | `minecraft:haste` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.health-boost | `minecraft:health_boost` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.hero-of-the-village | `minecraft:hero_of_the_village` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.hunger | `minecraft:hunger` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.HungerMobEffect` | | - | todo |
| combat-damage.mob-effect.infested | `minecraft:infested` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.InfestedMobEffect` | | - | todo |
| combat-damage.mob-effect.instant-damage | `minecraft:instant_damage` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.HealOrHarmMobEffect` | | - | todo |
| combat-damage.mob-effect.instant-health | `minecraft:instant_health` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.HealOrHarmMobEffect` | | - | todo |
| combat-damage.mob-effect.invisibility | `minecraft:invisibility` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.jump-boost | `minecraft:jump_boost` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.levitation | `minecraft:levitation` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.luck | `minecraft:luck` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.mining-fatigue | `minecraft:mining_fatigue` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.nausea | `minecraft:nausea` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.night-vision | `minecraft:night_vision` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.oozing | `minecraft:oozing` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.OozingMobEffect` | | - | todo |
| combat-damage.mob-effect.poison | `minecraft:poison` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.PoisonMobEffect` | | - | todo |
| combat-damage.mob-effect.raid-omen | `minecraft:raid_omen` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.RaidOmenMobEffect` | | - | todo |
| combat-damage.mob-effect.regeneration | `minecraft:regeneration` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.RegenerationMobEffect` | | - | todo |
| combat-damage.mob-effect.resistance | `minecraft:resistance` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.saturation | `minecraft:saturation` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.SaturationMobEffect` | | - | todo |
| combat-damage.mob-effect.slow-falling | `minecraft:slow_falling` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.slowness | `minecraft:slowness` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.speed | `minecraft:speed` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.strength | `minecraft:strength` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.trial-omen | `minecraft:trial_omen` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.unluck | `minecraft:unluck` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.water-breathing | `minecraft:water_breathing` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.weakness | `minecraft:weakness` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.MobEffect` | | - | todo |
| combat-damage.mob-effect.weaving | `minecraft:weaving` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.WeavingMobEffect` | | - | todo |
| combat-damage.mob-effect.wind-charged | `minecraft:wind_charged` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.WindChargedMobEffect` | | - | todo |
| combat-damage.mob-effect.wither | `minecraft:wither` (mob effect) | `net.minecraft.world.effect.MobEffects#bootstrap; net.minecraft.world.effect.WitherMobEffect` | | - | todo |
| combat-damage.potion.awkward | `minecraft:awkward` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.fire-resistance | `minecraft:fire_resistance` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.harming | `minecraft:harming` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.healing | `minecraft:healing` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.infested | `minecraft:infested` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.invisibility | `minecraft:invisibility` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.leaping | `minecraft:leaping` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.long-fire-resistance | `minecraft:long_fire_resistance` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.long-invisibility | `minecraft:long_invisibility` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.long-leaping | `minecraft:long_leaping` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.long-night-vision | `minecraft:long_night_vision` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.long-poison | `minecraft:long_poison` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.long-regeneration | `minecraft:long_regeneration` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.long-slow-falling | `minecraft:long_slow_falling` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.long-slowness | `minecraft:long_slowness` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.long-strength | `minecraft:long_strength` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.long-swiftness | `minecraft:long_swiftness` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.long-turtle-master | `minecraft:long_turtle_master` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.long-water-breathing | `minecraft:long_water_breathing` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.long-weakness | `minecraft:long_weakness` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.luck | `minecraft:luck` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.mundane | `minecraft:mundane` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.night-vision | `minecraft:night_vision` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.oozing | `minecraft:oozing` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.poison | `minecraft:poison` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.regeneration | `minecraft:regeneration` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.slow-falling | `minecraft:slow_falling` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.slowness | `minecraft:slowness` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.strength | `minecraft:strength` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.strong-harming | `minecraft:strong_harming` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.strong-healing | `minecraft:strong_healing` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.strong-leaping | `minecraft:strong_leaping` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.strong-poison | `minecraft:strong_poison` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.strong-regeneration | `minecraft:strong_regeneration` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.strong-slowness | `minecraft:strong_slowness` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.strong-strength | `minecraft:strong_strength` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.strong-swiftness | `minecraft:strong_swiftness` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.strong-turtle-master | `minecraft:strong_turtle_master` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.swiftness | `minecraft:swiftness` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.thick | `minecraft:thick` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.turtle-master | `minecraft:turtle_master` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.water | `minecraft:water` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.water-breathing | `minecraft:water_breathing` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.weakness | `minecraft:weakness` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.weaving | `minecraft:weaving` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
| combat-damage.potion.wind-charged | `minecraft:wind_charged` (potion) | `net.minecraft.world.item.alchemy.Potions#bootstrap; net.minecraft.world.item.alchemy.Potion` | | - | todo |
