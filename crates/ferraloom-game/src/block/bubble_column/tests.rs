use super::*;
use crate::{
    Behaviors,
    block::UnportedBehavior,
    entity::{EntityState, item::ItemEntity},
    level::{LevelContext, Region, chunk::LevelChunk, distance_manager::FullChunkStatus},
};
use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
use ferraloom_data::datapack::{Pack, ResourceManager, TagSet};
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::{ChunkPos, Vec3, random::RandomSource};
use std::{
    path::Path,
    sync::{Arc, OnceLock},
};

static EMPTY: Behaviors = Behaviors::EMPTY;
const POS: BlockPos = BlockPos::new(8, 64, 8);
const VECTORS: &str = include_str!("../../../tests/java/level/bubble-column.tsv");

fn level() -> Level {
    struct Tags {
        blocks: Arc<TagSet<Block>>,
        fluids: Arc<TagSet<Fluid>>,
    }
    static TAGS: OnceLock<Tags> = OnceLock::new();
    let tags = TAGS.get_or_init(|| {
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
        let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
        Tags {
            blocks: Arc::new(TagSet::load(&resources).unwrap()),
            fluids: Arc::new(TagSet::load(&resources).unwrap()),
        }
    });
    let mut level = Level::new(Region::new(&EMPTY), LevelContext::fixture(0, 63));
    level.set_block_tags(tags.blocks.clone());
    level.set_fluid_tags(tags.fluids.clone());
    for x in -1..=1 {
        for z in -1..=1 {
            let pos = ChunkPos::new(x, z);
            let section = LevelChunkSection::new(
                Strategy::block_states(),
                Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
                BiomeId(0),
            );
            level
                .add_chunk(LevelChunk::new(pos, -4..20, section))
                .unwrap();
            level
                .set_chunk_loading_status(pos, FullChunkStatus::BlockTicking, true)
                .unwrap();
            level.start_ticking_chunk(pos).unwrap();
        }
    }
    level
}
fn state(id: &str) -> BlockState {
    BlockState::from_id(id.parse().unwrap()).unwrap()
}
fn bubble(drag: bool) -> BlockState {
    Block::BUBBLE_COLUMN
        .default_state()
        .with(properties::DRAG, drag)
        .unwrap()
}
fn raw(level: &mut Level, pos: BlockPos, state: BlockState) {
    level.set_block_state(pos, state).unwrap();
}
fn rows(kind: &str) -> impl Iterator<Item = Vec<&'static str>> + '_ {
    VECTORS
        .lines()
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .filter(move |f| f[0] == kind)
}
fn bits(value: f64) -> String {
    format!("{:x}", value.to_bits())
}
fn saved_ticks(level: &Level) -> ferraloom_storage::SerializableChunkData {
    let mut data = ferraloom_storage::SerializableChunkData::default();
    level
        .save_chunk_ticks(ChunkPos::containing(POS), &mut data)
        .unwrap();
    data
}

#[test]
fn native_survival_and_shape_schedule_water_first_with_five_tick_delays() {
    let mut count = (0, 0);
    for f in rows("survive") {
        let mut level = level();
        raw(&mut level, POS.below(1), state(f[2]));
        let state = bubble(f[1].parse().unwrap());
        assert_eq!(
            LevelReaderRef::Server(&level)
                .required_behavior(state.block(), BlockMethod::CanSurvive)
                .unwrap()
                .can_survive(state, POS, &LevelReaderRef::Server(&level))
                .unwrap(),
            f[3] == "true",
            "{f:?}"
        );
        count.0 += 1;
    }
    for f in rows("shape") {
        let mut level = level();
        raw(&mut level, POS.below(1), state(f[2]));
        let state = bubble(f[1].parse().unwrap());
        let direction = Direction::from_3d_data_value(f[3].parse().unwrap());
        let behavior = LevelReaderRef::Server(&level)
            .required_behavior(state.block(), BlockMethod::UpdateShape)
            .unwrap();
        let result = behavior
            .update_shape(
                state,
                POS,
                direction,
                POS.relative(direction, 1),
                self::state(f[4]),
                &mut LevelAccessRef::Server(&mut level),
            )
            .unwrap();
        assert_eq!(result, self::state(f[5]), "{f:?}");
        let saved = saved_ticks(&level);
        let fluid = saved.fluid_ticks.unwrap();
        assert_eq!(fluid.len(), 1, "{f:?}");
        assert_eq!(fluid[0].delay, Some(5), "{f:?}");
        assert!(level.has_scheduled_fluid_tick(POS, Fluid::WATER));
        let block = saved.block_ticks.unwrap();
        assert_eq!(block.len(), usize::from(f[6] == "fluid:5;block:5"), "{f:?}");
        if let Some(tick) = block.first() {
            assert_eq!(tick.delay, Some(5), "{f:?}");
        }
        count.1 += 1;
    }
    assert_eq!(count, (12, 504));
}

