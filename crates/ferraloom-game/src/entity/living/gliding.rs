//! LivingEntity's glider eligibility, durability cadence and glide vibration.
use super::super::{
    Entity, EntityData, EntityRef, LivingEntity,
    player::{equipment::SLOTS, inventory::component},
};
use crate::{
    block::UnportedOperation,
    level::{Level, LevelError},
};
use ferraloom_data::registries::{
    data_component_type::Id as ComponentType, game_event::Id as GameEvent,
    mob_effect::Id as MobEffect,
};
use ferraloom_protocol::component::{DataComponent, EquipmentSlot, ItemStack};
use ferraloom_util::random::RandomSource;

/// Port of LivingEntity#canGlideUsing and ItemStack#nextDamageWillBreak. These
/// effective component reads also hide retained zero/negative-count payloads.
pub fn can_glide_using(stack: &ItemStack, slot: EquipmentSlot) -> bool {
    if component(stack, ComponentType::GLIDER).is_none() {
        return false;
    }
    let Some(DataComponent::Equippable(equippable)) = component(stack, ComponentType::EQUIPPABLE)
    else {
        return false;
    };
    if equippable.slot != slot {
        return false;
    }
    let damageable = component(stack, ComponentType::MAX_DAMAGE).is_some()
        && component(stack, ComponentType::UNBREAKABLE).is_none()
        && component(stack, ComponentType::DAMAGE).is_some();
    if !damageable {
        return true;
    }
    let max_damage = match component(stack, ComponentType::MAX_DAMAGE) {
        Some(DataComponent::MaxDamage(value)) => value.0,
        _ => 0,
    };
    let damage = match component(stack, ComponentType::DAMAGE) {
        Some(DataComponent::Damage(value)) => value.0,
        _ => 0,
    };
    // Mth.clamp, including its ordering if a caller constructs an invalid max.
    let damage = damage.max(0).min(max_damage);
    damage < max_damage.wrapping_sub(1)
}

/// Port of LivingEntity#canGlide. Preserve the native short-circuit reads and
/// EquipmentSlot.VALUES order rather than the protocol's numeric slot order.
pub fn can_glide<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &Level,
) -> Result<bool, LevelError> {
    let living = level.entity::<LivingEntity>(entity)?;
    if living.on_ground
        || living.vehicle.get().is_some()
        || living.has_effect(MobEffect::LEVITATION)
    {
        return Ok(false);
    }
    for slot in SLOTS {
        if behavior
            .item_by_slot(entity, slot, level)?
            .is_some_and(|stack| can_glide_using(stack, slot))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Port of server-side LivingEntity#updateFallFlying. The selected slot consumes
/// the canonical entity RNG before the shared durability effect seam. Warn may
/// skip that effect, but still emits the following native glide game event.
pub fn update_fall_flying<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    {
        let base = level.entity_mut::<EntityData>(entity)?;
        if base.velocity.y > -0.5 && base.fall_distance > 1.0 {
            base.fall_distance = 1.0;
        }
    }
    if !behavior.can_glide(entity, level)? {
        level
            .entity_mut::<EntityData>(entity)?
            .set_shared_flag(7, false);
        return Ok(());
    }
    let ticks = level
        .entity::<LivingEntity>(entity)?
        .fall_fly_ticks
        .wrapping_add(1);
    if ticks % 10 == 0 {
        if (ticks / 10) % 2 == 0 {
            let mut slots = Vec::new();
            for slot in SLOTS {
                if behavior
                    .item_by_slot(entity, slot, level)?
                    .is_some_and(|stack| can_glide_using(stack, slot))
                {
                    slots.push(slot);
                }
            }
            let index = level
                .entity_mut::<EntityData>(entity)?
                .random
                .next_int_bound(slots.len() as i32) as usize;
            let slot = slots[index];
            let item = behavior
                .item_by_slot(entity, slot, level)?
                .expect("canGlideUsing selected a living equipment slot")
                .item();
            level.unported_operation(UnportedOperation::ItemHurtAndBreak {
                item,
                amount: 1,
                slot,
            })?;
        }
        let position = level.entity::<EntityData>(entity)?.position;
        level.game_event(GameEvent::ELYTRA_GLIDE, position, Some(entity), None)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
