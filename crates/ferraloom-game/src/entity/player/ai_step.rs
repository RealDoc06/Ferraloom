//! Player#aiStep and its inherited LivingEntity server phases.
use super::{ServerPlayer, contact, food, item_tick, shoulders};
use crate::{
    entity::{Entity, EntityRef, LivingEntity, living},
    level::{Level, LevelError, tick_rate::TickRateManager},
};
use ferraloom_data::registries::attribute::Id as Attribute;

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
    living::ai_step(behavior, entity, level, tick_rate)?;
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
