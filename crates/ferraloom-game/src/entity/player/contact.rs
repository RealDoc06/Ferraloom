//! The ordered touch section of Player#aiStep.
use super::ServerPlayer;
use crate::{
    entity::{EntityData, EntityRef},
    level::{Level, LevelError},
};
use ferraloom_data::EntityType;
use ferraloom_util::{Aabb, random::RandomSource};

fn pickup_area(player: EntityRef, level: &Level) -> Result<Option<Aabb>, LevelError> {
    let state = level.entity::<ServerPlayer>(player)?;
    let healthy = state.health() > 0.0;
    if !healthy || level.entity_behavior(player)?.is_spectator(player, level)? {
        return Ok(None);
    }
    let bounds = state.bounding_box;
    if let Some(vehicle) = state.vehicle.get() {
        let vehicle = level.entity::<EntityData>(vehicle)?;
        if !vehicle.removed {
            return Ok(Some(
                bounds.minmax(vehicle.bounding_box).inflate(1.0, 0.0, 1.0),
            ));
        }
    }
    Ok(Some(bounds.inflate(1.0, 0.5, 1.0)))
}
/// Port of Player#aiStep's touch section. Capture the native query before any
/// callback; experience orbs are selected separately after the ordinary touches.
pub(super) fn touch_entities(player: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let Some(area) = pickup_area(player, level)? else {
        return Ok(());
    };
    let mut entities = Vec::new();
    for entity in level.entities_in(area, |id, _| id != player) {
        if !level.entity_behavior(entity)?.is_spectator(entity, level)? {
            entities.push(entity);
        }
    }
    let mut orbs = Vec::new();
    for entity in entities {
        let state = level.entity::<EntityData>(entity)?;
        if state.entity_type == EntityType::EXPERIENCE_ORB {
            orbs.push(entity);
        } else if !state.removed {
            level
                .entity_behavior(entity)?
                .player_touch(entity, player, level)?;
        }
    }
    if !orbs.is_empty() {
        let selected = level
            .entity_mut::<ServerPlayer>(player)?
            .random
            .next_int_bound(orbs.len() as i32);
        let entity = orbs[selected as usize];
        level
            .entity_behavior(entity)?
            .player_touch(entity, player, level)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
