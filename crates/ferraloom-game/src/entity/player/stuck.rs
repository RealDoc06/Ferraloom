//! Player's stuck-block override and ServerPlayer fall tracking/reset.
use super::ServerPlayer;
use crate::{
    entity::{Entity, EntityRef, LivingEntity},
    level::{Level, LevelError},
};
use ferraloom_data::BlockState;
use ferraloom_util::Vec3;

/// Port of Player#makeStuckInBlock. The Entity parent keeps ServerPlayer's
/// virtual reset; the impulse context is read only after that callback returns.
pub(super) fn make_stuck_in_block<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    state: BlockState,
    multiplier: Vec3,
    level: &mut Level,
) -> Result<(), LevelError> {
    if !level.entity::<ServerPlayer>(entity)?.abilities.flying {
        crate::entity::stuck::base_make_stuck_in_block(behavior, entity, state, multiplier, level)?;
    }
    level
        .entity_mut::<LivingEntity>(entity)?
        .try_reset_current_impulse_context();
    Ok(())
}

/// Port of ServerPlayer#resetFallDistance. A failed/reentrant criterion sees the
/// original fall state; clearing happens after its synchronous callback.
pub(super) fn reset_fall_distance(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let player = level.entity::<ServerPlayer>(entity)?;
    if player.health() > 0.0
        && let Some(start) = player.starting_to_fall_position
    {
        crate::advancements::criteria::fall_from_height(level, entity, start)?;
    }
    level
        .entity_mut::<ServerPlayer>(entity)?
        .starting_to_fall_position = None;
    crate::entity::stuck::base_reset_fall_distance(entity, level)
}

/// Port of ServerPlayer#trackStartFallingPosition's normal-fall capture.
pub(super) fn track_start_falling_position(
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    let player = level.entity_mut::<ServerPlayer>(entity)?;
    if player.fall_distance > 0.0 && player.starting_to_fall_position.is_none() {
        player.starting_to_fall_position = Some(player.position);
        // Pending FALL_AFTER_EXPLOSION here uses explosion attribution; the
        // unported explosion system cannot produce that context yet.
    }
    Ok(())
}

#[cfg(test)]
mod tests;
