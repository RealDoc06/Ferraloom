use crate::{
    Behaviors,
    block::BlockMethod,
    level::{
        Level, LevelContext, Region,
        chunk::{LevelChunk, LevelChunkSection},
        distance_manager::FullChunkStatus,
        effects::LevelEffect,
    },
};
use ferraloom_data::{
    Block, BlockState,
    datapack::{Pack, ResourceManager, TagSet},
    registries::fluid::Id as Fluid,
};
use ferraloom_util::{
    BlockPos, ChunkPos, Direction,
    random::{LegacyRandomSource, Random, RandomSource},
};
use std::{
    collections::BTreeMap,
    io::Read,
    sync::{Arc, LazyLock},
};

static VECTORS: LazyLock<String> = LazyLock::new(|| {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/building/amethyst.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
});
static EMPTY: Behaviors = Behaviors::EMPTY;
static TAGS: LazyLock<(Arc<TagSet<Block>>, Arc<TagSet<Fluid>>)> = LazyLock::new(|| {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies native tags");
    let resources =
        ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
    (
        Arc::new(TagSet::load(&resources).unwrap()),
        Arc::new(TagSet::load(&resources).unwrap()),
    )
});

fn level(origin: BlockPos) -> Level {
    let mut level = Level::new(Region::new(&EMPTY), LevelContext::fixture(0, 63));
    level.set_block_tags(Arc::clone(&TAGS.0));
    level.set_fluid_tags(Arc::clone(&TAGS.1));
    let biome = ferraloom_chunk::BiomeId(0);
    let section = LevelChunkSection::new(
        ferraloom_chunk::Strategy::block_states(),
        ferraloom_chunk::Strategy::biomes(Arc::new(
            ferraloom_chunk::IdMap::new(vec![biome]).unwrap(),
        )),
        biome,
    );
    let center = ChunkPos::containing(origin);
    for x in center.x - 1..=center.x + 1 {
        for z in center.z - 1..=center.z + 1 {
            let chunk = ChunkPos::new(x, z);
            level
                .add_chunk(LevelChunk::new(chunk, -4..20, section.clone()))
                .unwrap();
            level
                .set_chunk_loading_status(chunk, FullChunkStatus::BlockTicking, true)
                .unwrap();
        }
    }
    level
}

#[test]
fn budding_amethyst_growth_matches_native_states_write_order_and_rng() {
    let mut matched = 0;
    for line in VECTORS.lines().filter(|line| line.starts_with("grow\t")) {
        let f: Vec<_> = line.split('\t').collect();
        let xyz: Vec<i32> = f[1].split(',').map(|n| n.parse().unwrap()).collect();
        let origin = BlockPos::new(xyz[0], xyz[1], xyz[2]);
        let mut level = level(origin);
        let budding = Block::BUDDING_AMETHYST.default_state();
        let neighbor = BlockState::from_id(f[2].parse().unwrap()).unwrap();
        level.set_block_state(origin, budding).unwrap();
        for side in Direction::VALUES {
            level
                .set_block_state(origin.relative(side, 1), neighbor)
                .unwrap();
        }
        level.take_effects();
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(f[3].parse().unwrap())),
            0,
        );
        level
            .required_block_behavior(budding.block(), BlockMethod::RandomTick)
            .unwrap()
            .random_tick(budding, origin, &mut level)
            .unwrap();
        let states = Direction::VALUES
            .map(|side| {
                level
                    .get_block_state(origin.relative(side, 1))
                    .unwrap()
                    .id()
                    .to_string()
            })
            .join(",");
        assert_eq!(states, f[4], "{line}");
        // The native overload is setBlockAndUpdate (flags 3). Core flag tests
        // cover that overload; compare its ordered observable writes here.
        let native_writes: Vec<_> = f[5]
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| {
                let (write, flags) = s.rsplit_once('/').unwrap();
                assert_eq!(flags, "3", "{line}");
                write
            })
            .collect();
        let writes: Vec<_> = level
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                LevelEffect::BlockUpdated { pos, new, .. } => {
                    Some(format!("{},{},{}:{}", pos.x, pos.y, pos.z, new.id()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(writes, native_writes, "{line}");
        assert_eq!(level.random().next_long().to_string(), f[6], "{line}");
        matched += 1;
    }
    assert_eq!(matched, 11264);

    // Coordinator-requested routing audit uses the same compiled registry as
    // live random ticks. This is diagnostic output, not a claim of parity.
    let mut missing = BTreeMap::<String, std::collections::BTreeSet<&str>>::new();
    for state in ferraloom_data::BLOCK_STATES {
        if state.random_ticking {
            let block = state.block;
            if crate::block::dispatch::registered_behavior(block, BlockMethod::RandomTick).is_none()
            {
                missing
                    .entry(format!("{:?}", block.class()))
                    .or_default()
                    .insert(block.name());
            }
        }
    }
    for (class, blocks) in missing {
        println!(
            "RANDOM_TICK_GAP\t{class}\t{}",
            blocks.into_iter().collect::<Vec<_>>().join(",")
        );
    }
}

#[test]
fn amethyst_growth_acceptance_matches_every_native_registered_state() {
    let mut matched = 0;
    for line in VECTORS.lines().filter(|line| line.starts_with("accept\t")) {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        assert_eq!(
            super::amethyst::can_cluster_grow_at_state(state),
            f[2] == "true",
            "{line}"
        );
        matched += 1;
    }
    assert_eq!(matched, ferraloom_data::BLOCK_STATES.len());
}
