use crate::{
    Behaviors,
    block::{BlockMethod, DirectionalPlaceContext, LevelAccessRef, LevelReaderRef},
    level::{
        Level, LevelContext, Region,
        chunk::{LevelChunk, LevelChunkSection},
    },
};
use ferraloom_data::{
    Block, BlockState,
    datapack::{Pack, ResourceManager, TagSet},
};
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::{BlockPos, ChunkPos, Direction};
use std::{
    collections::BTreeSet,
    io::Read,
    sync::{Arc, LazyLock},
};

pub(super) const POS: BlockPos = BlockPos::new(8, 64, 8);
static EMPTY: Behaviors = Behaviors::EMPTY;

pub(super) fn level() -> Level {
    let mut level = Level::new(Region::new(&EMPTY), LevelContext::fixture(0, 63));
    let biome = ferraloom_chunk::BiomeId(0);
    level
        .add_chunk(LevelChunk::new(
            ChunkPos::new(0, 0),
            -4..20,
            LevelChunkSection::new(
                ferraloom_chunk::Strategy::block_states(),
                ferraloom_chunk::Strategy::biomes(Arc::new(
                    ferraloom_chunk::IdMap::new(vec![biome]).unwrap(),
                )),
                biome,
            ),
        ))
        .unwrap();
    level
}
/// The liquid-placement path executes real setBlock neighbor/output queries.
/// Supply the same live vanilla tags as a production level, without a fallback.
pub(super) fn tagged_level() -> Level {
    static TAGS: LazyLock<Arc<TagSet<Block>>> = LazyLock::new(|| {
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies native tags");
        let resources =
            ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
        Arc::new(TagSet::load(&resources).unwrap())
    });
    let mut level = level();
    level.set_block_tags(Arc::clone(&TAGS));
    level
}
fn orientation_vectors() -> String {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/building/orientation.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
}

#[test]
fn orientation_placements_match_every_native_variant_face_and_replaced_fluid() {
    let text = orientation_vectors();
    let mut level = level();
    let item = ItemStack::default();
    let mut blocks = BTreeSet::new();
    let mut count = 0;
    for line in text.lines().filter(|line| line.starts_with("place\t")) {
        let f: Vec<_> = line.split('\t').collect();
        let block = Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
        let direction = Direction::VALUES[f[2].parse::<usize>().unwrap()];
        let face = Direction::VALUES[f[3].parse::<usize>().unwrap()];
        let old = BlockState::from_id(f[4].parse().unwrap()).unwrap();
        level.set_block_state(POS, old).unwrap();
        let context = DirectionalPlaceContext::new(
            &LevelReaderRef::Server(&level),
            POS,
            direction,
            &item,
            face,
        )
        .unwrap();
        let state = level.get_state_for_placement(block, &context).unwrap();
        assert_eq!(
            state.map_or(-1, |state| i32::from(state.id())),
            f[5].parse::<i32>().unwrap(),
            "{line}"
        );
        blocks.insert(block);
        count += 1;
    }
    assert_eq!(blocks.len(), 101);
    assert_eq!(count, 39996);
}

#[test]
fn orientation_shape_updates_preserve_native_water_and_falling_tick_schedules() {
    let text = orientation_vectors();
    let mut count = 0;
    for line in text.lines().filter(|line| line.starts_with("update\t")) {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let direction = Direction::VALUES[f[2].parse::<usize>().unwrap()];
        let neighbor = BlockState::from_id(f[3].parse().unwrap()).unwrap();
        let at = POS.relative(direction, 1);
        let mut level = level();
        level.set_block_state(POS, state).unwrap();
        level.set_block_state(at, neighbor).unwrap();
        let behavior = level
            .required_block_behavior(state.block(), BlockMethod::UpdateShape)
            .unwrap();
        let result = behavior
            .update_shape(
                state,
                POS,
                direction,
                at,
                neighbor,
                &mut LevelAccessRef::Server(&mut level),
            )
            .unwrap();
        assert_eq!(result.id(), f[4].parse::<u16>().unwrap(), "{line}");
        let mut saved = ferraloom_storage::SerializableChunkData::default();
        level
            .save_chunk_ticks(ChunkPos::new(0, 0), &mut saved)
            .unwrap();
        let mut ticks = saved
            .block_ticks
            .unwrap_or_default()
            .into_iter()
            .map(|tick| format!("schedule:{}", tick.delay.unwrap()))
            .collect::<Vec<_>>();
        ticks.extend(
            saved
                .fluid_ticks
                .unwrap_or_default()
                .into_iter()
                .map(|tick| format!("fluid:{}", tick.delay.unwrap())),
        );
        assert_eq!(ticks.join(","), f[5], "{line}");
        count += 1;
    }
    assert_eq!(count, 2016);
}
