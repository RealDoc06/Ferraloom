//! LivingEntity travel arithmetic; world-dependent travel phases build on these.
use super::LivingEntity;
use crate::{
    entity::{Entity, EntityData, EntityRef},
    level::{Level, LevelError},
};
use ferraloom_data::{
    Block, EntityType,
    registries::{attribute::Id as Attribute, mob_effect::Id as MobEffect},
};
use ferraloom_util::{Vec3, java, mth};

pub mod fall_flying;
pub mod fluid;

/// Port of LivingEntity#travel. The current block's required fluid-state read
/// precedes all mode tests, even when the entity ultimately travels in air.
pub fn travel<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    input: Vec3,
    level: &mut Level,
) -> Result<(), LevelError> {
    let pos = level.entity::<EntityData>(entity)?.block_position;
    let state = level.get_block_state(pos)?.data().fluid;
    if fluid::should_travel_in_fluid(behavior, entity, state, level)? {
        fluid::travel_in_fluid(behavior, entity, input, level)
    } else if level.entity::<EntityData>(entity)?.shared_flag(7) {
        fall_flying::travel_fall_flying(behavior, entity, input, level)
    } else {
        travel_in_air(behavior, entity, input, level)
    }
}

/// Port of LivingEntity#getEffectiveGravity, reading the falling flag before the
/// active-effect lookup and the Entity parent's actual gravity query.
pub fn effective_gravity<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &Level,
) -> Result<f64, LevelError> {
    let living = level.entity::<LivingEntity>(entity)?;
    let slow_falling = living.velocity.y <= 0.0 && living.has_effect(MobEffect::SLOW_FALLING);
    let gravity = crate::entity::base_effective_gravity(behavior, entity, level)?;
    Ok(if slow_falling {
        java::min(gravity, 0.01)
    } else {
        gravity
    })
}

/// Port of LivingEntity#handleRelativeFrictionAndCalculateMovement. The move
/// uses Entity's collision engine and records its path for the later aiStep slot.
pub fn handle_relative_friction_and_calculate_movement<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    input: Vec3,
    friction: f32,
    level: &mut Level,
) -> Result<Vec3, LevelError> {
    let speed = if level.entity::<EntityData>(entity)?.on_ground {
        friction_influenced_speed(behavior.get_speed(entity, level)?, 0.0, true, friction)
    } else {
        behavior.get_flying_speed(entity, level)?
    };
    level
        .entity_mut::<EntityData>(entity)?
        .move_relative(speed, input);
    let movement = level.entity::<EntityData>(entity)?.velocity;
    let movement = handle_on_climbable(behavior, entity, movement, level)?;
    level.entity_mut::<EntityData>(entity)?.velocity = movement;
    behavior.move_by(
        entity,
        level,
        crate::entity::movement::MoverType::SelfMovement,
        movement,
    )?;
    let (mut movement, climb, powder_snow) = {
        let living = level.entity::<LivingEntity>(entity)?;
        (
            living.velocity,
            living.horizontal_collision || living.jumping,
            living.was_in_powder_snow,
        )
    };
    if climb
        && (super::climbable::on_climbable(behavior, entity, level)?
            || powder_snow && crate::block::plants::can_entity_walk_on_powder_snow(entity, level)?)
    {
        movement.y = 0.2;
    }
    Ok(movement)
}

