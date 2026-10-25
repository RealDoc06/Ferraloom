use crate::block::{BlockMethod, GameWorldgenBlockBehavior, LevelAccessRef, UnportedBehavior};
use ferraloom_data::{BlockState, registries::fluid::Id as Fluid};
use ferraloom_util::{
    BlockPos, ChunkPos, Direction,
    random::{LegacyRandomSource, Random, RandomSource},
};
use ferraloom_worldgen::region::{ChunkStatus, RegionChunk, WorldGenRegion, WorldgenBlockBehavior};
use std::io::Read;

const POS: BlockPos = BlockPos::new(8, 5, 8);
fn point(pos: BlockPos) -> [i32; 3] {
    [pos.x, pos.y, pos.z]
}
fn update(access: &mut LevelAccessRef<'_, '_>, fields: &[&str]) -> BlockState {
    let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
    let neighbor = BlockState::from_id(fields[2].parse().unwrap()).unwrap();
    let direction = fields[3].parse::<i32>().unwrap();
    for (pos, state) in std::iter::once((POS, state)).chain(
        Direction::VALUES
            .into_iter()
            .map(|d| (POS.relative(d, 1), neighbor)),
    ) {
        match access {
            LevelAccessRef::Server(level) => {
                level.set_block_state(pos, state).unwrap();
            }
            // Capture an already-existing feature state, without invoking BE
            // placement/lifecycle: these native shape methods never read a BE.
            LevelAccessRef::Worldgen(region) => {
                region.set_block_in_section(point(pos), state).unwrap()
            }
        }
    }
    *access.random().unwrap() = Random::Legacy(LegacyRandomSource::new(1234));
    let actual = if direction < 0 {
        match access {
            LevelAccessRef::Worldgen(region) => GameWorldgenBlockBehavior
                .update_from_neighbor_shapes(region, point(POS), state)
                .unwrap(),
            LevelAccessRef::Server(_) => access.update_from_neighbor_shapes(state, POS).unwrap(),
        }
    } else {
        let direction = Direction::VALUES[direction as usize];
        let next = POS.relative(direction, 1);
        match access {
            LevelAccessRef::Worldgen(region) => GameWorldgenBlockBehavior
                .update_shape(region, point(POS), direction, point(next), neighbor)
                .unwrap(),
            LevelAccessRef::Server(_) => access
                .as_reader()
                .required_behavior(state.block(), BlockMethod::UpdateShape)
                .unwrap()
                .update_shape(state, POS, direction, next, neighbor, access)
                .unwrap(),
        }
    };
    assert_eq!(actual.id(), fields[4].parse::<u16>().unwrap(), "{fields:?}");
    assert_eq!(
        access.random().unwrap().next_long(),
        fields[6].parse::<i64>().unwrap(),
        "{fields:?}"
    );
    actual
}

#[test]
fn native_sculk_shapes_keep_water_schedules_state_rng_and_worldgen_dispatch() {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../../tests/java/blocks/plants/sculk/shapes.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (_, generator) = crate::block::plants::worldgen_tests::setup();
    let mut chunks = [RegionChunk::new(
        generator.new_chunk([0, 0]),
        ChunkStatus::Features,
    )];
    let mut counts = [0; 2];
    for line in text.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let mut live = crate::block::plants::tests::level_with_height(2);
        live.set_unported_behavior(UnportedBehavior::Strict);
        update(&mut LevelAccessRef::Server(&mut live), &fields);
        let mut saved = ferraloom_storage::SerializableChunkData::default();
        live.save_chunk_ticks(ChunkPos::new(0, 0), &mut saved)
            .unwrap();
        assert!(saved.block_ticks.unwrap_or_default().is_empty(), "{line}");
        let fluid_ticks = saved.fluid_ticks.unwrap_or_default();
        let actual = fluid_ticks
            .iter()
            .map(|tick| {
                assert_eq!(
                    [tick.x.unwrap(), tick.y.unwrap(), tick.z.unwrap()],
                    point(POS),
                    "{line}"
                );
                format!(
                    "{},{},{}",
                    tick.id.as_ref().unwrap().to_string_checked().unwrap(),
                    tick.delay.unwrap(),
                    tick.priority.unwrap()
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(actual, fields[5], "live queue: {line}");
        assert_eq!(live.unported_behavior_stats().skipped_calls, 0);

        chunks[0].fluid_ticks = Default::default();
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
        update(&mut LevelAccessRef::Worldgen(&mut region), &fields);
        drop(region);
        let expected: Vec<_> = fields[5]
            .split(';')
            .filter(|t| !t.is_empty())
            .map(|tick| {
                let fields: Vec<_> = tick.split(',').collect();
                (fields[0].to_owned(), fields[2].parse::<i32>().unwrap())
            })
            .collect();
        let actual: Vec<_> = chunks[0]
            .fluid_ticks
            .ticks()
            .iter()
            .map(|tick| {
                assert_eq!(tick.position, point(POS), "{line}");
                assert_eq!(tick.kind, Fluid::WATER, "{line}");
                (tick.kind.name().to_owned(), tick.priority)
            })
            .collect();
        // ProtoChunkTicks intentionally retains kind/position/priority, not time.
        assert_eq!(actual, expected, "worldgen queue: {line}");
        counts[usize::from(fields[3] == "-1")] += 1;
    }
    assert_eq!(counts, [7416, 1236]);
}
