use crate::{
    Behaviors,
    block::{BlockMethod, LevelAccessRef},
    level::{Level, LevelContext, LevelError, Region, chunk::LevelChunk},
};
use ferraloom_data::{
    Block, BlockState,
    datapack::{Pack, ResourceManager, TagSet},
    properties as p,
};
use ferraloom_util::{
    BlockPos, ChunkPos, Direction,
    random::{LegacyRandomSource, Random, RandomSource},
};
use std::{collections::BTreeSet, io::Read, sync::Arc};

static BEHAVIORS: Behaviors = Behaviors::EMPTY;
const POS: BlockPos = BlockPos::new(8, 5, 8);

fn level() -> Level {
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    let biome = ferraloom_chunk::BiomeId(0);
    level
        .add_chunk(LevelChunk::new(
            ChunkPos::new(0, 0),
            0..1,
            ferraloom_chunk::LevelChunkSection::new(
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

/// Every ordinary/trapped/copper variant uses the required virtual, including
/// the six-side alias which re-dispatches copper's intermediate block changes.
#[test]
fn native_chest_neighbor_shapes_connections_water_and_copper_variants_match() {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies vanilla jar");
    let resources =
        ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
    let tags = Arc::new(TagSet::<Block>::load(&resources).unwrap());
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/blocks/entities/chest-shapes.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let mut counts = [0; 2];
    let mut blocks = BTreeSet::new();
    for line in text.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
        let side: i32 = fields[2].parse().unwrap();
        let neighbors: Vec<_> = fields[3]
            .split(',')
            .map(|id| BlockState::from_id(id.parse().unwrap()).unwrap())
            .collect();
        let mut level = level();
        level.set_block_tags(tags.clone());
        level.set_random_streams(Random::Legacy(LegacyRandomSource::new(263777)), 0);
        let result = if side < 0 {
            for (direction, neighbor) in Direction::VALUES.into_iter().zip(neighbors) {
                level
                    .set_block_state(POS.relative(direction, 1), neighbor)
                    .unwrap();
            }
            counts[1] += 1;
            LevelAccessRef::Server(&mut level)
                .update_from_neighbor_shapes(state, POS)
                .unwrap_or_else(|error| panic!("{line}: {error:?}"))
        } else {
            let direction = Direction::VALUES[side as usize];
            let behavior = level
                .required_block_behavior(state.block(), BlockMethod::UpdateShape)
                .unwrap();
            counts[0] += 1;
            behavior
                .update_shape(
                    state,
                    POS,
                    direction,
                    POS.relative(direction, 1),
                    neighbors[0],
                    &mut LevelAccessRef::Server(&mut level),
                )
                .unwrap_or_else(|error| panic!("{line}: {error:?}"))
        };
        assert_eq!(result.id(), fields[4].parse::<u16>().unwrap(), "{line}");
        assert_eq!(
            level.random().next_int(),
            fields[6].parse::<i32>().unwrap(),
            "RNG {line}"
        );
        let mut saved = ferraloom_storage::SerializableChunkData::default();
        level
            .save_chunk_ticks(ChunkPos::new(0, 0), &mut saved)
            .unwrap();
        let ticks = saved.fluid_ticks.unwrap();
        let delay: i32 = fields[5].parse().unwrap();
        if delay < 0 {
            assert!(ticks.is_empty(), "{line}: {ticks:?}");
        } else {
            assert_eq!(ticks.len(), 1, "{line}");
            let tick = &ticks[0];
            assert_eq!(
                tick.id.as_ref().unwrap().to_string_checked().unwrap(),
                "minecraft:water"
            );
            assert_eq!(
                [tick.x.unwrap(), tick.y.unwrap(), tick.z.unwrap()],
                [POS.x, POS.y, POS.z]
            );
            assert_eq!(tick.delay, Some(delay), "{line}");
            assert_eq!(tick.priority, Some(0), "{line}");
        }
        blocks.insert(state.block());
    }
    assert_eq!(counts, [63_360, 23_040]);
    assert_eq!(blocks.len(), 10);
}

#[test]
fn water_tick_precedes_the_required_copper_connection_tag_query() {
    let mut level = level();
    let state = Block::COPPER_CHEST
        .default_state()
        .with(p::WATERLOGGED, true)
        .unwrap();
    let behavior = level
        .required_block_behavior(state.block(), BlockMethod::UpdateShape)
        .unwrap();
    assert_eq!(
        behavior.update_shape(
            state,
            POS,
            Direction::Up,
            POS.above(1),
            Block::AIR.default_state(),
            &mut LevelAccessRef::Server(&mut level)
        ),
        Err(LevelError::MissingBlockTags)
    );
    assert!(level.has_scheduled_fluid_tick(POS, ferraloom_data::registries::fluid::Id::WATER));
}
