//! One shared LivingEntity#aiStep for players and mobs.
use crate::{
    block::UnportedOperation,
    entity::{Entity, EntityData, EntityMethod, EntityRef, LivingEntity, living},
    level::{Level, LevelError, tick_rate::TickRateManager},
};
use ferraloom_data::{EntityType, registries::mob_effect::Id as MobEffect, tags::FluidTags};
use ferraloom_util::Vec3;

/// Port of LivingEntity#aiStep's distinct Player and non-player thresholds.
fn threshold_velocity(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let state = level.entity_mut::<LivingEntity>(entity)?;
    let mut velocity = state.velocity;
    if state.entity_type == EntityType::PLAYER {
        if velocity.horizontal_distance_sqr() < 9.0e-6 {
            velocity.x = 0.0;
            velocity.z = 0.0;
        }
    } else {
        if velocity.x.abs() < 0.003 {
            velocity.x = 0.0;
        }
        if velocity.z.abs() < 0.003 {
            velocity.z = 0.0;
        }
    }
    if velocity.y.abs() < 0.003 {
        velocity.y = 0.0;
    }
    state.set_delta_movement(velocity);
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

/// Port of the shared server LivingEntity#aiStep; subclasses call this parent
/// body from their own aiStep override, never copy its phases.
pub fn ai_step<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
    tick_rate: &TickRateManager,
) -> Result<(), LevelError> {
    level.retain_entity(entity)?;
    let result = ai_step_body(behavior, entity, level, tick_rate);
    level.release_entity(entity);
    result
}
fn ai_step_body<E: Entity + ?Sized>(
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
    if !behavior.can_simulate_movement(entity, level)? {
        let state = level.entity_mut::<EntityData>(entity)?;
        state.set_delta_movement(state.velocity * 0.98);
    }
    // Head interpolation is client-written and remains zero on the server.
    crate::entity::player::item_tick::tick_equipment(entity, level)?;
    threshold_velocity(entity, level)?;
    behavior.apply_input(entity, level)?;
    if behavior.is_immobile(entity, level)? {
        let state = level.entity_mut::<LivingEntity>(entity)?;
        state.jumping = false;
        state.xxa = 0.0;
        state.zza = 0.0;
    } else if behavior.is_effective_ai(entity, level)? {
        behavior.server_ai_step(entity, level)?;
    }
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
    if let Some(controller) = behavior.controlling_passenger(entity, level)?
        && matches!(
            level.entity_state(controller)?,
            crate::entity::EntityState::Player(_)
        )
        && behavior.is_alive(entity, level)?
    {
        return Err(LevelError::UnsupportedEntityMethod(
            level.entity_state(entity)?.entity_type(),
            EntityMethod::TravelRidden,
        ));
    } else if behavior.can_simulate_movement(entity, level)?
        && behavior.is_effective_ai(entity, level)?
    {
        behavior.travel(entity, input, level)?;
    }
    behavior.apply_effects_from_blocks(entity, level)?;
    living::freezing::tick_freezing(behavior, entity, level)?;
    {
        let living = level.entity_mut::<LivingEntity>(entity)?;
        if living.auto_spin_attack_ticks > 0 {
            living.auto_spin_attack_ticks -= 1;
            level.unported_operation(UnportedOperation::EntityMethod(
                level.entity_state(entity)?.entity_type(),
                EntityMethod::CheckAutoSpinAttack,
            ))?;
        }
    }
    behavior.push_entities(entity, level, tick_rate)?;
    if behavior.is_sensitive_to_water(entity, level)? {
        // Rain's environment query belongs with the first water-sensitive leaf.
        return Err(LevelError::UnsupportedEntityMethod(
            level.entity_state(entity)?.entity_type(),
            EntityMethod::SensitiveToWater,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
