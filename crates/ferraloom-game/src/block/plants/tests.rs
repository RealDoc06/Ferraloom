use super::*;
use crate::{
    Behaviors,
    block::{BlockBehavior, LevelAccessRef, LevelReaderRef},
    level::{Level, Region, chunk::LevelChunk, effects::LevelEffect},
};
use ferraloom_data::{BlockState, properties as p};
use ferraloom_util::{BlockPos, ChunkPos, Direction};
use std::sync::Arc;
static EMPTY: Behaviors = Behaviors::EMPTY;
fn level() -> Level {
    let mut level = Level::new(Region::new(&EMPTY));
    level
        .add_chunk(LevelChunk::new(
            ChunkPos::new(0, 0),
            0..2,
            ferraloom_chunk::LevelChunkSection::new(
                ferraloom_chunk::Strategy::block_states(),
                ferraloom_chunk::Strategy::biomes(Arc::new(
                    ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
                )),
                ferraloom_chunk::BiomeId(0),
            ),
        ))
        .unwrap();
    level
}
#[test]
fn upper_double_plant_requires_its_own_lower_half() {
    let mut level = level();
    let pos = BlockPos::new(8, 5, 8);
    let upper = Block::TALL_GRASS
        .default_state()
        .with(p::HALF, p::Half::Upper)
        .unwrap();
    for (below, expected) in [
        (Block::TALL_GRASS.default_state(), true),
        (upper, false),
        (Block::LARGE_FERN.default_state(), false),
        (Block::STONE.default_state(), false),
    ] {
        level.set_block_state(pos.below(1), below).unwrap();
        assert_eq!(
            survival::PLANT
                .can_survive(upper, pos, &LevelReaderRef::Server(&level))
                .unwrap(),
            expected,
            "{below:?}"
        );
    }
}
#[test]
fn multiface_neighbor_loss_removes_only_the_unsupported_face() {
    let mut level = level();
    let pos = BlockPos::new(8, 5, 8);
    let north =
        multiface::with_face(Block::GLOW_LICHEN.default_state(), Direction::North, true).unwrap();
    let both = multiface::with_face(north, Direction::South, true).unwrap();
    for (input, expected) in [
        (
            both,
            multiface::with_face(both, Direction::North, false).unwrap(),
        ),
        (north, Block::AIR.default_state()),
    ] {
        let output = multiface::MULTIFACE
            .update_shape(
                input,
                pos,
                Direction::North,
                pos.north(1),
                Block::AIR.default_state(),
                &mut LevelAccessRef::Server(&mut level),
            )
            .unwrap();
        assert_eq!(output, expected);
    }
}
#[test]
fn leaf_decay_gates_preserve_persistent_and_connected_leaves() {
    let mut level = level();
    let pos = BlockPos::new(8, 5, 8);
    let decaying = Block::OAK_LEAVES
        .default_state()
        .with(p::WATERLOGGED, true)
        .unwrap();
    // Isolate this callback: vanilla removeBlock delegates fluid replacement to setBlock.
    for state in [
        decaying.with(p::PERSISTENT, true).unwrap(),
        decaying.with(p::DISTANCE, 6).unwrap(),
    ] {
        level.set_block_state(pos, state).unwrap();
        leaves::LEAVES.random_tick(state, pos, &mut level).unwrap();
        assert_eq!(level.get_block_state(pos).unwrap(), state);
    }
    // Dry leaves exercise the same no-particle removal without starting fluid flow.
    let dry = decaying.with(p::WATERLOGGED, false).unwrap();
    level.set_block_state(pos, dry).unwrap();
    leaves::LEAVES.random_tick(dry, pos, &mut level).unwrap();
    assert_eq!(
        level.get_block_state(pos).unwrap(),
        Block::AIR.default_state()
    );
    let effects = level.take_effects();
    assert!(
        effects
            .iter()
            .any(|e| matches!(e,LevelEffect::DropResources{state,..} if *state==dry))
    );
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, LevelEffect::DestroyParticles { .. }))
    );
}
#[test]
fn shared_raw_brightness_requires_light_context() {
    let level = level();
    assert!(
        LevelReaderRef::Server(&level)
            .get_raw_brightness(BlockPos::new(8, 5, 8), 0)
            .is_err()
    );
}

