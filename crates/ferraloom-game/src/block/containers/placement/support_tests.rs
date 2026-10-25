use crate::{
    Behaviors,
    block::{BlockMethod, LevelAccessRef, LevelReaderRef},
    level::{Level, LevelContext, Region, chunk::LevelChunk},
};
use ferraloom_data::BlockState;
use ferraloom_util::{
    BlockPos, ChunkPos, Direction,
    random::{LegacyRandomSource, Random, RandomSource},
};
use std::{io::Read, sync::Arc};

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

#[test]
fn native_sign_and_banner_support_updates_keep_early_air_water_ticks_and_rng() {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/items/sign-support.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let mut count = 0;
    for line in text.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
        let support = BlockState::from_id(fields[2].parse().unwrap()).unwrap();
        let direction = fields[3].parse::<i32>().unwrap();
        let mut level = level();
        level.set_block_state(POS, state).unwrap();
        for side in Direction::VALUES {
            level
                .set_block_state(POS.relative(side, 1), support)
                .unwrap();
        }
        assert_eq!(
            LevelReaderRef::Server(&level)
                .can_survive(state, POS)
                .unwrap(),
            fields[4].parse::<bool>().unwrap(),
            "{line}"
        );
        *level.random() = Random::Legacy(LegacyRandomSource::new(1234));
        let mut access = LevelAccessRef::Server(&mut level);
        let result = if direction < 0 {
            access.update_from_neighbor_shapes(state, POS).unwrap()
        } else {
            let side = Direction::VALUES[direction as usize];
            access
                .as_reader()
                .required_behavior(state.block(), BlockMethod::UpdateShape)
                .unwrap()
                .update_shape(
                    state,
                    POS,
                    side,
                    POS.relative(side, 1),
                    support,
                    &mut access,
                )
                .unwrap()
        };
        assert_eq!(result.id(), fields[5].parse::<u16>().unwrap(), "{line}");
        assert_eq!(
            level.random().next_long(),
            fields[7].parse::<i64>().unwrap(),
            "{line}"
        );
        let mut saved = ferraloom_storage::SerializableChunkData::default();
        level
            .save_chunk_ticks(ChunkPos::new(0, 0), &mut saved)
            .unwrap();
        assert!(saved.block_ticks.unwrap_or_default().is_empty(), "{line}");
        let actual = saved
            .fluid_ticks
            .unwrap_or_default()
            .iter()
            .map(|tick| {
                assert_eq!(
                    [tick.x.unwrap(), tick.y.unwrap(), tick.z.unwrap()],
                    [POS.x, POS.y, POS.z]
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
        assert_eq!(actual, fields[6], "{line}");
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        count += 1;
    }
    assert_eq!(count, 18_480);
}
