//! LivingEntity's fall-flying physics and its ordered sound/damage hook.
use super::{Entity, EntityData, EntityRef, Level, LevelError};
use crate::entity::movement::MoverType;
use ferraloom_util::{Vec3, mth};

/// Port of LivingEntity#updateFallFlyingMovement. Pitch multiplies in float;
/// lift uses double cosine while upward conversion uses the native sine table.
pub fn update_fall_flying_movement<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    mut movement: Vec3,
    level: &Level,
) -> Result<Vec3, LevelError> {
    let (look, lean) = {
        let base = level.entity::<EntityData>(entity)?;
        (base.look_angle(), base.pitch * 0.017453292)
    };
    let look_horizontal = (look.x * look.x + look.z * look.z).sqrt();
    let horizontal = movement.horizontal_distance();
    let gravity = behavior.effective_gravity(entity, level)?;
    let cosine = f64::from(lean).cos();
    let lift = cosine * cosine;
    movement = movement + Vec3::new(0.0, gravity * (-1.0 + lift * 0.75), 0.0);
    if movement.y < 0.0 && look_horizontal > 0.0 {
        let convert = movement.y * -0.1 * lift;
        movement = movement
            + Vec3::new(
                look.x * convert / look_horizontal,
                convert,
                look.z * convert / look_horizontal,
            );
    }
    if lean < 0.0 && look_horizontal > 0.0 {
        let convert = horizontal * f64::from(-mth::sin(f64::from(lean))) * 0.04;
        movement = movement
            + Vec3::new(
                -look.x * convert / look_horizontal,
                convert * 3.2,
                -look.z * convert / look_horizontal,
            );
    }
    if look_horizontal > 0.0 {
        movement = movement
            + Vec3::new(
                (look.x / look_horizontal * horizontal - movement.x) * 0.1,
                0.0,
                (look.z / look_horizontal * horizontal - movement.z) * 0.1,
            );
    }
    Ok(movement.multiply(0.9900000095367432, 0.9800000190734863, 0.9900000095367432))
}

/// Port of LivingEntity#stopFallFlying. Both writes matter even when the flag
/// was already clear: the shared metadata accessor must become dirty.
pub fn stop_fall_flying(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let base = level.entity_mut::<EntityData>(entity)?;
    base.set_shared_flag(7, true);
    base.set_shared_flag(7, false);
    Ok(())
}

/// Port of server-side LivingEntity#handleFallFlyingCollisions. This dispatches
/// sound before typed damage; the damage pipeline retains its current scope.
pub fn handle_fall_flying_collisions<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    old_speed: f64,
    new_speed: f64,
    level: &mut Level,
) -> Result<(), LevelError> {
    if level.entity::<EntityData>(entity)?.horizontal_collision {
        let damage = ((old_speed - new_speed) * 10.0 - 3.0) as f32;
        if damage > 0.0 {
            let sounds = behavior.fall_sounds();
            let sound = if damage as i32 > 4 {
                sounds.big
            } else {
                sounds.small
            };
            behavior.play_sound(entity, sound, 1.0, 1.0, level)?;
            let source = level.damage_sources().fly_into_wall();
            behavior.hurt_server(entity, level, source, damage)?;
        }
    }
    Ok(())
}

/// Port of server-side LivingEntity#travelFallFlying. A climbable switches to
/// the air body before stopping flight; other movement uses Entity collision.
pub fn travel_fall_flying<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    input: Vec3,
    level: &mut Level,
) -> Result<(), LevelError> {
    if super::super::climbable::on_climbable(behavior, entity, level)? {
        super::travel_in_air(behavior, entity, input, level)?;
        stop_fall_flying(entity, level)
    } else {
        let movement = level.entity::<EntityData>(entity)?.velocity;
        let old_speed = movement.horizontal_distance();
        let movement = update_fall_flying_movement(behavior, entity, movement, level)?;
        level.entity_mut::<EntityData>(entity)?.velocity = movement;
        behavior.move_by(entity, level, MoverType::SelfMovement, movement)?;
        let new_speed = level
            .entity::<EntityData>(entity)?
            .velocity
            .horizontal_distance();
        handle_fall_flying_collisions(behavior, entity, old_speed, new_speed, level)
    }
}

#[cfg(test)]
mod tests;