fn tags() -> (
    Arc<ferraloom_data::datapack::TagSet<Block>>,
    Arc<ferraloom_data::datapack::TagSet<ferraloom_data::registries::fluid::Id>>,
) {
    use ferraloom_data::datapack::{Pack, ResourceManager, TagSet, load_tags};
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies vanilla jar");
    let resources =
        ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
    let blocks = load_tags(&resources, "block", |id, _| {
        Block::from_name(&id.to_string())
    });
    let fluids = load_tags(&resources, "fluid", |id, _| {
        ferraloom_data::registries::fluid::Id::from_name(&id.to_string())
    });
    assert!(blocks.diagnostics.is_empty());
    assert!(fluids.diagnostics.is_empty());
    (
        Arc::new(TagSet::from_tags(blocks, ferraloom_data::BLOCKS.len()).unwrap()),
        Arc::new(TagSet::from_tags(fluids, ferraloom_data::registries::fluid::ALL.len()).unwrap()),
    )
}
#[test]
fn survival_and_six_direction_shape_updates_match_vanilla_levels() {
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/survival.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (blocks, fluids) = tags();
    let pos = BlockPos::new(8, 5, 8);
    let chunk = ChunkPos::new(0, 0);
    let mut count = 0;
    for line in text.lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
        let support = BlockState::from_id(fields[2].parse().unwrap()).unwrap();
        let light = fields[3].parse().unwrap();
        let mut level = level();
        level.set_block_tags(blocks.clone());
        level.set_fluid_tags(fluids.clone());
        level.set_light_snapshot(
            chunk,
            crate::level::weather::ChunkLightSnapshot {
                sky: [(0, ferraloom_light::DataLayer::new(light))].into(),
                block: Default::default(),
            },
        );
        level.set_random_streams(Random::Legacy(LegacyRandomSource::new(1234)), 0);
        level.set_block_state(pos, state).unwrap();
        level.set_block_state(pos.below(1), support).unwrap();
        if fields[0] == "survive" {
            assert_eq!(
                level.can_survive(state, pos).unwrap(),
                fields[4] == "true",
                "{line}"
            );
        } else {
            let direction = Direction::VALUES[fields[4].parse::<usize>().unwrap()];
            let neighbor_pos = pos.relative(direction, 1);
            let neighbor = level.get_block_state(neighbor_pos).unwrap();
            let behavior = level
                .required_block_behavior(state.block(), BlockMethod::UpdateShape)
                .unwrap();
            let next = behavior
                .update_shape(
                    state,
                    pos,
                    direction,
                    neighbor_pos,
                    neighbor,
                    &mut LevelAccessRef::Server(&mut level),
                )
                .unwrap();
            assert_eq!(next.id(), fields[5].parse::<u16>().unwrap(), "{line}");
            assert_eq!(
                level.random().next_long(),
                fields[6].parse::<i64>().unwrap(),
                "RNG {line}"
            );
            let mut saved = ferraloom_storage::SerializableChunkData::default();
            level.save_chunk_ticks(chunk, &mut saved).unwrap();
            let mut ticks = Vec::new();
            for tick in saved.block_ticks.unwrap() {
                ticks.push(format!("schedule:{}", tick.delay.unwrap()));
            }
            for tick in saved.fluid_ticks.unwrap() {
                ticks.push(format!("fluid:{}", tick.delay.unwrap()));
            }
            let mut expected = fields[7]
                .split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            ticks.sort();
            expected.sort();
            assert_eq!(ticks, expected, "ticks {line}");
        }
        count += 1;
    }
    assert_eq!(count, 75040);
}

#[test]
fn crop_bonemeal_and_growth_speed_match_real_vanilla_methods() {
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/growth.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (blocks, fluids) = tags();
    let pos = BlockPos::new(8, 5, 8);
    let chunk = ChunkPos::new(0, 0);
    let mut count = 0;
    for line in text.lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let mut level = level();
        level.set_block_tags(blocks.clone());
        level.set_fluid_tags(fluids.clone());
        level.set_light_snapshot(
            chunk,
            crate::level::weather::ChunkLightSnapshot {
                sky: [(0, ferraloom_light::DataLayer::new(15))].into(),
                block: Default::default(),
            },
        );
        if f[0] == "grow" {
            level.set_random_streams(
                Random::Legacy(LegacyRandomSource::new(f[2].parse().unwrap())),
                0,
            );
            level.set_block_state(pos, state).unwrap();
            level
                .set_block_state(pos.below(1), Block::FARMLAND.default_state())
                .unwrap();
            crops::grow_crops(state, pos, &mut LevelAccessRef::Server(&mut level)).unwrap();
            assert_eq!(
                level.get_block_state(pos).unwrap().id(),
                f[3].parse::<u16>().unwrap(),
                "{line}"
            );
            assert_eq!(
                level.random().next_long(),
                f[4].parse::<i64>().unwrap(),
                "RNG {line}"
            );
        } else {
            let moisture = f[2].parse().unwrap();
            let pattern = f[3].parse::<u8>().unwrap();
            let soil = Block::FARMLAND
                .default_state()
                .with(p::MOISTURE, moisture)
                .unwrap();
            for x in -1..=1 {
                for z in -1..=1 {
                    level
                        .set_block_state(pos.below(1).offset(x, 0, z), soil)
                        .unwrap();
                }
            }
            for (bit, at) in [
                (1, pos.west(1)),
                (2, pos.north(1)),
                (4, pos.west(1).north(1)),
                (8, pos.east(1).south(1)),
            ] {
                if pattern & bit != 0 {
                    level.set_block_state(at, state).unwrap();
                }
            }
            assert_eq!(
                crops::growth_speed(state.block(), pos, &LevelReaderRef::Server(&level))
                    .unwrap()
                    .to_bits(),
                f[4].parse::<u32>().unwrap(),
                "{line}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 592);
}

#[test]
fn snow_melts_only_above_eleven_block_light_and_emits_destroy_context() {
    for light in [0, 11, 12, 15] {
        let mut level = level();
        let pos = BlockPos::new(8, 5, 8);
        let state = Block::SNOW.default_state();
        level
            .set_block_state(pos.below(1), Block::STONE.default_state())
            .unwrap();
        level.set_block_state(pos, state).unwrap();
        level.set_light_snapshot(
            ChunkPos::new(0, 0),
            crate::level::weather::ChunkLightSnapshot {
                sky: [(0, ferraloom_light::DataLayer::new(15))].into(),
                block: [(0, ferraloom_light::DataLayer::new(light))].into(),
            },
        );
        snow::SNOW.random_tick(state, pos, &mut level).unwrap();
        assert_eq!(
            level.get_block_state(pos).unwrap(),
            if light > 11 {
                Block::AIR.default_state()
            } else {
                state
            }
        );
        let effects = level.take_effects();
        assert_eq!(effects.iter().filter(|e|matches!(e,LevelEffect::GameEvent{event:ferraloom_data::registries::game_event::Id::BLOCK_DESTROY,affected_state:Some(affected),..} if *affected==state)).count(),usize::from(light>11));
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, LevelEffect::DestroyParticles { .. }))
        );
    }
}
