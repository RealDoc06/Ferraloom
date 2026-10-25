use ferraloom_data::{Block, BlockState, EntityType};
use ferraloom_util::{BlockPos, ChunkPos};

use crate::{
    Behaviors,
    block::BlockBehavior,
    entity::{
        DamageSource, Entity, EntityRef, EntityState,
        animal::{Cow, CowBehavior},
        hurt_unarmored,
    },
    level::{Level, LevelError, Region, chunk::LevelChunk},
    ticks::{LevelTicks, TickPriority},
};

const ATTACKER: EntityRef = EntityRef(1);
const TARGET: EntityRef = EntityRef(2);
const PLACED: BlockPos = BlockPos::new(-1, 64, -1);

struct TestCow;
impl Entity for TestCow {
    fn trace_entity_refs(&self, state: &EntityState, references: &mut Vec<EntityRef>) {
        CowBehavior.trace_entity_refs(state, references);
    }

    fn tick(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        CowBehavior.tick(entity, level)?;
        // These borrows end before damage calls back into this active entity.
        let target = level.entity::<Cow>(entity)?.as_mob().target;
        if let Some(mut target) = target {
            let source = level.damage_sources().mob_attack(entity);
            target.hurt_server(level, source, 4.0)?;
            level.set_block_state(PLACED, Block::STONE.default_state())?;
            level.schedule_block_tick(PLACED, Block::STONE, 1, TickPriority::Normal)?;
            // Re-reading sees the callback's mutation, with no restore step.
            assert_eq!(level.entity::<Cow>(entity)?.as_living().health(), 9.0);
        }
        Ok(())
    }

    fn hurt_server(
        &self,
        entity: EntityRef,
        level: &mut Level,
        source: DamageSource,
        damage: f32,
    ) -> Result<bool, LevelError> {
        let attacker = source.causing_entity();
        let accepted = hurt_unarmored(entity, level, source, damage)?;
        if accepted && let Some(mut attacker) = attacker {
            assert_eq!(
                level.entity::<Cow>(attacker)?.as_living().parent.tick_count,
                1
            );
            assert!(!level.entity::<Cow>(attacker)?.as_living().parent.first_tick);
            // Adversarial virtual override, like damage reflection: enter the
            // attacker synchronously while its outer tick frame remains active.
            let source = level.damage_sources().generic();
            attacker.hurt_server(level, source, 1.0)?;
        }
        Ok(accepted)
    }
}

struct TurnToDirt;
impl BlockBehavior for TurnToDirt {
    fn tick(&self, _: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        level.set_block_state(pos, Block::DIRT.default_state())?;
        // A newly queued due tick cannot execute in the collection being drained.
        level.schedule_block_tick(pos, Block::DIRT, 1, TickPriority::Normal)
    }
}
struct TurnToAir;
impl BlockBehavior for TurnToAir {
    fn tick(&self, _: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        level.set_block_state(pos, Block::AIR.default_state())?;
        Ok(())
    }
}
static TEST_BEHAVIORS: Behaviors = Behaviors::EMPTY
    .with_entity(EntityType::COW, &TestCow)
    .with_block(Block::STONE, &TurnToDirt)
    .with_block(Block::DIRT, &TurnToAir);

