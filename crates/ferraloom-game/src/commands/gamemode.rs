//! ServerPlayerGameMode's flight correction at the game-mode change call site.
use crate::{
    entity::{
        EntityData, EntityRef, collision,
        player::{GameType, ServerPlayer},
    },
    level::{Level, LevelError},
};
use ferraloom_util::{Aabb, Axis, shapes};

/// Port of ServerPlayerGameMode#isInRangeOfGround and Entity#getAvailableSpaceBelow(1).
/// Queries use the current game mode's canonical collision context.
pub fn is_in_range_of_ground(player: EntityRef, level: &Level) -> Result<bool, LevelError> {
    let bb = level.entity::<EntityData>(player)?.bounding_box;
    let collect = |bounds| {
        let entities = collision::entity_collisions(level, Some(player), bounds)?;
        collision::collect_colliders(level, Some(player), bounds, &entities)
    };
    if !collect(bb)?.is_empty() {
        return Ok(false);
    }
    let below = Aabb::new(
        bb.min_x,
        bb.min_y - 1.0,
        bb.min_z,
        bb.max_x,
        bb.min_y,
        bb.max_z,
    );
    let colliders = collect(below)?;
    let available = if colliders.is_empty() {
        1.0
    } else {
        -shapes::collide(Axis::Y, bb, &colliders, -1.0)
    };
    Ok(available < 1.0)
}
/// Port of ServerPlayerGameMode#changeGameModeForPlayer's flight branch, after
/// GameType#updatePlayerAbilities and before its first abilities packet.
pub fn stop_flying_if_near_ground(player: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let state = level.entity::<ServerPlayer>(player)?;
    if state.abilities.flying
        && state.game_mode != GameType::Spectator
        && is_in_range_of_ground(player, level)?
    {
        level.entity_mut::<ServerPlayer>(player)?.abilities.flying = false;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_data::Block;
    use ferraloom_util::{BlockPos, Uuid, Vec3};
    #[test]
    fn native_mode_change_ground_predicate_matches_clipping_and_below_queries() {
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("vanilla/gamemode-ground-vectors.json")).unwrap();
        for row in &rows {
            let mut level = crate::entity::tests::level(false);
            level.set_unported_behavior(crate::block::UnportedBehavior::Strict);
            level
                .set_block_state(BlockPos::new(0, 63, 0), Block::BEDROCK.default_state())
                .unwrap();
            if row["ceiling"] == true {
                level
                    .set_block_state(BlockPos::new(0, 65, 0), Block::BEDROCK.default_state())
                    .unwrap();
            }
            let player = level.next_entity_ref();
            let state = ServerPlayer::new(
                Uuid::new(0, 1),
                "ground".into(),
                level.next_entity_id(),
                Vec3::new(0.5, row["y"].as_f64().unwrap(), 0.5),
                GameType::Creative,
            );
            level
                .add_entity(player, crate::entity::EntityState::Player(Box::new(state)))
                .unwrap();
            assert_eq!(
                is_in_range_of_ground(player, &level).unwrap(),
                row["near"].as_bool().unwrap(),
                "{row}"
            );
            assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        }
        assert_eq!(rows.len(), 16);
    }
}
