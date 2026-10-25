//! Entity's stuck-block and fall-reset virtual chain.
use super::{Entity, EntityData, EntityRef};
use crate::level::{Level, LevelError};
use ferraloom_data::BlockState;
use ferraloom_util::Vec3;

/// Port of Entity#makeStuckInBlock's virtual entry point.
pub fn make_stuck_in_block(
    entity: EntityRef,
    state: BlockState,
    multiplier: Vec3,
    level: &mut Level,
) -> Result<(), LevelError> {
    level
        .entity_behavior(entity)?
        .make_stuck_in_block(entity, state, multiplier, level)
}

/// Port of Entity#resetFallDistance's virtual entry point.
pub fn reset_fall_distance(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    level
        .entity_behavior(entity)?
        .reset_fall_distance(entity, level)
}

/// Explicit Entity parent body. Its reset call remains virtual on the subclass.
pub(crate) fn base_make_stuck_in_block<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    _state: BlockState,
    multiplier: Vec3,
    level: &mut Level,
) -> Result<(), LevelError> {
    behavior.reset_fall_distance(entity, level)?;
    level
        .entity_mut::<EntityData>(entity)?
        .stuck_speed_multiplier = multiplier;
    Ok(())
}

/// Port of Entity#resetFallDistance's explicit parent body.
pub(crate) fn base_reset_fall_distance(
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    level.entity_mut::<EntityData>(entity)?.fall_distance = 0.0;
    Ok(())
}
