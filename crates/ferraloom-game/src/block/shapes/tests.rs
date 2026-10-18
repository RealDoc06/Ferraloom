use super::*;
use crate::{
    Behaviors,
    entity::{EntityData, EntityRef, player::ServerPlayer},
    level::{LevelContext, Region, entity::Visibility},
};
use ferraloom_data::Block;
use ferraloom_util::{ChunkPos, Uuid, Vec3};
use ferraloom_worldgen::feature::LiveWorldGenLevel;

const POS: BlockPos = BlockPos::new(8, 64, 8);
static EMPTY: Behaviors = Behaviors::EMPTY;

fn player_level() -> (Level, EntityRef) {
    let mut level = Level::new(Region::new(&EMPTY), LevelContext::fixture(0, 63));
    level
        .update_entity_chunk_status(ChunkPos::containing(POS), Visibility::Tracked)
        .unwrap();
    let entity = level.next_entity_ref();
    let player = ServerPlayer::new(
        Uuid::offline_player("Placement"),
        "Placement".into(),
        level.next_entity_id(),
        Vec3::new(8.5, 64.0, 8.5),
        GameType::Survival,
    );
    level
        .add_entity(entity, EntityState::Player(Box::new(player)))
        .unwrap();
    (level, entity)
}

#[test]
fn placement_query_includes_the_context_player() {
    let (level, player) = player_level();
    let context =
        BlockCollisionContext::Entity(EntityCollisionView::capture(player, &level, false).unwrap());
    let stone = Block::STONE.default_state();
    assert!(!level.is_unobstructed(stone, POS, &context).unwrap());
    assert!(!LiveWorldGenLevel::is_unobstructed(&level, stone, [8, 64, 8]).unwrap());
}

#[test]
fn only_unremoved_blocks_building_entities_obstruct() {
    let (mut level, player) = player_level();
    let stone = Block::STONE.default_state();
    let context = BlockCollisionContext::empty();
    level
        .entity_mut::<EntityData>(player)
        .unwrap()
        .blocks_building = false;
    assert!(level.is_unobstructed(stone, POS, &context).unwrap());
    level
        .entity_mut::<EntityData>(player)
        .unwrap()
        .blocks_building = true;
    assert!(!level.is_unobstructed(stone, POS, &context).unwrap());
    // Keep the receiver in the query index to exercise EntityGetter's own guard.
    level.entity_mut::<EntityData>(player).unwrap().removed = true;
    assert!(level.is_unobstructed(stone, POS, &context).unwrap());
}

#[test]
fn spectators_are_excluded_but_creative_players_obstruct() {
    let (mut level, player) = player_level();
    let stone = Block::STONE.default_state();
    let context = BlockCollisionContext::empty();
    level.entity_mut::<ServerPlayer>(player).unwrap().game_mode = GameType::Spectator;
    assert!(level.is_unobstructed(stone, POS, &context).unwrap());
    level.entity_mut::<ServerPlayer>(player).unwrap().game_mode = GameType::Creative;
    assert!(!level.is_unobstructed(stone, POS, &context).unwrap());
}

#[test]
fn empty_shape_short_circuits_and_the_supplied_context_selects_the_shape() {
    let (mut level, player) = player_level();
    let empty = BlockCollisionContext::empty();
    assert!(
        level
            .is_unobstructed(Block::AIR.default_state(), POS, &empty)
            .unwrap()
    );
    level
        .entity_mut::<EntityData>(player)
        .unwrap()
        .fall_distance = 3.0;
    let powder = Block::POWDER_SNOW.default_state();
    let mut entity_context = EntityCollisionView::capture(player, &level, false).unwrap();
    assert!(level.is_unobstructed(powder, POS, &empty).unwrap());
    assert!(LiveWorldGenLevel::is_unobstructed(&level, powder, [8, 64, 8]).unwrap());
    assert!(
        !level
            .is_unobstructed(powder, POS, &BlockCollisionContext::Entity(entity_context))
            .unwrap()
    );
    entity_context = EntityCollisionView::capture(player, &level, false).unwrap();
    entity_context.placement = true;
    assert!(
        level
            .is_unobstructed(powder, POS, &BlockCollisionContext::Entity(entity_context))
            .unwrap()
    );
}
