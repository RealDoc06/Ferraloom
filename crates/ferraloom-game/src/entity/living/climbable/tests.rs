use super::*;
use crate::entity::{
    EntityData, EntityState,
    player::{GameType, ServerPlayer},
};
use ferraloom_data::BlockState;
use ferraloom_util::{ChunkPos, Uuid, Vec3};

const VECTORS: &str = include_str!("../../../../tests/java/entity/vectors.tsv");
fn player(level: &mut Level, mode: GameType) -> EntityRef {
    let entity = level.next_entity_ref();
    let player = ServerPlayer::new(
        Uuid::new(5, 6),
        "climb".into(),
        level.next_entity_id().unwrap(),
        Vec3::new(8.5, 64.0, 8.5),
        mode,
    );
    level
        .add_entity(entity, EntityState::Player(Box::new(player)))
        .unwrap();
    entity
}
fn last(entity: EntityRef, level: &Level) -> String {
    level
        .entity::<LivingEntity>(entity)
        .unwrap()
        .last_climbable_pos
        .get()
        .map_or_else(
            || "none".to_owned(),
            |pos| format!("{},{},{}", pos.x, pos.y, pos.z),
        )
}

#[test]
fn climbing_and_player_pushability_match_native_blocks_health_and_spectator_queries() {
    let mut count = 0;
    for row in VECTORS
        .lines()
        .filter(|line| line.starts_with("living-climb\t"))
    {
        let row: Vec<_> = row.split('\t').collect();
        let mut level = crate::entity::tests::level(false);
        level
            .set_block_state(
                BlockPos::new(8, 64, 8),
                BlockState::from_id(row[2].parse().unwrap()).unwrap(),
            )
            .unwrap();
        level
            .set_block_state(
                BlockPos::new(8, 63, 8),
                BlockState::from_id(row[3].parse().unwrap()).unwrap(),
            )
            .unwrap();
        let entity = player(&mut level, GameType::from_id(row[4].parse().unwrap()));
        let living = level.entity_mut::<LivingEntity>(entity).unwrap();
        living.set_shared_flag(7, row[5].parse().unwrap());
        living.set_health(f32::from_bits(u32::from_str_radix(row[6], 16).unwrap()));
        let behavior = level.entity_behavior(entity).unwrap();
        assert_eq!(
            on_climbable(behavior, entity, &level).unwrap().to_string(),
            row[7],
            "climb case {}",
            row[1]
        );
        assert_eq!(
            behavior.is_pushable(entity, &level).unwrap().to_string(),
            row[8],
            "pushable case {}",
            row[1]
        );
        assert_eq!(last(entity, &level), row[9]);
        count += 1;
    }
    assert_eq!(count, 12);
}

#[test]
fn native_in_block_cache_and_last_climbable_position_survive_block_replacement() {
    let mut level = crate::entity::tests::level(false);
    let pos = BlockPos::new(8, 64, 8);
    level
        .set_block_state(pos, Block::LADDER.default_state())
        .unwrap();
    let entity = player(&mut level, GameType::Survival);
    let behavior = level.entity_behavior(entity).unwrap();
    let first = on_climbable(behavior, entity, &level).unwrap();
    level
        .set_block_state(pos, Block::AIR.default_state())
        .unwrap();
    let cached = on_climbable(behavior, entity, &level).unwrap();
    level
        .set_entity_pos(entity, Vec3::new(8.75, 64.0, 8.5))
        .unwrap();
    let same_block = on_climbable(behavior, entity, &level).unwrap();
    level
        .set_entity_pos(entity, Vec3::new(9.5, 64.0, 8.5))
        .unwrap();
    level
        .set_entity_pos(entity, Vec3::new(8.5, 64.0, 8.5))
        .unwrap();
    let invalidated = on_climbable(behavior, entity, &level).unwrap();
    let expected = VECTORS
        .lines()
        .find(|line| line.starts_with("living-climb-cache\t"))
        .unwrap();
    assert_eq!(
        format!(
            "living-climb-cache\t{first},{cached},{same_block},{invalidated}\t{}",
            last(entity, &level)
        ),
        expected
    );
    assert_eq!(
        level
            .entity::<EntityData>(entity)
            .unwrap()
            .in_block_state
            .get(),
        Some(Block::AIR.default_state())
    );
}

#[test]
fn nonplayer_pushability_requires_both_visibility_and_simulation_eligibility() {
    let mut level = crate::entity::tests::level(false);
    let entity = level.next_entity_ref();
    let mut cow = crate::entity::animal::Cow::default();
    cow.as_living_mut().set_pos(Vec3::new(8.5, 64.0, 8.5));
    level
        .add_entity(entity, EntityState::Cow(Box::new(cow)))
        .unwrap();
    let behavior = level.entity_behavior(entity).unwrap();
    assert!(!behavior.is_pushable(entity, &level).unwrap());
    level
        .set_chunk_entity_ticking(ChunkPos::ZERO, true)
        .unwrap();
    assert!(behavior.is_pushable(entity, &level).unwrap());
    level
        .update_entity_chunk_status(ChunkPos::ZERO, crate::level::entity::Visibility::Tracked)
        .unwrap();
    assert!(!behavior.is_pushable(entity, &level).unwrap());
}
