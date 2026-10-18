//! Real ServerLevel callbacks with captured light/environment inputs and RNG.
use super::{
    super::BlockMethod,
    tests::{level_with_height, tags},
};
use crate::level::{
    Level,
    environment::{ClockSnapshot, EnvironmentDefinitions},
    weather::ChunkLightSnapshot,
};
use ferraloom_data::{
    Block, BlockState,
    datapack::{Pack, ResourceManager},
};
use ferraloom_light::DataLayer;
use ferraloom_util::{
    BlockPos, ChunkPos,
    random::{LegacyRandomSource, Random, RandomSource},
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::Read,
    sync::{Arc, OnceLock},
};

const POS: BlockPos = BlockPos::new(8, 100, 8);
type Cells = BTreeMap<(i32, i32, i32), BlockState>;
fn cells(text: &str) -> Cells {
    text.split(',')
        .filter(|s| !s.is_empty())
        .map(|cell| {
            let c: Vec<i32> = cell.split(':').map(|s| s.parse().unwrap()).collect();
            (
                (c[0], c[1], c[2]),
                BlockState::from_id(c[3] as u16).unwrap(),
            )
        })
        .collect()
}
fn snapshot(level: &Level) -> Cells {
    let mut result = BTreeMap::new();
    for z in -3..=3 {
        for y in -20..=20 {
            for x in -3..=3 {
                let state = level.get_block_state(POS.offset(x, y, z)).unwrap();
                if !state.is_air() {
                    result.insert((x, y, z), state);
                }
            }
        }
    }
    result
}
fn setup(before: &str, seed: i64) -> Level {
    let mut level = level_with_height(8);
    let (blocks, fluids) = tags();
    level.set_block_tags(blocks);
    level.set_fluid_tags(fluids);
    level.rules_mut().block_drops = Some(false);
    for ((x, y, z), state) in cells(before) {
        level.set_block_state(POS.offset(x, y, z), state).unwrap();
    }
    level.set_random_streams(Random::Legacy(LegacyRandomSource::new(seed)), 0);
    level
}
fn environment(level: &mut Level, sky_darken: i32, evaporates: bool, eyeblossom: &str) {
    static DEFINITIONS: OnceLock<Arc<EnvironmentDefinitions>> = OnceLock::new();
    let definitions = DEFINITIONS
        .get_or_init(|| {
            let mut pack = Pack::new("native-plant-observations");
            pack.insert(
                "worldgen/biome/fixture.json".parse().unwrap(),
                serde_json::to_vec(&json!({"attributes":{}})).unwrap(),
            );
            Arc::new(EnvironmentDefinitions::load(&ResourceManager::new(vec![pack])).unwrap())
        })
        .clone();
    // These are the observed inputs of the callback, not inferred clock/weather
    // defaults. The normal typed runtime performs every attribute query.
    let dimension = definitions
        .dimension(&json!({"attributes":{
            "gameplay/sky_light_level": 15 - sky_darken,
            "gameplay/water_evaporates": evaporates,
            "gameplay/eyeblossom_open": eyeblossom,
        }}))
        .unwrap();
    level
        .set_environment_attributes(
            definitions,
            &dimension,
            "overworld".parse().unwrap(),
            &["fixture".parse().unwrap()],
            ClockSnapshot::new([]),
        )
        .unwrap();
    assert_eq!(level.sky_darken().unwrap(), sky_darken);
}
fn ticks(level: &mut Level, fluid: bool) -> String {
    let mut saved = ferraloom_storage::SerializableChunkData::default();
    level
        .save_chunk_ticks(ChunkPos::containing(POS), &mut saved)
        .unwrap();
    let list = if fluid {
        saved.fluid_ticks
    } else {
        saved.block_ticks
    }
    .unwrap();
    // Saved ticks preserve source insertion order. Sort the captured records by
    // vanilla DRAIN_ORDER, as the Java harness does before recording its queue.
    let mut list: Vec<_> = list.into_iter().enumerate().collect();
    list.sort_by_key(|(order, tick)| (tick.delay.unwrap(), tick.priority.unwrap(), *order));
    list.into_iter()
        .filter_map(|(_, tick)| {
            let x = tick.x.unwrap() - POS.x;
            let y = tick.y.unwrap() - POS.y;
            let z = tick.z.unwrap() - POS.z;
            if x.abs() > 3 || y.abs() > 20 || z.abs() > 3 {
                return None;
            }
            let name = tick.id.unwrap().to_string_checked().unwrap();
            let id = if fluid {
                ferraloom_data::registries::fluid::Id::from_name(&name)
                    .unwrap()
                    .protocol_id()
            } else {
                Block::from_name(&name).unwrap().protocol_id()
            };
            Some(format!(
                "{x}:{y}:{z}:{id}:{}:{}",
                tick.delay.unwrap(),
                tick.priority.unwrap()
            ))
        })
        .collect::<Vec<_>>()
        .join(",")
}
#[test]
fn native_ground_ticks_match_changes_queues_and_rng() {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/ground-ticks.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let mut count = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let mut level = setup(f[8], f[2].parse().unwrap());
        let darken: i32 = f[5].parse().unwrap();
        environment(&mut level, darken, f[7] == "true", "default");
        let mut sky = DataLayer::new(0);
        let mut block = DataLayer::new(0);
        sky.set(8, (POS.y & 15) as u8, 8, f[3].parse().unwrap());
        block.set(8, (POS.y & 15) as u8, 8, f[4].parse().unwrap());
        // Only the raw maximum above is read here. The native record supplies
        // that value separately from the center's sky and block light layers.
        let above: u8 = f[6].parse().unwrap();
        sky.set(
            8,
            ((POS.y + 1) & 15) as u8,
            8,
            (i32::from(above) + darken).clamp(0, 15) as u8,
        );
        level.set_light_snapshot(
            ChunkPos::containing(POS),
            ChunkLightSnapshot {
                sky: [(POS.y >> 4, sky)].into(),
                block: [(POS.y >> 4, block)].into(),
            },
        );
        assert_eq!(
            level.max_local_raw_brightness(POS.above(1)).unwrap(),
            i32::from(above),
            "light {line}"
        );
        let method = if state.block() == Block::FROSTED_ICE {
            BlockMethod::Tick
        } else {
            BlockMethod::RandomTick
        };
        let behavior = level
            .required_block_behavior(state.block(), method)
            .unwrap();
        if method == BlockMethod::Tick {
            behavior.tick(state, POS, &mut level).unwrap();
        } else {
            behavior.random_tick(state, POS, &mut level).unwrap();
        }
        assert_eq!(snapshot(&level), cells(f[9]), "blocks {line}");
        assert_eq!(ticks(&mut level, false), f[10], "block ticks {line}");
        assert_eq!(ticks(&mut level, true), f[11], "fluid ticks {line}");
        assert_eq!(
            level.random().next_long(),
            f[12].parse::<i64>().unwrap(),
            "random {line}"
        );
        count += 1;
    }
    assert_eq!(count, 720);
}
#[test]
fn native_eyeblossom_transitions_match_queues_and_rng() {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/eyeblossom-ticks.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let mut count = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let mut level = setup(f[5], f[2].parse().unwrap());
        environment(&mut level, 0, false, f[4]);
        let method = if f[3] == "true" {
            BlockMethod::Tick
        } else {
            BlockMethod::RandomTick
        };
        let behavior = level
            .required_block_behavior(state.block(), method)
            .unwrap();
        if method == BlockMethod::Tick {
            behavior.tick(state, POS, &mut level).unwrap();
        } else {
            behavior.random_tick(state, POS, &mut level).unwrap();
        }
        assert_eq!(snapshot(&level), cells(f[6]), "blocks {line}");
        assert_eq!(ticks(&mut level, false), f[7], "ticks {line}");
        assert_eq!(
            level.random().next_long(),
            f[8].parse::<i64>().unwrap(),
            "random {line}"
        );
        count += 1;
    }
    assert_eq!(count, 192);
}