fn region() -> Level {
    let mut region = {
        let mut fixture_level = Level::new(
            Region::new(&TEST_BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        fixture_level.install_shared(Box::default());
        fixture_level
    };
    for pos in [ChunkPos::containing(PLACED), ChunkPos::ZERO] {
        region
            .add_chunk(LevelChunk::new(
                pos,
                -4..20,
                ferraloom_chunk::LevelChunkSection::new(
                    ferraloom_chunk::Strategy::block_states(),
                    ferraloom_chunk::Strategy::biomes(std::sync::Arc::new(
                        ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
                    )),
                    ferraloom_chunk::BiomeId(0),
                ),
            ))
            .unwrap();
        region
            .update_entity_chunk_status(pos, crate::level::entity::Visibility::Ticking)
            .unwrap();
        region.set_chunk_entity_ticking(pos, true).unwrap();
        region.set_chunk_block_ticking(pos, true).unwrap();
        region
            .set_chunk_loading_status(
                pos,
                crate::level::distance_manager::FullChunkStatus::EntityTicking,
                true,
            )
            .unwrap();
    }
    for request in region.take_entity_storage_requests() {
        let crate::level::entity::EntityStorageRequest::Load { id, .. } = request else {
            panic!("new fixture only requests empty entity loads")
        };
        region.complete_entity_chunk_load(id, Ok(None)).unwrap();
    }
    region.process_pending_entity_loads().unwrap();
    region
}

#[test]
fn entity_tick_reenters_attacker_and_places_a_scheduled_block() {
    let mut region = region();
    let mut cow = Cow::default();
    cow.as_mob_mut().target = Some(TARGET);
    region
        .add_entity(ATTACKER, EntityState::Cow(Box::new(cow)))
        .unwrap();
    region
        .add_entity(TARGET, EntityState::Cow(Box::default()))
        .unwrap();
    region.tick_entities().unwrap();
    assert_eq!(
        region.entity::<Cow>(ATTACKER).unwrap().as_living().health(),
        9.0
    );
    assert_eq!(
        region.entity::<Cow>(TARGET).unwrap().as_living().health(),
        6.0
    );
    assert_eq!(
        region.get_block_state(PLACED).unwrap().block(),
        Block::STONE
    );
    region.tick_scheduled_blocks(0, 65536).unwrap();
    assert_eq!(
        region.get_block_state(PLACED).unwrap().block(),
        Block::STONE
    );
    region.tick_scheduled_blocks(1, 65536).unwrap();
    assert_eq!(region.get_block_state(PLACED).unwrap().block(), Block::DIRT);
    region.tick_scheduled_blocks(2, 65536).unwrap();
    assert_eq!(region.get_block_state(PLACED).unwrap().block(), Block::AIR);
}

#[test]
fn living_damage_cooldown_applies_only_the_increase() {
    let mut region = region();
    region
        .add_entity(TARGET, EntityState::Cow(Box::default()))
        .unwrap();
    let mut entity = TARGET;
    let source = region.damage_sources().generic();
    assert!(
        entity
            .hurt_server(&mut region, source.clone(), 3.0)
            .unwrap()
    );
    assert!(
        !entity
            .hurt_server(&mut region, source.clone(), 2.0)
            .unwrap()
    );
    assert!(entity.hurt_server(&mut region, source, 5.0).unwrap());
    assert_eq!(
        region.entity::<Cow>(TARGET).unwrap().as_living().health(),
        5.0
    );
    assert_eq!(
        region
            .entity::<Cow>(TARGET)
            .unwrap()
            .as_living()
            .damage_cooldown_time,
        20
    );
}

#[test]
fn scheduled_ticks_use_container_then_intra_tick_order_and_deduplicate() {
    let mut ticks = LevelTicks::default();
    let a = BlockPos::new(0, 0, 0);
    let b = BlockPos::new(1, 0, 0);
    let c = BlockPos::new(16, 0, 0);
    ticks.schedule(a, Block::STONE, 1, TickPriority::Low);
    ticks.schedule(b, Block::STONE, 2, TickPriority::ExtremelyHigh);
    ticks.schedule(c, Block::STONE, 3, TickPriority::Normal);
    ticks.schedule(a, Block::STONE, 0, TickPriority::ExtremelyHigh);
    // c beats a across chunks, but b cannot overtake a within its own chunk.
    let first = ticks.collect_due(3, 1);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].pos, c);
    let collected = ticks.collect_due(3, 65536);
    assert_eq!(
        collected.iter().map(|tick| tick.pos).collect::<Vec<_>>(),
        [a, b]
    );
    assert!(ticks.collect_due(3, 65536).is_empty());
}

#[test]
fn removed_entities_are_visible_until_management_and_never_tick() {
    let mut region = region();
    region
        .add_entity(TARGET, EntityState::Cow(Box::default()))
        .unwrap();
    region.remove_entity(TARGET).unwrap();
    region.tick_entities().unwrap();
    assert!(
        region
            .entity::<Cow>(TARGET)
            .unwrap()
            .as_living()
            .parent
            .removed
    );
    assert_eq!(
        region
            .entity::<Cow>(TARGET)
            .unwrap()
            .as_living()
            .parent
            .tick_count,
        0
    );
    region.finish_entity_management();
    assert_eq!(
        region.entity_state(TARGET).unwrap_err(),
        LevelError::MissingEntity(TARGET)
    );
}

#[test]
fn block_access_does_not_hide_missing_neighbors_or_overwrite_replacements() {
    let mut region = region();
    let missing = BlockPos::new(16, 64, 0);
    assert_eq!(
        region.get_block_state(missing),
        Err(LevelError::MissingChunk(ChunkPos::new(1, 0)))
    );
    region
        .set_block_state(PLACED, Block::STONE.default_state())
        .unwrap();
    region
        .schedule_block_tick(PLACED, Block::STONE, 1, TickPriority::Normal)
        .unwrap();
    region
        .set_block_state(PLACED, Block::DIRT.default_state())
        .unwrap();
    region.tick_scheduled_blocks(1, 65536).unwrap();
    assert_eq!(region.get_block_state(PLACED).unwrap().block(), Block::DIRT);
}
