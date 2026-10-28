//! Shared server LivingEntity tick. Subclass tick overrides call this once.
use crate::{
    entity::{Entity, EntityData, EntityMethod, EntityRef, LivingEntity, synched_data as data},
    level::{Level, LevelError, tick_rate::TickRateManager},
};
use ferraloom_protocol::{codec::VarInt, entity_data::Pose};

fn required(entity: EntityRef, method: EntityMethod, level: &Level) -> Result<(), LevelError> {
    Err(LevelError::UnsupportedEntityMethod(
        level.entity_state(entity)?.entity_type(),
        method,
    ))
}

/// Port of LivingEntity#tick and its inherited baseTick slots. Keep
/// no state borrow across callbacks. A counted receiver root survives removal.
pub fn tick<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
    tick_rate: &TickRateManager,
) -> Result<(), LevelError> {
    level.retain_entity(entity)?;
    let result = tick_body(behavior, entity, level, tick_rate);
    level.release_entity(entity);
    result
}
fn tick_body<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
    tick_rate: &TickRateManager,
) -> Result<(), LevelError> {
    level.entity_mut::<LivingEntity>(entity)?.swing_state.tick();
    crate::entity::base_tick(behavior, entity, level)?;
    // Breathing and remaining baseTick world effects retain their existing gap.
    super::damage_state::base_tick_damage_state(entity, level)?;
    if !level
        .entity::<LivingEntity>(entity)?
        .active_effects
        .is_empty()
    {
        required(entity, EntityMethod::TickEffects, level)?;
    }
    {
        let state = level.entity_mut::<LivingEntity>(entity)?;
        state.old_y_head_rot = state.y_head_rot;
        state.old_y_body_rot = state.y_body_rot;
    }
    if level.entity::<LivingEntity>(entity)?.is_using_item() {
        required(entity, EntityMethod::UpdatingUsingItem, level)?;
    }
    {
        let state = level.entity_mut::<LivingEntity>(entity)?;
        state.old_swim_amount = state.swim_amount;
        let swimming = state.pose() == Pose::Swimming
            || !state.shared_flag(7) && state.pose() == Pose::FallFlying;
        state.swim_amount = if swimming {
            (state.swim_amount + 0.09).min(1.0)
        } else {
            (state.swim_amount - 0.09).max(0.0)
        };
        let arrows = state.entity_data.get(data::ARROW_COUNT).0;
        if arrows > 0 {
            if state.remove_arrow_time <= 0 {
                state.remove_arrow_time = 20_i32.wrapping_mul(30_i32.wrapping_sub(arrows));
            }
            state.remove_arrow_time = state.remove_arrow_time.wrapping_sub(1);
            if state.remove_arrow_time <= 0 {
                state.entity_data.set(data::ARROW_COUNT, VarInt(arrows - 1));
            }
        }
        let stingers = state.entity_data.get(data::STINGER_COUNT).0;
        if stingers > 0 {
            if state.remove_stinger_time <= 0 {
                state.remove_stinger_time = 20_i32.wrapping_mul(30_i32.wrapping_sub(stingers));
            }
            state.remove_stinger_time = state.remove_stinger_time.wrapping_sub(1);
            if state.remove_stinger_time <= 0 {
                state
                    .entity_data
                    .set(data::STINGER_COUNT, VarInt(stingers - 1));
            }
        }
    }
    // Existing gap: detectEquipmentUpdates (modifier reconciliation and packets).
    if level.entity::<EntityData>(entity)?.tick_count % 20 == 0 {
        crate::entity::damage::combat_tracker::recheck_status(behavior, entity, level)?;
    }
    if level
        .entity::<EntityData>(entity)?
        .entity_data
        .get(data::SLEEPING_POS)
        .is_some()
    {
        required(entity, EntityMethod::TickSleeping, level)?;
    }
    if !level.entity::<EntityData>(entity)?.removed {
        behavior.ai_step(entity, level, tick_rate)?;
    }
    behavior.tick_head_turn(entity, level)?;
    {
        let state = level.entity_mut::<LivingEntity>(entity)?;
        // These are client interpolation histories, normalized by native tick.
        state.old_yaw = normalize_old(state.yaw, state.old_yaw);
        state.old_pitch = normalize_old(state.pitch, state.old_pitch);
        state.old_y_body_rot = normalize_old(state.y_body_rot, state.old_y_body_rot);
        state.old_y_head_rot = normalize_old(state.y_head_rot, state.old_y_head_rot);
        state.fall_fly_ticks = if state.shared_flag(7) {
            state.fall_fly_ticks.wrapping_add(1)
        } else {
            0
        };
        if state.entity_data.get(data::SLEEPING_POS).is_some() {
            state.pitch = 0.0;
        }
    }
    super::refresh_dirty_attributes(entity, level)?;
    level
        .entity_mut::<LivingEntity>(entity)?
        .tick_current_impulse_grace_time();
    Ok(())
}

fn normalize_old(current: f32, mut old: f32) -> f32 {
    while current - old < -180.0 {
        old -= 360.0;
    }
    while current - old >= 180.0 {
        old += 360.0;
    }
    old
}

/// Port of LivingEntity#tickHeadTurn's parent. Mob replaces it with its
/// BodyRotationControl; the AI owner supplies that canonical subsystem.
pub fn tick_head_turn_parent(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let state = level.entity_mut::<LivingEntity>(entity)?;
    let delta = state.position - state.old_position;
    let distance = (delta.x * delta.x + delta.z * delta.z) as f32;
    let mut target = state.y_body_rot;
    if distance > 0.0025000002 {
        let direction = (ferraloom_util::mth::atan2(delta.z, delta.x) as f32) * 57.295776 - 90.0;
        let difference = (ferraloom_util::mth::wrap_degrees_f32(state.yaw) - direction).abs();
        target = if 95.0 < difference && difference < 265.0 {
            direction - 180.0
        } else {
            direction
        };
    }
    if state.swing_animation(1.0) > 0.0 {
        target = state.yaw;
    }
    state.y_body_rot += ferraloom_util::mth::wrap_degrees_f32(target - state.y_body_rot) * 0.3;
    let difference = ferraloom_util::mth::wrap_degrees_f32(state.yaw - state.y_body_rot);
    if difference.abs() > 50.0 {
        state.y_body_rot += difference - difference.signum() * 50.0;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
