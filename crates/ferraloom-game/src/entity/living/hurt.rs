//! Ordered LivingEntity/Player damage pipeline; entity and item callbacks keep
//! their native visibility points and use short canonical state borrows.
use crate::{
    block::UnportedOperation,
    entity::{
        Entity, EntityData, EntityMethod, EntityRef, EntityState, LivingEntity,
        damage::{DamageSource, DamageTypeKey, DamageTypeTag, combat_rules, combat_tracker},
        player::{self, ServerPlayer, equipment::SLOTS, inventory::component},
    },
    item::damage::{can_be_hurt_by, is_damageable_item},
    level::{Level, LevelError, effects::LevelEffect},
    stats::{Stat, custom},
};
use ferraloom_data::{
    EntityType,
    registries::{
        attribute::Id as Attribute, data_component_type::Id as ComponentType,
        mob_effect::Id as MobEffect, sound_event::Id as Sound,
    },
    tags::EntityTypeTags,
};
use ferraloom_protocol::{
    codec::VarInt,
    component::{DataComponent, EquipmentSlot},
    play::{SoundSource, clientbound as cb},
};
use ferraloom_util::{Vec3, java, mth, random::RandomSource};

fn pending_value(entity: EntityRef, method: EntityMethod, level: &Level) -> LevelError {
    match level.entity_state(entity) {
        Ok(state) => LevelError::UnsupportedEntityMethod(state.entity_type(), method),
        Err(error) => error,
    }
}
fn has_enchantments<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &Level,
) -> Result<bool, LevelError> {
    for slot in SLOTS {
        if let Some(stack) = behavior.item_by_slot(entity, slot, level)? {
            if matches!(component(stack,ComponentType::ENCHANTMENTS),Some(DataComponent::Enchantments(value)) if !value.0.is_empty())
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
/// Port of Entity#isInvulnerableToBase, in its native short-circuit order.
pub fn is_invulnerable_to_base<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    source: &DamageSource,
    level: &Level,
) -> Result<bool, LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    if base.removed {
        return Ok(true);
    }
    if (base.permanently_invulnerable || base.invulnerable_time > 0)
        && !source.is_tag(DamageTypeTag::BypassesInvulnerability)
        && !source.is_creative_player(level)?
    {
        return Ok(true);
    }
    if source.is_tag(DamageTypeTag::IsFire) && behavior.fire_immune(entity, level)? {
        return Ok(true);
    }
    Ok(source.is_tag(DamageTypeTag::IsFall)
        && level
            .entity_type_tags()?
            .contains_key(EntityTypeTags::FALL_DAMAGE_IMMUNE, base.entity_type))
}
/// Port of LivingEntity#isInvulnerableTo. Empty enchantment iteration is proven
/// false; unresolved nonempty registry effects are required value queries.
pub fn living_is_invulnerable_to<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    source: &DamageSource,
    level: &Level,
) -> Result<bool, LevelError> {
    if is_invulnerable_to_base(behavior, entity, source, level)? {
        return Ok(true);
    }
    if has_enchantments(behavior, entity, level)? {
        return Err(pending_value(
            entity,
            EntityMethod::EnchantmentDamageImmunity,
            level,
        ));
    }
    Ok(false)
}
/// Player rule gates precede ServerPlayer's dimension/client-load gates.
pub fn player_is_invulnerable_to<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    source: &DamageSource,
    level: &Level,
) -> Result<bool, LevelError> {
    if living_is_invulnerable_to(behavior, entity, source, level)? {
        return Ok(true);
    }
    let rules = level.rules();
    let disabled = if source.is_tag(DamageTypeTag::IsDrowning) {
        !rules.drowning_damage
    } else if source.is_tag(DamageTypeTag::IsFall) {
        !rules.fall_damage
    } else if source.is_tag(DamageTypeTag::IsFire) {
        !rules.fire_damage
    } else {
        source.is_tag(DamageTypeTag::IsFreezing) && !rules.freeze_damage
    };
    if disabled {
        return Ok(true);
    }
    let player = level.entity::<ServerPlayer>(entity)?;
    Ok(player.changing_dimension && !source.is_type(DamageTypeKey::EnderPearl) || !player.loaded)
}
/// Port of LivingEntity#doHurtEquipment. ItemStack durability mutation remains
/// the shared items hook; all preceding eligibility/component reads are real.
pub fn hurt_equipment<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    source: &DamageSource,
    damage: f32,
    slots: &[EquipmentSlot],
    level: &mut Level,
) -> Result<(), LevelError> {
    if damage <= 0.0 {
        return Ok(());
    }
    let amount = java::max_f32(1.0, damage / 4.0) as i32;
    for &slot in slots {
        let item = {
            let stack = behavior
                .item_by_slot(entity, slot, level)?
                .ok_or(LevelError::WrongEntityType(entity))?;
            if matches!(component(stack,ComponentType::EQUIPPABLE),Some(DataComponent::Equippable(value)) if value.damage_on_hurt)
                && is_damageable_item(stack)
                && can_be_hurt_by(stack, source, level)?
            {
                Some(stack.item())
            } else {
                None
            }
        };
        if let Some(item) = item {
            level.unported_operation(UnportedOperation::ItemHurtAndBreak { item, amount, slot })?;
        }
    }
    Ok(())
}
/// Port of LivingEntity#getDamageAfterArmorAbsorb, including virtual hurtArmor
/// before reading live armor/toughness and the direct weapon's enchantments.
pub fn after_armor<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    source: &DamageSource,
    damage: f32,
    level: &mut Level,
) -> Result<f32, LevelError> {
    if source.is_tag(DamageTypeTag::BypassesArmor) {
        return Ok(damage);
    }
    behavior.hurt_armor(entity, source, damage, level)?;
    let living = level.entity::<LivingEntity>(entity)?;
    let armor = mth::floor(
        living
            .attributes
            .value(Attribute::ARMOR)
            .expect("living armor attribute"),
    ) as f32;
    let toughness = living
        .attributes
        .value(Attribute::ARMOR_TOUGHNESS)
        .expect("living toughness attribute") as f32;
    combat_rules::get_damage_after_absorb(entity, damage, source, armor, toughness, level)
}
fn server_player(
    entity: Option<EntityRef>,
    level: &Level,
) -> Result<Option<EntityRef>, LevelError> {
    entity
        .map(|id| {
            level
                .entity_state(id)
                .map(|state| matches!(state, EntityState::Player(_)).then_some(id))
        })
        .transpose()
        .map(Option::flatten)
}
fn stat_amount(value: f32) -> i32 {
    mth::floor(f64::from(value) + 0.5)
}
/// Port of LivingEntity#getDamageAfterMagicAbsorb, preserving float operation
/// order and the victim/attacker stat choice before enchantment evaluation.
pub fn after_magic<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    source: &DamageSource,
    mut damage: f32,
    level: &mut Level,
) -> Result<f32, LevelError> {
    if source.is_tag(DamageTypeTag::BypassesEffects) {
        return Ok(damage);
    }
    let resistance = level
        .entity::<LivingEntity>(entity)?
        .get_effect(MobEffect::RESISTANCE)
        .map(|effect| effect.amplifier());
    if let Some(amplifier) = resistance
        && !source.is_tag(DamageTypeTag::BypassesResistance)
    {
        let absorb = 25_i32.wrapping_sub(amplifier.wrapping_add(1).wrapping_mul(5));
        let before = damage;
        damage = java::max_f32(damage * absorb as f32 / 25.0, 0.0);
        let resisted = before - damage;
        if resisted > 0.0 && resisted < 3.4028235e37_f32 {
            if server_player(Some(entity), level)?.is_some() {
                behavior.award_stat(
                    entity,
                    Stat::Custom(custom::Id::DAMAGE_RESISTED),
                    stat_amount(resisted * 10.0),
                    level,
                )?;
            } else if let Some(attacker) = server_player(source.causing_entity(), level)? {
                level.entity_behavior(attacker)?.award_stat(
                    attacker,
                    Stat::Custom(custom::Id::DAMAGE_DEALT_RESISTED),
                    stat_amount(resisted * 10.0),
                    level,
                )?;
            }
        }
    }
    if damage <= 0.0 {
        return Ok(0.0);
    }
    if source.is_tag(DamageTypeTag::BypassesEnchantments) {
        return Ok(damage);
    }
    if has_enchantments(behavior, entity, level)? {
        return Err(pending_value(
            entity,
            EntityMethod::EnchantmentDamageProtection,
            level,
        ));
    }
    Ok(damage)
}
/// Port of LivingEntity#actuallyHurt. Record combat before health changes, and
/// preserve the Living-only second absorption subtraction after setHealth.
pub fn actually_hurt<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    source: &DamageSource,
    damage: f32,
    level: &mut Level,
) -> Result<(), LevelError> {
    if behavior.is_invulnerable_to(entity, source, level)? {
        return Ok(());
    }
    let damage = after_armor(behavior, entity, source, damage, level)?;
    let before = after_magic(behavior, entity, source, damage, level)?;
    let damage = java::max_f32(before - behavior.absorption_amount(entity, level)?, 0.0);
    let absorption = behavior.absorption_amount(entity, level)? - (before - damage);
    behavior.set_absorption_amount(entity, absorption, level)?;
    let absorbed = before - damage;
    if absorbed > 0.0
        && absorbed < 3.4028235e37_f32
        && let Some(attacker) = server_player(source.causing_entity(), level)?
    {
        level.entity_behavior(attacker)?.award_stat(
            attacker,
            Stat::Custom(custom::Id::DAMAGE_DEALT_ABSORBED),
            stat_amount(absorbed * 10.0),
            level,
        )?;
    }
    if damage != 0.0 {
        combat_tracker::record_damage(behavior, entity, source.clone(), damage, level)?;
        let living = level.entity_mut::<LivingEntity>(entity)?;
        living.set_health(living.health() - damage);
        let absorption = behavior.absorption_amount(entity, level)? - damage;
        behavior.set_absorption_amount(entity, absorption, level)?;
        behavior.game_event(
            entity,
            ferraloom_data::registries::game_event::Id::ENTITY_DAMAGE,
            level,
        )?;
    }
    Ok(())
}
/// Port of Player#actuallyHurt, whose absorption and exhaustion/stat placement
/// differs from its Living parent even when armor math is identical.
pub fn player_actually_hurt<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    source: &DamageSource,
    damage: f32,
    level: &mut Level,
) -> Result<(), LevelError> {
    if behavior.is_invulnerable_to(entity, source, level)? {
        return Ok(());
    }
    let damage = after_armor(behavior, entity, source, damage, level)?;
    let before = after_magic(behavior, entity, source, damage, level)?;
    let damage = java::max_f32(before - behavior.absorption_amount(entity, level)?, 0.0);
    let absorption = behavior.absorption_amount(entity, level)? - (before - damage);
    behavior.set_absorption_amount(entity, absorption, level)?;
    let absorbed = before - damage;
    if absorbed > 0.0 && absorbed < 3.4028235e37_f32 {
        behavior.award_stat(
            entity,
            Stat::Custom(custom::Id::DAMAGE_ABSORBED),
            stat_amount(absorbed * 10.0),
            level,
        )?;
    }
    if damage != 0.0 {
        level
            .entity_mut::<ServerPlayer>(entity)?
            .cause_food_exhaustion(source.food_exhaustion());
        combat_tracker::record_damage(behavior, entity, source.clone(), damage, level)?;
        let living = level.entity_mut::<LivingEntity>(entity)?;
        living.set_health(living.health() - damage);
        if damage < 3.4028235e37_f32 {
            behavior.award_stat(
                entity,
                Stat::Custom(custom::Id::DAMAGE_TAKEN),
                stat_amount(damage * 10.0),
                level,
            )?;
        }
        behavior.game_event(
            entity,
            ferraloom_data::registries::game_event::Id::ENTITY_DAMAGE,
            level,
        )?;
    }
    Ok(())
}
/// Port of LivingEntity#applyItemBlocking's zero/no-item cases. The only current
/// use-item producer is unported; an actually blocking item requires items' use
/// duration/blocking pipeline, never a fabricated zero damage reduction.
pub fn apply_item_blocking(
    entity: EntityRef,
    damage: f32,
    level: &Level,
) -> Result<f32, LevelError> {
    if damage <= 0.0 {
        return Ok(0.0);
    }
    let living = level.entity::<LivingEntity>(entity)?;
    if !living.is_using_item()
        || component(living.get_use_item(), ComponentType::BLOCKS_ATTACKS).is_none()
    {
        return Ok(0.0);
    }
    Err(pending_value(
        entity,
        EntityMethod::ApplyItemBlocking,
        level,
    ))
}
/// Port of LivingEntity#knockback, including the zero-direction random loop.
pub fn knockback(
    entity: EntityRef,
    mut power: f64,
    mut x: f64,
    mut z: f64,
    level: &mut Level,
) -> Result<(), LevelError> {
    power *= 1.0
        - level
            .entity::<LivingEntity>(entity)?
            .attributes
            .value(Attribute::KNOCKBACK_RESISTANCE)
            .expect("living knockback attribute");
    if power <= 0.0 {
        return Ok(());
    }
    let living = level.entity_mut::<LivingEntity>(entity)?;
    living.needs_sync = true;
    while x * x + z * z < 9.999999747378752e-6 {
        x = (living.random.next_double() - living.random.next_double()) * 0.01;
        z = (living.random.next_double() - living.random.next_double()) * 0.01;
    }
    let delta = Vec3::new(x, 0.0, z).normalize().scale(power);
    let velocity = living.velocity;
    living.velocity = Vec3::new(
        velocity.x / 2.0 - delta.x,
        if living.on_ground {
            java::min(0.4, velocity.y / 2.0 + power)
        } else {
            velocity.y
        },
        velocity.z / 2.0 - delta.z,
    );
    Ok(())
}
/// Native direction uses source position unless the direct entity is a
/// projectile. Projectile factories are not ported, so none can enter today.
pub fn default_knockback<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    source: &DamageSource,
    damage: f32,
    blocked: bool,
    level: &mut Level,
) -> Result<(), LevelError> {
    if let Some(direct) = source.direct_entity() {
        // The first projectile state must supply its own horizontal direction.
        match level.entity_state(direct)? {
            EntityState::Cow(_)
            | EntityState::Player(_)
            | EntityState::PrimedTnt(_)
            | EntityState::Item(_)
            | EntityState::FallingBlock(_)
            | EntityState::ExperienceOrb(_) => {}
        }
    }
    let position = source.source_position(level)?;
    let current = level.entity::<EntityData>(entity)?.position;
    let delta = position
        .map(|position| position - current)
        .unwrap_or(Vec3::ZERO);
    behavior.knockback(
        entity,
        0.4000000059604645,
        delta.x,
        delta.z,
        source,
        damage,
        level,
    )?;
    if !blocked {
        behavior.indicate_damage(entity, delta.x, delta.z, level)?;
    }
    Ok(())
}
/// Port of ServerPlayer#indicateDamage; the base Living callback is empty.
pub fn player_indicate_damage(
    entity: EntityRef,
    x: f64,
    z: f64,
    level: &mut Level,
) -> Result<(), LevelError> {
    let player = level.entity_mut::<ServerPlayer>(entity)?;
    player.hurt_dir = (mth::atan2(z, x) * 57.2957763671875 - f64::from(player.yaw)) as f32;
    let packet = cb::HurtAnimation {
        id: VarInt(player.network_id),
        yaw: player.hurt_dir,
    };
    level.emit(LevelEffect::PlayerPacket {
        recipients: vec![entity],
        packet: Box::new(cb::Packet::HurtAnimation(packet)),
    });
    Ok(())
}
/// Port of LivingEntity#getVoicePitch. Player overrides this without RNG draws.
pub fn voice_pitch<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
) -> Result<f32, LevelError> {
    let baby = behavior.is_baby(entity, level)?;
    let random = &mut level.entity_mut::<EntityData>(entity)?.random;
    Ok((random.next_float() - random.next_float()) * 0.2 + if baby { 1.5 } else { 1.0 })
}
pub fn play_hurt_sound<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    source: &DamageSource,
    level: &mut Level,
) -> Result<(), LevelError> {
    if let Some(sound) = behavior.hurt_sound(entity, source, level)? {
        let volume = behavior.sound_volume();
        let pitch = behavior.voice_pitch(entity, level)?;
        behavior.play_sound(entity, sound, volume, pitch, level)?;
    }
    Ok(())
}
fn secondary_hurt_sound(
    entity: EntityRef,
    source: &DamageSource,
    level: &mut Level,
) -> Result<(), LevelError> {
    if source.is_type(DamageTypeKey::Thorns) {
        let state = level.entity_state(entity)?;
        let category = if matches!(state, EntityState::Player(_)) {
            SoundSource::Players
        } else {
            SoundSource::Hostile
        };
        level.play_sound(
            None,
            state.as_entity().position,
            Sound::ENCHANT_THORNS_HIT,
            category,
            1.0,
            1.0,
        )?;
    }
    Ok(())
}

