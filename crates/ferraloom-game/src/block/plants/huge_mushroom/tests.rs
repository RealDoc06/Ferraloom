use crate::block::{
    BlockMethod, DirectionalPlaceContext, GameWorldgenBlockBehavior, LevelAccessRef,
};
use ferraloom_data::{Block, BlockState};
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::{
    BlockPos, Direction, Mirror, Rotation,
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

fn replay(text: &str, access: &mut LevelAccessRef<'_, '_>) {
    let pos = BlockPos::new(8, 5, 8);
    let point = |p: BlockPos| [p.x, p.y, p.z];
    let stack = ItemStack::default();
    let mut counts = [0; 3];
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let state = || BlockState::from_id(f[1].parse().unwrap()).unwrap();
        match f[0] {
            "update" => {
                let state = state();
                let neighbor = BlockState::from_id(f[2].parse().unwrap()).unwrap();
                let direction = Direction::VALUES[f[3].parse::<usize>().unwrap()];
                let at = pos.relative(direction, 1);
                put(access, pos, state);
                *access.random().unwrap() = Random::Legacy(LegacyRandomSource::new(1234));
                let actual = match access {
                    LevelAccessRef::Worldgen(region) => GameWorldgenBlockBehavior
                        .update_shape(region, point(pos), direction, point(at), neighbor)
                        .unwrap(),
                    LevelAccessRef::Server(_) => access
                        .as_reader()
                        .required_behavior(state.block(), BlockMethod::UpdateShape)
                        .unwrap()
                        .update_shape(state, pos, direction, at, neighbor, access)
                        .unwrap(),
                };
                assert_eq!(actual.id(), f[4].parse::<u16>().unwrap(), "{line}");
                assert_eq!(
                    access.random().unwrap().next_long(),
                    f[5].parse::<i64>().unwrap(),
                    "{line}"
                );
                counts[0] += 1;
            }
            "place" => {
                let block = Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
                let blocks = [
                    Block::BROWN_MUSHROOM_BLOCK,
                    Block::RED_MUSHROOM_BLOCK,
                    Block::MUSHROOM_STEM,
                ];
                let other =
                    blocks[(blocks.iter().position(|&b| b == block).unwrap() + 1) % blocks.len()];
                let mask: u8 = f[2].parse().unwrap();
                for (i, direction) in Direction::VALUES.into_iter().enumerate() {
                    let neighbor = if mask & (1 << i) != 0 {
                        block
                    } else if f[3] == "0" {
                        Block::AIR
                    } else {
                        other
                    };
                    put(access, pos.relative(direction, 1), neighbor.default_state());
                }
                put(access, pos, Block::AIR.default_state());
                *access.random().unwrap() = Random::Legacy(LegacyRandomSource::new(1234));
                let context = DirectionalPlaceContext::new(
                    &access.as_reader(),
                    pos,
                    Direction::VALUES[f[4].parse::<usize>().unwrap()],
                    &stack,
                    Direction::Up,
                )
                .unwrap();
                let actual = access
                    .get_state_for_placement(block, &context)
                    .unwrap()
                    .unwrap();
                assert_eq!(actual.id(), f[5].parse::<u16>().unwrap(), "{line}");
                let shaped = match access {
                    LevelAccessRef::Worldgen(region) => GameWorldgenBlockBehavior
                        .update_from_neighbor_shapes(region, point(pos), block.default_state())
                        .unwrap(),
                    LevelAccessRef::Server(_) => access
                        .update_from_neighbor_shapes(block.default_state(), pos)
                        .unwrap(),
                };
                assert_eq!(shaped, actual, "neighbor sweep {line}");
                assert_eq!(
                    access.random().unwrap().next_long(),
                    f[6].parse::<i64>().unwrap(),
                    "{line}"
                );
                counts[1] += 1;
            }
            "transform" => {
                let state = state();
                let actual = Rotation::VALUES.map(|r| state.rotate(r)).into_iter().chain(
                    [Mirror::None, Mirror::LeftRight, Mirror::FrontBack].map(|m| state.mirror(m)),
                );
                for (actual, expected) in actual.zip(&f[2..]) {
                    assert_eq!(actual.id(), expected.parse::<u16>().unwrap(), "{line}");
                }
                counts[2] += 1;
            }
            _ => panic!("unknown native vector: {line}"),
        }
    }
    assert_eq!(counts, [4608, 2304, 192]);
}

#[test]
fn native_huge_mushroom_virtuals_match_live_and_worldgen() {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/blocks/plants/huge-mushrooms.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let mut live = super::super::tests::level_with_height(2);
    replay(&text, &mut LevelAccessRef::Server(&mut live));
    assert!(live.take_effects().is_empty());
    let mut saved = ferraloom_storage::SerializableChunkData::default();
    live.save_chunk_ticks(ferraloom_util::ChunkPos::new(0, 0), &mut saved)
        .unwrap();
    assert!(saved.block_ticks.unwrap_or_default().is_empty());
    assert!(saved.fluid_ticks.unwrap_or_default().is_empty());

    let (_, generator) = super::super::worldgen_tests::setup();
    let mut chunks = [RegionChunk::new(
        generator.new_chunk([0, 0]),
        ChunkStatus::Features,
    )];
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
    region.set_random(Random::Legacy(LegacyRandomSource::new(1234)));
    replay(&text, &mut LevelAccessRef::Worldgen(&mut region));
    drop(region);
    assert!(chunks[0].block_ticks.ticks().is_empty());
    assert!(chunks[0].fluid_ticks.ticks().is_empty());
}
