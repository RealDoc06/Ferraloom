//! Player's wrapper around the LivingEntity travel body.
use super::ServerPlayer;
use crate::{
    entity::{Entity, EntityData, EntityRef},
    level::{Level, LevelError},
};
use ferraloom_util::{BlockPos, Vec3};

/// Port of Player#travel. Passengers invoke the parent directly; swimming steers
/// before that call, and flight restores its captured vertical motion after it.
pub fn travel<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    input: Vec3,
    level: &mut Level,
) -> Result<(), LevelError> {
    if level.entity::<EntityData>(entity)?.vehicle.get().is_none() {
        if behavior.is_swimming(entity, level)? {
            let (look_y, jumping, position) = {
                let player = level.entity::<ServerPlayer>(entity)?;
                (player.look_angle().y, player.jumping, player.position)
            };
            let multiplier = if look_y < -0.2 { 0.085 } else { 0.06 };
            if look_y <= 0.0
                || jumping
                || !level
                    .get_block_state(BlockPos::containing(
                        position.x,
                        position.y + 1.0 - 0.1,
                        position.z,
                    ))?
                    .data()
                    .fluid
                    .empty
            {
                let base = level.entity_mut::<EntityData>(entity)?;
                base.velocity =
                    base.velocity + Vec3::new(0.0, (look_y - base.velocity.y) * multiplier, 0.0);
            }
        }
        if level.entity::<ServerPlayer>(entity)?.abilities.flying {
            let original_y = level.entity::<EntityData>(entity)?.velocity.y;
            // Explicit LivingEntity parent body; virtual speed/fluid predicates
            // continue to use the selected Player dispatcher.
            crate::entity::living::travel::travel(behavior, entity, input, level)?;
            level.entity_mut::<EntityData>(entity)?.velocity.y = original_y * 0.6;
            return Ok(());
        }
    }
    crate::entity::living::travel::travel(behavior, entity, input, level)
}

#[cfg(test)]
mod tests;
