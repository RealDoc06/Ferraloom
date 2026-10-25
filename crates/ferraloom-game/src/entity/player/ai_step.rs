//! Player#aiStep and its inherited LivingEntity server phases.
use super::{ServerPlayer, contact, food, item_tick, shoulders};
use crate::{
    block::UnportedOperation,
    entity::{Entity, EntityData, EntityMethod, EntityRef, LivingEntity, living, synched_data},
    level::{Level, LevelError, tick_rate::TickRateManager},
};
use ferraloom_data::{
    EntityType,
    registries::{attribute::Id as Attribute, mob_effect::Id as MobEffect},
    tags::FluidTags,
};
use ferraloom_util::Vec3;

/// Port of LivingEntity#aiStep's player velocity thresholds and #applyInput.
fn apply_input(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let living = level.entity_mut::<LivingEntity>(entity)?;
    // Server players can always simulate movement. Head interpolation counters
    // are client-written and remain zero on the server.
    let mut movement = living.velocity;
    if movement.horizontal_distance_sqr() < 9.0e-6 {
        movement.x = 0.0;
        movement.z = 0.0;
    }
    if movement.y.abs() < 0.003 {
        movement.y = 0.0;
    }
    living.set_delta_movement(movement);
    living.xxa *= 0.98;
    living.zza *= 0.98;
    let immobile =
        living.health() <= 0.0 || living.entity_data.get(synched_data::SLEEPING_POS).is_some();
    if immobile {
        living.jumping = false;
        living.xxa = 0.0;
        living.zza = 0.0;
    }
    // Player inherits LivingEntity#serverAiStep's empty implementation.
    Ok(())
}
/// Port of LivingEntity#aiStep's jump phase, including its water/lava ordering.
fn jump<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    if level.entity::<LivingEntity>(entity)?.jumping
        && behavior.is_affected_by_fluids(entity, level)?
    {
        let base = level.entity::<EntityData>(entity)?;
        let lava = base.is_in_lava(level)?;
        let height = base.fluid_interaction.height(
            if lava {
                FluidTags::LAVA
            } else {
                FluidTags::WATER
            },
            level,
        )?;
        let water_and_height = base.was_touching_water && height > 0.0;
        let threshold = base.fluid_jump_threshold();
        if !water_and_height || base.on_ground && (height <= threshold || height.is_nan()) {
            let base = level.entity::<EntityData>(entity)?;
            let lava = base.is_in_lava(level)?;
            let ground = base.on_ground;
            let shallow_lava = if lava && ground {
                base.fluid_interaction.height(FluidTags::LAVA, level)?
                    <= base.fluid_jump_threshold()
            } else {
                false
            };
            if !lava || ground && shallow_lava {
                if (ground || water_and_height && height <= threshold)
                    && level.entity::<LivingEntity>(entity)?.no_jump_delay == 0
                {
                    behavior.jump_from_ground(entity, level)?;
                    level.entity_mut::<LivingEntity>(entity)?.no_jump_delay = 10;
                }
            } else {
                behavior.jump_in_liquid(entity, level)?;
            }
        } else {
            behavior.jump_in_liquid(entity, level)?;
        }
    } else {
        level.entity_mut::<LivingEntity>(entity)?.no_jump_delay = 0;
    }
    Ok(())
}

/// Port of LivingEntity#aiStep for its Player subclass. Player's controlling
/// passenger is always null, and its water-sensitivity virtual is false.
fn living_ai_step<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
    tick_rate: &TickRateManager,
) -> Result<(), LevelError> {
    // noJumpDelay precedes the equipment tick, but velocity/input follows it.
    {
        let living = level.entity_mut::<LivingEntity>(entity)?;
        if living.no_jump_delay > 0 {
            living.no_jump_delay -= 1;
        }
    }
    item_tick::tick_equipment(entity, level)?;
    apply_input(entity, level)?;
    jump(behavior, entity, level)?;
    if level.entity::<EntityData>(entity)?.shared_flag(7) {
        living::gliding::update_fall_flying(behavior, entity, level)?;
    }
    let (input, reset_fall_distance) = {
        let living = level.entity::<LivingEntity>(entity)?;
        let input = Vec3::new(
            f64::from(living.xxa),
            f64::from(living.yya),
            f64::from(living.zza),
        );
        (
            input,
            living.has_effect(MobEffect::SLOW_FALLING) || living.has_effect(MobEffect::LEVITATION),
        )
    };
    if reset_fall_distance {
        behavior.reset_fall_distance(entity, level)?;
    }
    // Server Player#isEffectiveAi is true; retain the native canSimulate query.
    if behavior.can_simulate_movement(entity, level)? {
        behavior.travel(entity, input, level)?;
    }
    behavior.apply_effects_from_blocks(entity, level)?;
    living::freezing::tick_freezing(behavior, entity, level)?;
    {
        let living = level.entity_mut::<LivingEntity>(entity)?;
        if living.auto_spin_attack_ticks > 0 {
            living.auto_spin_attack_ticks -= 1;
            level.unported_operation(UnportedOperation::EntityMethod(
                EntityType::PLAYER,
                EntityMethod::CheckAutoSpinAttack,
            ))?;
        }
    }
    behavior.push_entities(entity, level, tick_rate)
}

/// Port of Player#aiStep with ServerPlayer's regeneration and shoulder overrides.
/// All callbacks use handles with canonical borrows ended before world access.
pub(super) fn ai_step<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
    tick_rate: &TickRateManager,
) -> Result<(), LevelError> {
    let player = level.entity_mut::<ServerPlayer>(entity)?;
    if player.jump_trigger_time > 0 {
        player.jump_trigger_time -= 1;
    }
    food::tick_regeneration(entity, level)?;
    item_tick::tick_inventory(entity, level)?;
    let player = level.entity::<ServerPlayer>(entity)?;
    if player.abilities.flying && player.vehicle.get().is_none() {
        behavior.reset_fall_distance(entity, level)?;
    }
    living_ai_step(behavior, entity, level, tick_rate)?;
    {
        let living = level.entity_mut::<LivingEntity>(entity)?;
        living.y_head_rot = living.yaw;
        living.speed = living
            .attributes
            .value(Attribute::MOVEMENT_SPEED)
            .expect("player supplier defines MOVEMENT_SPEED") as f32;
    }
    contact::touch_entities(entity, level)?;
    shoulders::handle_shoulder_entities(entity, level)
}
#[cfg(test)]
mod tests;
