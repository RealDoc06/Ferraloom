//! LivingEntity's required block/tag reads for climbing and pushability.
use super::LivingEntity;
use crate::{
    entity::{Entity, EntityRef},
    level::{Level, LevelError},
};
use ferraloom_data::{Block, BlockClass, EntityType, properties, tags::BlockTags};
use ferraloom_util::BlockPos;

/// Port of LivingEntity#onClimbable. Failed tests retain the last successful
/// climbable position; the cached in-block state follows Entity's lifetime.
pub fn on_climbable<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &Level,
) -> Result<bool, LevelError> {
    if behavior.is_spectator(entity, level)? {
        return Ok(false);
    }
    let living = level.entity::<LivingEntity>(entity)?;
    let pos = living.block_position;
    let state = living.in_block_state(level)?;
    if living.shared_flag(7)
        && level
            .block_tags()?
            .contains_key(BlockTags::CAN_GLIDE_THROUGH, state.block())
    {
        return Ok(false);
    }
    let climbable = level
        .block_tags()?
        .contains_key(BlockTags::CLIMBABLE, state.block());
    let trapdoor = if !climbable && state.block().is_instance_of(BlockClass::TrapDoorBlock) {
        if state.get(properties::OPEN).expect("TrapDoorBlock has OPEN") {
            let below = level.get_block_state(BlockPos::new(pos.x, pos.y - 1, pos.z))?;
            below.block() == Block::LADDER
                && below
                    .get(properties::FACING)
                    .expect("LadderBlock has FACING")
                    == state
                        .get(properties::FACING)
                        .expect("TrapDoorBlock has FACING")
        } else {
            false
        }
    } else {
        false
    };
    if climbable || trapdoor {
        living.last_climbable_pos.set(Some(pos));
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Port of LivingEntity#isPushable. Always-ticking players bypass both the
/// section-manager status and the separate simulation-distance predicate.
pub fn is_pushable<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &Level,
) -> Result<bool, LevelError> {
    let living = level.entity::<LivingEntity>(entity)?;
    if living.entity_type != EntityType::PLAYER
        && !(level
            .entity_chunk_visibility(living.chunk_position)
            .is_ticking()
            && level.is_entity_ticking_chunk(living.chunk_position))
    {
        return Ok(false);
    }
    Ok(behavior.is_alive(entity, level)?
        && !behavior.is_spectator(entity, level)?
        && !on_climbable(behavior, entity, level)?)
}

#[cfg(test)]
mod tests;
