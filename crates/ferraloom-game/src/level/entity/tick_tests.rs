use super::*;
use crate::{
    Behaviors,
    entity::{
        Entity,
        player::{GameType, ServerPlayer},
    },
    level::{Region, chunk::LevelChunk, tick_rate::TickRateManager},
};
use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
use ferraloom_data::EntityType;
use std::sync::Arc;

struct Probe;
impl Entity for Probe {
    fn trace_entity_refs(&self, state: &EntityState, references: &mut Vec<EntityRef>) {
        crate::entity::trace_base_entity_refs(state.as_entity(), references);
    }
    fn check_despawn(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        let tag = &mut level.entity_mut::<EntityData>(entity)?.custom_data;
        tag.insert(
            "despawn_checks",
            ferraloom_nbt::Tag::Int(tag.get_int_or("despawn_checks", 0) + 1),
        );
        Ok(())
    }
    fn tick(&self, _: EntityRef, _: &mut Level) -> Result<(), LevelError> {
        Ok(())
    }
}
fn level() -> Level {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY
        .with_entity(EntityType::COW, &Probe)
        .with_entity(EntityType::PLAYER, &Probe);
    let mut level = Level::new(Region::new(&BEHAVIORS));
    level
        .add_chunk(LevelChunk::new(
            ChunkPos::ZERO,
            -4..20,
            LevelChunkSection::new(
                Strategy::block_states(),
                Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
                BiomeId(0),
            ),
        ))
        .unwrap();
    level
        .update_entity_chunk_status(ChunkPos::ZERO, Visibility::Ticking)
        .unwrap();
    level
}
fn add_cow(level: &mut Level) -> EntityRef {
    let id = level.next_entity_ref();
    level
        .add_entity(id, EntityState::Cow(Box::default()))
        .unwrap();
    id
}
fn add_player(level: &mut Level) -> EntityRef {
    let id = level.next_entity_ref();
    let player = ServerPlayer::new(
        Uuid::new(1, id.0 as i64),
        "fixture".into(),
        level.next_entity_id(),
        Vec3::ZERO,
        GameType::Survival,
    );
    level
        .add_entity(id, EntityState::Player(Box::new(player)))
        .unwrap();
    id
}

#[test]
fn simulation_eligibility_is_separate_from_visibility_and_follows_despawn() {
    let mut level = level();
    let cow = add_cow(&mut level);
    let player = add_player(&mut level);
    level.tick_entities().unwrap();
    let base = level.entity::<EntityData>(cow).unwrap();
    assert_eq!(base.tick_count, 0);
    assert_eq!(base.custom_data.get_int("despawn_checks"), Some(1));
    assert_eq!(level.entity::<EntityData>(player).unwrap().tick_count, 1);
    assert_eq!(level.all_entities(), vec![cow, player]);
    level
        .set_chunk_entity_ticking(ChunkPos::ZERO, true)
        .unwrap();
    level.tick_entities().unwrap();
    assert_eq!(level.entity::<EntityData>(cow).unwrap().tick_count, 1);
    assert_eq!(
        level
            .entity::<EntityData>(cow)
            .unwrap()
            .custom_data
            .get_int("despawn_checks"),
        Some(2)
    );
}

#[test]
fn frozen_entities_with_transitive_player_passengers_still_tick() {
    let mut level = level();
    level
        .set_chunk_entity_ticking(ChunkPos::ZERO, true)
        .unwrap();
    let root = add_cow(&mut level);
    let intermediate = add_cow(&mut level);
    let player = add_player(&mut level);
    let frozen = add_cow(&mut level);
    assert!(crate::entity::start_riding(intermediate, root, true, &mut level).unwrap());
    assert!(crate::entity::start_riding(player, intermediate, true, &mut level).unwrap());
    let mut rate = TickRateManager::default();
    rate.set_frozen(true);
    rate.tick();
    level.tick_entities_with_tick_rate(&rate).unwrap();
    for id in [root, intermediate, player] {
        assert_eq!(level.entity::<EntityData>(id).unwrap().tick_count, 1);
    }
    let base = level.entity::<EntityData>(frozen).unwrap();
    assert_eq!(base.tick_count, 0);
    assert!(base.custom_data.get_int("despawn_checks").is_none());
}
