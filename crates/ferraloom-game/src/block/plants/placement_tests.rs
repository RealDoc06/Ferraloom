use crate::block::{DirectionalPlaceContext, GameWorldgenBlockBehavior, LevelAccessRef};
use ferraloom_data::{Block, BlockState};
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::{
    BlockPos, Direction,
    random::{LegacyRandomSource, Random, RandomSource},
};
use ferraloom_worldgen::region::{ChunkStatus, RegionChunk, WorldGenRegion, WorldgenBlockBehavior};
use std::io::Read;

fn put(access: &mut LevelAccessRef<'_, '_>, pos: BlockPos, state: BlockState) {
    match access {
        LevelAccessRef::Server(level) => {
            level.set_block_state(pos, state).unwrap();
        }
        LevelAccessRef::Worldgen(region) => {
            region.set_block([pos.x, pos.y, pos.z], state, 816).unwrap();
        }
    }
}

fn replay(text: &str, access: &mut LevelAccessRef<'_, '_>) -> [usize; 3] {
    let pos = BlockPos::new(8, 5, 8);
    let stack = ItemStack::default();
    let mut counts = [0; 3];
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let block = || Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
        if f[0] == "survive" {
            let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
            for face in Direction::VALUES {
                put(access, pos.relative(face, 1), Block::AIR.default_state());
            }
            put(
                access,
                pos.relative(Direction::VALUES[f[3].parse::<usize>().unwrap()], 1),
                BlockState::from_id(f[2].parse().unwrap()).unwrap(),
            );
            *access.random().unwrap() = Random::Legacy(LegacyRandomSource::new(1234));
            let actual = match access {
                LevelAccessRef::Server(_) => access.as_reader().can_survive(state, pos).unwrap(),
                LevelAccessRef::Worldgen(region) => GameWorldgenBlockBehavior
                    .can_survive(state, region, [pos.x, pos.y, pos.z])
                    .unwrap(),
            };
            assert_eq!(actual, f[4].parse::<bool>().unwrap(), "{line}");
            assert_eq!(
                access.random().unwrap().next_long(),
                f[5].parse::<i64>().unwrap(),
                "{line}"
            );
            counts[0] += 1;
            continue;
        }
        let clicked = if f[0] == "wall" {
            let mask: u8 = f[2].parse().unwrap();
            let support =
                [Block::JUNGLE_LOG, Block::STONE, Block::OAK_SLAB][f[3].parse::<usize>().unwrap()];
            for (i, direction) in [
                Direction::North,
                Direction::East,
                Direction::South,
                Direction::West,
            ]
            .into_iter()
            .enumerate()
            {
                put(
                    access,
                    pos.relative(direction, 1),
                    if mask & (1 << i) != 0 {
                        support
                    } else {
                        Block::AIR
                    }
                    .default_state(),
                );
            }
            put(access, pos, Block::AIR.default_state());
            counts[1] += 1;
            4
        } else {
            assert_eq!(f[0], "wet");
            put(
                access,
                pos,
                BlockState::from_id(f[2].parse().unwrap()).unwrap(),
            );
            counts[2] += 1;
            3
        };
        *access.random().unwrap() = Random::Legacy(LegacyRandomSource::new(1234));
        let context = DirectionalPlaceContext::new(
            &access.as_reader(),
            pos,
            Direction::VALUES[f[clicked].parse::<usize>().unwrap()],
            &stack,
            Direction::Up,
        )
        .unwrap();
        let actual = access
            .get_state_for_placement(block(), &context)
            .unwrap()
            .map_or(-1, |state| i32::from(state.id()));
        assert_eq!(actual, f[clicked + 1].parse::<i32>().unwrap(), "{line}");
        assert_eq!(
            access.random().unwrap().next_long(),
            f[clicked + 2].parse::<i64>().unwrap(),
            "{line}"
        );
    }
    counts
}

#[test]
fn native_wall_plant_and_sculk_placement_matches_both_level_arms() {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/plant-placement.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (blocks, fluids) = super::tests::tags();
    let mut live = super::tests::level_with_height(2);
    live.set_block_tags(blocks.clone());
    live.set_fluid_tags(fluids.clone());
    let counts = replay(&text, &mut LevelAccessRef::Server(&mut live));
    assert!(live.take_effects().is_empty());

    let (_, generator) = super::worldgen_tests::setup();
    let mut chunks = [RegionChunk::new(
        generator.new_chunk([0, 0]),
        ChunkStatus::Features,
    )];
    let registry_context = crate::level::LevelContext::fixture(0, 63);
    let mut region = WorldGenRegion::new(
        &mut chunks,
        [0, 0],
        &[ChunkStatus::Features],
        0,
        None,
        &GameWorldgenBlockBehavior,
        0,
        63,
    )
    .unwrap();
    region.set_registry_context(registry_context.registry_context().unwrap());
    region.set_block_tags(blocks);
    region.set_fluid_tags(fluids);
    region.set_random(Random::Legacy(LegacyRandomSource::new(1234)));
    assert_eq!(
        replay(&text, &mut LevelAccessRef::Worldgen(&mut region)),
        counts
    );
    assert_eq!(counts, [21_360, 576, 816]);
}