#[test]
fn native_column_and_liquid_ticks_preserve_unchanged_write_stops_and_rng() {
    let mut count = 0;
    for f in rows("column") {
        let mut level = level();
        let below = state(f[2]);
        raw(&mut level, POS.below(1), below);
        let tower: Vec<_> = f[3].split(',').map(state).collect();
        for (y, &state) in tower.iter().enumerate() {
            raw(&mut level, POS.above(y as i32), state);
        }
        level.random().set_seed(42);
        if f[1] == "true" {
            level
                .optional_block_call(tower[0].block(), BlockMethod::Tick, |behavior, level| {
                    behavior.tick(tower[0], POS, level)
                })
                .unwrap();
        } else {
            update_column(
                Block::BUBBLE_COLUMN,
                &mut LevelAccessRef::Server(&mut level),
                POS,
                tower[0],
                below,
            )
            .unwrap();
        }
        for (y, expected) in f[4].split(',').map(state).enumerate() {
            assert_eq!(
                level.get_block_state(POS.above(y as i32)).unwrap(),
                expected,
                "{f:?}"
            );
        }
        assert_eq!(
            level.random().next_long(),
            f[5].parse::<i64>().unwrap(),
            "{f:?}"
        );
        count += 1;
    }
    assert_eq!(count, 90);
}

#[test]
fn native_precise_contact_dispatches_original_position_shape_and_motion_hooks() {
    let mut count = 0;
    for f in rows("inside") {
        let mut level = level();
        raw(&mut level, POS.above(1), state(f[1]));
        let mut item = ItemEntity::new_at(
            &mut level,
            Vec3::new(8.5, 64.0, 8.5),
            ItemStack::new(Item::STONE, 1, Default::default()),
        );
        item.parent.velocity = Vec3::new(
            0.25,
            f64::from_bits(u64::from_str_radix(f[4], 16).unwrap()),
            -0.125,
        );
        item.parent.fall_distance = 8.0;
        item.parent.inside_block_is_precise = f[3].parse().unwrap();
        let entity = level.next_entity_ref();
        level
            .add_entity(entity, EntityState::Item(Box::new(item)))
            .unwrap();
        level.random().set_seed(42);
        level
            .call_entity_inside(bubble(f[2].parse().unwrap()), POS, entity)
            .unwrap();
        let base = level.entity::<EntityData>(entity).unwrap();
        assert_eq!(bits(base.velocity.y), f[5], "{f:?}");
        assert_eq!(bits(base.fall_distance), f[6], "{f:?}");
        assert_eq!(
            level.random().next_long(),
            f[7].parse::<i64>().unwrap(),
            "{f:?}"
        );
        count += 1;
    }
    assert_eq!(count, 96);
}

#[test]
fn native_pickup_returns_water_even_when_the_air_write_does_not_change_a_block() {
    let mut count = 0;
    for f in rows("pickup") {
        let mut level = level();
        let state = bubble(f[1].parse().unwrap());
        if f[2] == "true" {
            raw(&mut level, POS, state);
        }
        let behavior = LevelReaderRef::Server(&level)
            .required_behavior(state.block(), BlockMethod::PickupBlock)
            .unwrap();
        assert_eq!(
            behavior
                .pickup_block(state, POS, &mut LevelAccessRef::Server(&mut level))
                .unwrap()
                .unwrap()
                .name(),
            f[3]
        );
        assert_eq!(level.get_block_state(POS).unwrap(), self::state(f[5]));
        count += 1;
    }
    assert_eq!(count, 4);
}

#[test]
fn required_tags_and_chunk_reads_keep_native_short_circuits_and_water_schedule() {
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        let mut level = Level::new(Region::new(&EMPTY), LevelContext::fixture(0, 63));
        level.set_unported_behavior(policy);
        let entity = level.next_entity_ref();
        let item = ItemEntity::new_at(
            &mut level,
            Vec3::new(8.5, 64.0, 8.5),
            ItemStack::new(Item::STONE, 1, Default::default()),
        );
        level
            .add_entity(entity, EntityState::Item(Box::new(item)))
            .unwrap();
        level.call_entity_inside(bubble(true), POS, entity).unwrap();
        level
            .entity_mut::<EntityData>(entity)
            .unwrap()
            .inside_block_is_precise = true;
        assert_eq!(
            level.call_entity_inside(bubble(true), POS, entity),
            Err(LevelError::MissingChunk(ChunkPos::containing(POS)))
        );
        assert_eq!(
            BUBBLE_COLUMN.can_survive(bubble(true), POS, &LevelReaderRef::Server(&level)),
            Err(LevelError::MissingChunk(ChunkPos::containing(POS)))
        );
        let section = LevelChunkSection::new(
            Strategy::block_states(),
            Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
            BiomeId(0),
        );
        level
            .add_chunk(LevelChunk::new(ChunkPos::containing(POS), -4..20, section))
            .unwrap();
        raw(&mut level, POS.below(1), bubble(false));
        assert!(
            BUBBLE_COLUMN
                .can_survive(bubble(true), POS, &LevelReaderRef::Server(&level))
                .unwrap()
        );
        assert!(
            can_occupy(
                Block::BUBBLE_COLUMN,
                bubble(true),
                &LevelReaderRef::Server(&level)
            )
            .unwrap()
        );
        raw(&mut level, POS.below(1), Block::STONE.default_state());
        assert_eq!(
            BUBBLE_COLUMN.update_shape(
                bubble(true),
                POS,
                Direction::Down,
                POS.below(1),
                Block::STONE.default_state(),
                &mut LevelAccessRef::Server(&mut level)
            ),
            Err(LevelError::MissingBlockTags)
        );
        assert!(level.has_scheduled_fluid_tick(POS, Fluid::WATER));
        assert!(!level.has_scheduled_block_tick(POS, Block::BUBBLE_COLUMN));
        assert_eq!(
            can_occupy(
                Block::BUBBLE_COLUMN,
                Block::WATER.default_state(),
                &LevelReaderRef::Server(&level)
            ),
            Err(LevelError::MissingFluidTags)
        );
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
    }
}