/// Port of server-side LivingEntity#travelInAir. Levitation is queried after
/// movement; gravity and all drag arithmetic preserve native evaluation order.
pub fn travel_in_air<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    input: Vec3,
    level: &mut Level,
) -> Result<(), LevelError> {
    let below = behavior.block_pos_below_that_affects_my_movement(entity, level)?;
    let friction = if level.entity::<EntityData>(entity)?.on_ground {
        compute_modified_friction(
            level.get_block_state(below)?.block().data().friction,
            super::attribute(entity, Attribute::FRICTION_MODIFIER, level)? as f32,
        )
    } else {
        1.0
    };
    let movement =
        handle_relative_friction_and_calculate_movement(behavior, entity, input, friction, level)?;
    let levitation = level
        .entity::<LivingEntity>(entity)?
        .get_effect(MobEffect::LEVITATION)
        .map(|effect| effect.amplifier());
    let y = if let Some(amplifier) = levitation {
        movement.y + (0.05 * f64::from(amplifier + 1) - movement.y) * 0.2
    } else {
        movement.y - behavior.effective_gravity(entity, level)?
    };
    let velocity = if level.entity::<LivingEntity>(entity)?.discard_friction {
        Vec3::new(movement.x, y, movement.z)
    } else {
        let modifier = super::attribute(entity, Attribute::AIR_DRAG_MODIFIER, level)? as f32;
        let air_drag = compute_modified_friction(0.91, modifier);
        let horizontal = friction * air_drag;
        let vertical = if behavior.omnidirectional_air_mover() {
            air_drag
        } else {
            compute_modified_friction(0.98, modifier)
        };
        Vec3::new(
            movement.x * f64::from(horizontal),
            y * f64::from(vertical),
            movement.z * f64::from(horizontal),
        )
    };
    level.entity_mut::<EntityData>(entity)?.velocity = velocity;
    Ok(())
}

/// Port of LivingEntity#handleOnClimbable, including player-only ladder
/// suppression and the scaffolding exception. Reads use the canonical cache.
pub fn handle_on_climbable<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    delta: Vec3,
    level: &mut Level,
) -> Result<Vec3, LevelError> {
    if !super::climbable::on_climbable(behavior, entity, level)? {
        return Ok(delta);
    }
    level.entity_mut::<EntityData>(entity)?.fall_distance = 0.0;
    let x = mth::clamp(delta.x, -0.15000000596046448, 0.15000000596046448);
    let z = mth::clamp(delta.z, -0.15000000596046448, 0.15000000596046448);
    let mut y = java::max(delta.y, -0.15000000596046448);
    if y < 0.0 {
        let base = level.entity::<EntityData>(entity)?;
        if base.in_block_state(level)?.block() != Block::SCAFFOLDING
            && base.shared_flag(1)
            && base.entity_type == EntityType::PLAYER
        {
            y = 0.0;
        }
    }
    Ok(Vec3::new(x, y, z))
}

/// Port of LivingEntity#computeModifiedFriction, retaining every float step.
pub fn compute_modified_friction(friction: f32, modifier: f32) -> f32 {
    mth::clamp_f32(1.0 - (1.0 - friction) * modifier, 0.0, 1.0)
}

/// Port of LivingEntity#getFrictionInfluencedSpeed with captured virtual speeds.
pub fn friction_influenced_speed(
    speed: f32,
    flying_speed: f32,
    on_ground: bool,
    block_friction: f32,
) -> f32 {
    if on_ground {
        // Native compares the float with a double literal: 0.6F is above it.
        if f64::from(block_friction) > 0.6 {
            speed * (0.216_000_02 / (block_friction * block_friction * block_friction))
        } else {
            speed
        }
    } else {
        flying_speed
    }
}

impl LivingEntity {
    /// Port of LivingEntity#getFluidFallingAdjustedMovement. The falling flag is
    /// captured before travel; it must not be inferred from this later velocity.
    pub fn fluid_falling_adjusted_movement(
        &self,
        base_gravity: f64,
        is_falling: bool,
        movement: Vec3,
    ) -> Vec3 {
        if base_gravity != 0.0 && !self.shared_flag(3) {
            let y = if is_falling
                && (movement.y - 0.005).abs() >= 0.003
                && (movement.y - base_gravity / 16.0).abs() < 0.003
            {
                -0.003
            } else {
                movement.y - base_gravity / 16.0
            };
            Vec3::new(movement.x, y, movement.z)
        } else {
            movement
        }
    }
}

#[cfg(test)]
mod tests;
