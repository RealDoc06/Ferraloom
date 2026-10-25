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
    level_with_behaviors(&BEHAVIORS)
}
fn level_with_behaviors(behaviors: &'static Behaviors) -> Level {
    let mut level = {
        let mut fixture_level = Level::new(
            Region::new(behaviors),
            crate::level::LevelContext::fixture(0, 63),
        );
        fixture_level.install_shared(Box::default());
        fixture_level
    };
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

struct TickContextProbe;
impl Entity for TickContextProbe {
    fn trace_entity_refs(&self, state: &EntityState, references: &mut Vec<EntityRef>) {
        crate::entity::trace_base_entity_refs(state.as_entity(), references);
    }
    fn tick_with_tick_rate(
        &self,
        entity: EntityRef,
        level: &mut Level,
        tick_rate: &TickRateManager,
    ) -> Result<(), LevelError> {
        level.entity_mut::<EntityData>(entity)?.custom_data.insert(
            "runs_normally",
            ferraloom_nbt::Tag::Byte(i8::from(tick_rate.runs_normally())),
        );
        Ok(())
    }
    fn ride_tick_with_tick_rate(
        &self,
        entity: EntityRef,
        level: &mut Level,
        tick_rate: &TickRateManager,
    ) -> Result<(), LevelError> {
        crate::entity::living_ride_tick_with_tick_rate(self, entity, level, tick_rate)
    }
}

#[test]
fn entity_and_passenger_ticks_borrow_the_callers_manager_without_freeze_state() {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY
        .with_entity(EntityType::COW, &TickContextProbe)
        .with_entity(EntityType::PLAYER, &TickContextProbe);
    let mut level = level_with_behaviors(&BEHAVIORS);
    level
        .set_chunk_entity_ticking(ChunkPos::ZERO, true)
        .unwrap();
    let root = add_cow(&mut level);
    let intermediate = add_cow(&mut level);
    let player = add_player(&mut level);
    crate::entity::start_riding(intermediate, root, true, &mut level).unwrap();
    crate::entity::start_riding(player, intermediate, true, &mut level).unwrap();
    let mut rate = TickRateManager::default();
    for frozen in [true, false] {
        rate.set_frozen(frozen);
        rate.tick();
        level.tick_entities_with_tick_rate(&rate).unwrap();
        for entity in [root, intermediate, player] {
            assert_eq!(
                level
                    .entity::<EntityData>(entity)
                    .unwrap()
                    .custom_data
                    .get_boolean_or("runs_normally", frozen),
                !frozen
            );
        }
    }
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
        level.next_entity_id().unwrap(),
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

struct SwitchMount;
impl Entity for SwitchMount {
    fn trace_entity_refs(&self, state: &EntityState, references: &mut Vec<EntityRef>) {
        crate::entity::trace_base_entity_refs(state.as_entity(), references);
        references.extend(state.as_mob().and_then(|mob| mob.target));
    }
    fn tick(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        if let Some(next) = level.entity_state(entity)?.as_mob().unwrap().target {
            crate::entity::start_riding(entity, next, true, level)?;
        } else {
            crate::entity::stop_riding(entity, level)?;
        }
        Ok(())
    }
    fn ride_tick(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        crate::entity::living_ride_tick(self, entity, level)
    }
}

#[test]
fn vanilla_ride_tick_uses_the_vehicle_after_derived_tick_and_resets_living_fall_distance() {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY.with_entity(EntityType::COW, &SwitchMount);
    let mut rows = 0;
    for row in include_str!("../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|row| row.starts_with("riding\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let mut level = {
            let mut fixture_level = Level::new(
                Region::new(&BEHAVIORS),
                crate::level::LevelContext::fixture(0, 63),
            );
            fixture_level.install_shared(Box::default());
            fixture_level
        };
        level
            .update_entity_chunk_status(ChunkPos::ZERO, Visibility::Ticking)
            .unwrap();
        let mut add_mount = |seed| {
            let id = level.next_entity_ref();
            let tnt = crate::entity::item::PrimedTnt::new(level.next_entity_id().unwrap(), seed);
            level
                .add_entity(id, EntityState::PrimedTnt(Box::new(tnt)))
                .unwrap();
            id
        };
        let old = add_mount(1);
        let next = add_mount(2);
        let rider = add_cow(&mut level);
        level
            .set_entity_pos(old, Vec3::new(1.0, 64.0, 1.0))
            .unwrap();
        level
            .set_entity_pos(next, Vec3::new(5.0, 67.0, 8.0))
            .unwrap();
        level
            .set_entity_pos(rider, Vec3::new(9.0, 70.0, 9.0))
            .unwrap();
        if fields[1] == "switch" {
            level
                .entity_state_mut(rider)
                .unwrap()
                .as_mob_mut()
                .unwrap()
                .target = Some(next);
        }
        crate::entity::start_riding(rider, old, true, &mut level).unwrap();
        let base = level.entity_mut::<EntityData>(rider).unwrap();
        base.velocity = Vec3::new(1.0, 2.0, 3.0);
        base.fall_distance = 8.0;
        level
            .tick_passenger(
                old,
                rider,
                &crate::level::tick_rate::TickRateManager::default(),
            )
            .unwrap();
        let base = level.entity::<EntityData>(rider).unwrap();
        let bits = |value: Vec3| {
            format!(
                "{:x},{:x},{:x}",
                value.x.to_bits(),
                value.y.to_bits(),
                value.z.to_bits()
            )
        };
        assert_eq!(bits(base.position), fields[2], "{row}");
        assert_eq!(bits(base.velocity), fields[3], "{row}");
        assert_eq!(
            format!("{:x}", base.fall_distance.to_bits()),
            fields[4],
            "{row}"
        );
        assert_eq!(
            (base.vehicle.get() == Some(next)).to_string(),
            fields[5],
            "{row}"
        );
        assert_eq!(base.tick_count, 1);
        rows += 1;
    }
    assert_eq!(rows, 2);
}

#[test]
fn sneaking_player_dismounts_before_riding_tick_without_zeroing_velocity_or_fall_distance() {
    let mut level = level();
    let vehicle = add_cow(&mut level);
    let player = add_player(&mut level);
    crate::entity::start_riding(player, vehicle, true, &mut level).unwrap();
    let base = level.entity_mut::<EntityData>(player).unwrap();
    base.set_shared_flag(1, true);
    base.velocity = Vec3::new(1.0, 2.0, 3.0);
    base.fall_distance = 8.0;
    crate::entity::Entity::ride_tick(&crate::entity::player::PlayerBehavior, player, &mut level)
        .unwrap();
    let base = level.entity::<EntityData>(player).unwrap();
    assert_eq!(base.vehicle.get(), None);
    assert_eq!(base.velocity, Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(base.fall_distance, 8.0);
    assert_eq!(base.boarding_cooldown, 0);
}
