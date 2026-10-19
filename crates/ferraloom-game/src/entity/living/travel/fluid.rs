//! LivingEntity's fluid travel bodies, using Entity movement and Level queries.
use super::{Entity, EntityData, EntityRef, Level, LevelError, LivingEntity};
use crate::entity::movement::MoverType;
use ferraloom_data::{
    FluidState,
    registries::{attribute::Id as Attribute, mob_effect::Id as MobEffect},
    tags::{EntityTypeTags, FluidTags, TagKey},
};
use ferraloom_util::Vec3;

/// Port of LivingEntity#shouldTravelInFluid, including Player's flying override.
pub fn should_travel_in_fluid<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    state: FluidState,
    level: &Level,
) -> Result<bool, LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    let in_liquid = base.was_touching_water || base.is_in_lava(level)?;
    Ok(in_liquid
        && behavior.is_affected_by_fluids(entity, level)?
        && !behavior.can_stand_on_fluid(entity, state, level)?)
}

/// Port of LivingEntity#travelInFluid. Falling, position and gravity are captured
/// before movement; ridden flotation follows the complete water or lava body.
pub fn travel_in_fluid<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    input: Vec3,
    level: &mut Level,
) -> Result<(), LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    let falling = base.velocity.y <= 0.0;
    let old_y = base.position.y;
    let gravity = behavior.effective_gravity(entity, level)?;
    if level.entity::<EntityData>(entity)?.was_touching_water {
        travel_in_water(behavior, entity, input, gravity, falling, old_y, level)?;
    } else {
        travel_in_lava(behavior, entity, input, gravity, falling, old_y, level)?;
    }
    float_in_liquid_while_ridden(entity, FluidTags::ENTITY_FLOATABLE, level)
}

/// Port of LivingEntity#travelInWater, retaining native float interpolation and
/// post-move climb, gravity adjustment and exit-probe order.
pub fn travel_in_water<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    input: Vec3,
    gravity: f64,
    falling: bool,
    old_y: f64,
    level: &mut Level,
) -> Result<(), LevelError> {
    let mut slow_down = if level.entity::<EntityData>(entity)?.shared_flag(3) {
        0.9
    } else {
        behavior.water_slow_down()
    };
    let mut speed = 0.02;
    let mut efficiency =
        super::super::attribute(entity, Attribute::WATER_MOVEMENT_EFFICIENCY, level)? as f32;
    if !level.entity::<EntityData>(entity)?.on_ground {
        efficiency *= 0.5;
    }
    if efficiency > 0.0 {
        slow_down += (0.54600006 - slow_down) * efficiency;
        speed += (behavior.get_speed(entity, level)? - speed) * efficiency;
    }
    if level
        .entity::<LivingEntity>(entity)?
        .has_effect(MobEffect::DOLPHINS_GRACE)
    {
        slow_down = 0.96;
    }
    level
        .entity_mut::<EntityData>(entity)?
        .move_relative(speed, input);
    let movement = level.entity::<EntityData>(entity)?.velocity;
    behavior.move_by(entity, level, MoverType::SelfMovement, movement)?;
    let mut movement = level.entity::<EntityData>(entity)?.velocity;
    if level.entity::<EntityData>(entity)?.horizontal_collision
        && super::super::climbable::on_climbable(behavior, entity, level)?
    {
        movement.y = 0.2;
    }
    movement = Vec3::new(
        movement.x * f64::from(slow_down),
        movement.y * 0.800000011920929,
        movement.z * f64::from(slow_down),
    );
    let movement = level
        .entity::<LivingEntity>(entity)?
        .fluid_falling_adjusted_movement(gravity, falling, movement);
    level.entity_mut::<EntityData>(entity)?.velocity = movement;
    jump_out_of_fluid(behavior, entity, old_y, level)
}

/// Port of LivingEntity#travelInLava, including shallow-fluid gravity before the
/// separate lava gravity subtraction and the final exit probe.
pub fn travel_in_lava<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    input: Vec3,
    gravity: f64,
    falling: bool,
    old_y: f64,
    level: &mut Level,
) -> Result<(), LevelError> {
    level
        .entity_mut::<EntityData>(entity)?
        .move_relative(0.02, input);
    let movement = level.entity::<EntityData>(entity)?.velocity;
    behavior.move_by(entity, level, MoverType::SelfMovement, movement)?;
    let base = level.entity::<EntityData>(entity)?;
    let movement =
        if base.fluid_interaction.height(FluidTags::LAVA, level)? <= base.fluid_jump_threshold() {
            let movement = Vec3::new(
                base.velocity.x * 0.5,
                base.velocity.y * 0.800000011920929,
                base.velocity.z * 0.5,
            );
            level
                .entity::<LivingEntity>(entity)?
                .fluid_falling_adjusted_movement(gravity, falling, movement)
        } else {
            base.velocity.scale(0.5)
        };
    level.entity_mut::<EntityData>(entity)?.velocity = movement;
    if gravity != 0.0 {
        let base = level.entity_mut::<EntityData>(entity)?;
        base.velocity = base.velocity + Vec3::new(0.0, -gravity / 4.0, 0.0);
    }
    jump_out_of_fluid(behavior, entity, old_y, level)
}

/// Port of LivingEntity#jumpOutOfFluid. The required liquid scan lives in Level,
/// and Entity#isFree preserves collision-first short circuiting.
pub fn jump_out_of_fluid<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    old_y: f64,
    level: &mut Level,
) -> Result<(), LevelError> {
    let (movement, horizontal_collision, y) = {
        let base = level.entity::<EntityData>(entity)?;
        (base.velocity, base.horizontal_collision, base.position.y)
    };
    if horizontal_collision
        && behavior.is_free(
            entity,
            Vec3::new(
                movement.x,
                movement.y + 0.6000000238418579 - y + old_y,
                movement.z,
            ),
            level,
        )?
    {
        level.entity_mut::<EntityData>(entity)?.velocity =
            Vec3::new(movement.x, 0.30000001192092896, movement.z);
    }
    Ok(())
}

/// Port of LivingEntity#floatInLiquidWhileRidden with actual resolved tags.
pub fn float_in_liquid_while_ridden(
    entity: EntityRef,
    tag: TagKey<ferraloom_data::registries::fluid::Id>,
    level: &mut Level,
) -> Result<(), LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    if !base.passengers.is_empty()
        && level
            .entity_type_tags()?
            .contains_key(EntityTypeTags::CAN_FLOAT_WHILE_RIDDEN, base.entity_type)
        && base.fluid_interaction.height(tag, level)? > base.fluid_jump_threshold()
    {
        let base = level.entity_mut::<EntityData>(entity)?;
        base.velocity = base.velocity + Vec3::new(0.0, 0.03999999910593033, 0.0);
    }
    Ok(())
}

/// Port of LivingEntity#travelFlying's three-speed overload.
pub fn travel_flying<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    input: Vec3,
    speeds: [f32; 3],
    level: &mut Level,
) -> Result<(), LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    let (speed, drag) = if base.was_touching_water {
        (speeds[0], 0.800000011920929)
    } else if base.is_in_lava(level)? {
        (speeds[1], 0.5)
    } else {
        (speeds[2], 0.9100000262260437)
    };
    level
        .entity_mut::<EntityData>(entity)?
        .move_relative(speed, input);
    let movement = level.entity::<EntityData>(entity)?.velocity;
    behavior.move_by(entity, level, MoverType::SelfMovement, movement)?;
    let base = level.entity_mut::<EntityData>(entity)?;
    base.velocity = base.velocity.scale(drag);
    Ok(())
}

#[cfg(test)]
mod tests;