fn remember_responsibility(
    entity: EntityRef,
    source: &DamageSource,
    level: &mut Level,
) -> Result<(), LevelError> {
    if let Some(attacker) = source.causing_entity() {
        if level.entity_state(attacker)?.as_living().is_some()
            && !source.is_tag(DamageTypeTag::NoAnger)
        {
            let ignore_wind = source.is_type(DamageTypeKey::WindCharge)
                && level.entity_type_tags()?.contains_key(
                    EntityTypeTags::NO_ANGER_FROM_WIND_CHARGE,
                    level.entity::<EntityData>(entity)?.entity_type,
                );
            if !ignore_wind {
                super::damage_state::set_last_hurt_by_mob(entity, Some(attacker), level)?;
            }
        }
        // No Wolf factory is ported. The exhaustive state match keeps that
        // owner-reference branch required when a canonical wolf is introduced.
        match level.entity_state(attacker)? {
            EntityState::Player(_) => {
                super::damage_state::set_last_hurt_by_player(entity, Some(attacker), 100, level)?
            }
            EntityState::Cow(_)
            | EntityState::PrimedTnt(_)
            | EntityState::Item(_)
            | EntityState::FallingBlock(_)
            | EntityState::ExperienceOrb(_) => {}
        }
    }
    Ok(())
}
fn check_totem_death_protection<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    source: &DamageSource,
    level: &Level,
) -> Result<bool, LevelError> {
    if source.is_tag(DamageTypeTag::BypassesInvulnerability) {
        return Ok(false);
    }
    for slot in [EquipmentSlot::Mainhand, EquipmentSlot::Offhand] {
        if let Some(stack) = behavior.item_by_slot(entity, slot, level)?
            && component(stack, ComponentType::DEATH_PROTECTION).is_some()
        {
            return Err(pending_value(
                entity,
                EntityMethod::CheckTotemDeathProtection,
                level,
            ));
        }
    }
    Ok(false)
}
fn on_mob_hurt_effects(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    // Observable iteration uses our deterministic identity-map order, as the
    // collection policy specifies; holder identity hash order is not parity.
    let effects: Vec<_> = level
        .entity::<LivingEntity>(entity)?
        .active_effects
        .values()
        .map(|effect| effect.effect())
        .collect();
    for effect in effects {
        // MobEffect's parent is empty. Registry source has only this override.
        if effect == MobEffect::INFESTED {
            let random = &mut level.entity_mut::<EntityData>(entity)?.random;
            if random.next_float() <= 0.1 {
                let _count = random.next_int_bound(2) + 1;
                // First EntityType#create precedes launch-angle/level RNG. A
                // missing factory is required, never a false spawn result.
                return Err(LevelError::UnsupportedEntity(EntityType::SILVERFISH));
            }
        }
    }
    Ok(())
}
fn hurt_body<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    source: &DamageSource,
    mut damage: f32,
    level: &mut Level,
) -> Result<bool, LevelError> {
    if behavior.is_invulnerable_to(entity, source, level)? {
        return Ok(false);
    }
    let living = level.entity::<LivingEntity>(entity)?;
    if living.is_dead_or_dying() {
        return Ok(false);
    }
    if source.is_tag(DamageTypeTag::IsFire) && living.has_effect(MobEffect::FIRE_RESISTANCE) {
        return Ok(false);
    }
    if living
        .entity_data
        .get(crate::entity::synched_data::SLEEPING_POS)
        .is_some()
    {
        return Err(pending_value(entity, EntityMethod::StopSleeping, level));
    }
    level.entity_mut::<LivingEntity>(entity)?.no_action_time = 0;
    if damage < 0.0 {
        damage = 0.0;
    }
    let original = damage;
    let damage_blocked = behavior.apply_item_blocking(entity, source, damage, level)?;
    damage -= damage_blocked;
    let blocked = damage_blocked > 0.0;
    if source.is_tag(DamageTypeTag::IsFreezing)
        && level.entity_type_tags()?.contains_key(
            EntityTypeTags::FREEZE_HURTS_EXTRA_TYPES,
            level.entity::<EntityData>(entity)?.entity_type,
        )
    {
        damage *= 5.0;
    }
    if source.is_tag(DamageTypeTag::DamagesHelmet)
        && behavior
            .item_by_slot(entity, EquipmentSlot::Head, level)?
            .is_some_and(|stack| !stack.is_empty())
    {
        behavior.hurt_helmet(entity, source, damage, level)?;
        damage *= 0.75;
    }
    if !damage.is_finite() {
        damage = f32::MAX;
    }
    let living = level.entity::<LivingEntity>(entity)?;
    let full =
        if living.damage_cooldown_time > 10 && !source.is_tag(DamageTypeTag::BypassesCooldown) {
            if damage <= living.last_hurt {
                return Ok(false);
            }
            let increment = damage - living.last_hurt;
            behavior.actually_hurt(entity, source, increment, level)?;
            level.entity_mut::<LivingEntity>(entity)?.last_hurt = damage;
            false
        } else {
            let living = level.entity_mut::<LivingEntity>(entity)?;
            living.last_hurt = damage;
            living.damage_cooldown_time = 20;
            behavior.actually_hurt(entity, source, damage, level)?;
            let living = level.entity_mut::<LivingEntity>(entity)?;
            living.hurt_duration = 10;
            living.hurt_time = 10;
            true
        };
    remember_responsibility(entity, source, level)?;
    if full {
        if blocked {
            // The required applyItemBlocking seam above prevents entry today.
            // Items must carry the original useItem receiver through this
            // callback before enabling its nonzero blocking result.
            return Err(pending_value(
                entity,
                EntityMethod::ApplyItemBlocking,
                level,
            ));
        }
        level.broadcast_damage_event(entity, source)?;
        if !source.is_tag(DamageTypeTag::NoImpact) && (!blocked || damage > 0.0) {
            level.entity_mut::<EntityData>(entity)?.sync_velocity = true;
        }
        let fully_blocked = blocked && damage <= 0.0;
        if !source.is_tag(DamageTypeTag::NoKnockback) && !fully_blocked {
            behavior.deal_default_knockback(entity, source, damage, blocked, level)?;
        }
    }
    if level.entity::<LivingEntity>(entity)?.is_dead_or_dying() {
        if !check_totem_death_protection(behavior, entity, source, level)? {
            if full {
                if let Some(sound) = behavior.death_sound(entity, level)? {
                    let volume = behavior.sound_volume();
                    let pitch = behavior.voice_pitch(entity, level)?;
                    behavior.play_sound(entity, sound, volume, pitch, level)?;
                }
                secondary_hurt_sound(entity, source, level)?;
            }
            behavior.die(entity, source, level)?;
        }
    } else if full {
        play_hurt_sound(behavior, entity, source, level)?;
        secondary_hurt_sound(entity, source, level)?;
    }
    let success = !blocked || damage > 0.0;
    if success {
        let stamp = level.game_time();
        let living = level.entity_mut::<LivingEntity>(entity)?;
        living.last_damage_source = Some(source.clone());
        living.last_damage_stamp = stamp;
        on_mob_hurt_effects(entity, level)?;
    }
    if server_player(Some(entity), level)?.is_some() {
        crate::advancements::criteria::entity_hurt_player(
            level, entity, source, original, damage, blocked,
        )?;
        if damage_blocked > 0.0 && damage_blocked < 3.4028235e37_f32 {
            behavior.award_stat(
                entity,
                Stat::Custom(custom::Id::DAMAGE_BLOCKED_BY_SHIELD),
                stat_amount(damage_blocked * 10.0),
                level,
            )?;
        }
    }
    if let Some(attacker) = server_player(source.causing_entity(), level)? {
        crate::advancements::criteria::player_hurt_entity(
            level, attacker, entity, source, original, damage, blocked,
        )?;
    }
    Ok(success)
}
/// DamageSource and the current receiver are native stack-held objects. Explicit
/// owner roots keep them alive through synchronous callbacks and management.
fn with_damage_refs<T>(
    entity: EntityRef,
    source: &DamageSource,
    level: &mut Level,
    body: impl FnOnce(&mut Level) -> Result<T, LevelError>,
) -> Result<T, LevelError> {
    let mut roots = vec![entity];
    source.trace_entity_refs(&mut roots);
    for (retained, &root) in roots.iter().enumerate() {
        if let Err(error) = level.retain_entity(root) {
            for &prior in roots[..retained].iter().rev() {
                level.release_entity(prior)?;
            }
            return Err(error);
        }
    }
    let result = body(level);
    let mut release_error = None;
    for &root in roots.iter().rev() {
        if let Err(error) = level.release_entity(root) {
            release_error.get_or_insert(error);
        }
    }
    match result {
        Err(error) => Err(error),
        Ok(value) => {
            if let Some(error) = release_error {
                Err(error)
            } else {
                Ok(value)
            }
        }
    }
}
/// Port of LivingEntity#hurtServer's entry, retaining the derived dispatcher.
pub fn hurt_server<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    source: DamageSource,
    damage: f32,
    level: &mut Level,
) -> Result<bool, LevelError> {
    with_damage_refs(entity, &source, level, |level| {
        hurt_body(behavior, entity, &source, damage, level)
    })
}
fn teams_allow_damage(
    entity: EntityRef,
    other: EntityRef,
    level: &Level,
) -> Result<bool, LevelError> {
    let own = level.entity_scoreboard_name(entity)?;
    let other = level.entity_scoreboard_name(other)?;
    let scoreboard = level.shared()?.scoreboard.scoreboard();
    let own = scoreboard.get_players_team(&own);
    let other = scoreboard.get_players_team(&other);
    Ok(own.is_none_or(|team| !team.is_allied_to(other) || team.allow_friendly_fire))
}
/// ServerPlayer#hurtServer then its Player parent, with repeated virtual
/// invulnerability calls kept at their native positions.
pub fn player_hurt_server<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    source: DamageSource,
    mut damage: f32,
    level: &mut Level,
) -> Result<bool, LevelError> {
    with_damage_refs(entity, &source, level, |level| {
        if behavior.is_invulnerable_to(entity, &source, level)? {
            return Ok(false);
        }
        if let Some(attacker) = server_player(source.causing_entity(), level)?
            && (!level.rules().pvp || !teams_allow_damage(entity, attacker, level)?)
        {
            return Ok(false);
        }
        // AbstractArrow's owner variant is required when the first projectile
        // canonical state/factory lands; all current states are non-projectiles.
        if let Some(cause) = source.causing_entity() {
            match level.entity_state(cause)? {
                EntityState::Cow(_)
                | EntityState::Player(_)
                | EntityState::PrimedTnt(_)
                | EntityState::Item(_)
                | EntityState::FallingBlock(_)
                | EntityState::ExperienceOrb(_) => {}
            }
        }
        if behavior.is_invulnerable_to(entity, &source, level)? {
            return Ok(false);
        }
        let player = level.entity::<ServerPlayer>(entity)?;
        if player.abilities.invulnerable && !source.is_tag(DamageTypeTag::BypassesInvulnerability) {
            return Ok(false);
        }
        let player = level.entity_mut::<ServerPlayer>(entity)?;
        player.no_action_time = 0;
        if player.is_dead_or_dying() {
            return Ok(false);
        }
        player::shoulders::remove_entities_on_shoulder(entity, level)?;
        if source.scales_with_difficulty(level)? {
            damage = match level.difficulty() {
                ferraloom_protocol::play::Difficulty::Peaceful => 0.0,
                ferraloom_protocol::play::Difficulty::Easy => {
                    java::min_f32(damage / 2.0 + 1.0, damage)
                }
                ferraloom_protocol::play::Difficulty::Normal => damage,
                ferraloom_protocol::play::Difficulty::Hard => damage * 3.0 / 2.0,
            };
        }
        if damage == 0.0 {
            return Ok(false);
        }
        hurt_body(behavior, entity, &source, damage, level)
    })
}

#[cfg(test)]
mod tests;
