//! Real Java Feature#place fixtures. Pending game behavior is never a pass.
use crate::{
    Error,
    biome::BiomeSource,
    feature::{FeatureContext, FeatureLoader},
    generator::{GenerationData, NoiseBasedChunkGenerator},
    region::{ChunkStatus, RegionChunk, WorldGenRegion, WorldgenBlockBehavior},
    settings::NoiseGeneratorSettings,
};
use ferraloom_data::{
    Block, BlockState,
    datapack::{Pack, ResourceManager},
};
use ferraloom_util::random::{RandomSource, WorldgenRandom, XoroshiroRandomSource};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::Read,
    path::Path,
    sync::{Arc, Mutex},
};

fn cases() -> Vec<Value> {
    let mut text = String::new();
    flate2::read::GzDecoder::new(include_bytes!("testdata/nether_end.json.gz").as_slice())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn resources() -> ResourceManager {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder provides vanilla jar");
    ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()])
}
fn generator(resources: &ResourceManager) -> NoiseBasedChunkGenerator {
    let data = Arc::new(GenerationData::load(resources).unwrap());
    let value = resources
        .require(&"worldgen/noise_settings/overworld.json".parse().unwrap())
        .unwrap()
        .json()
        .unwrap();
    NoiseBasedChunkGenerator::new(
        data,
        NoiseGeneratorSettings::decode(&value).unwrap(),
        BiomeSource::Fixed("plains".parse().unwrap()),
        8675309,
    )
    .unwrap()
}
fn initial(p: [i32; 3], ground: Block, mode: i32) -> BlockState {
    if !(-64..=319).contains(&p[1]) {
        return Block::VOID_AIR.default_state();
    }
    if p[1] == -64 {
        return Block::BEDROCK.default_state();
    }
    let floor = if mode == 0 {
        63
    } else {
        60 + (p[0] * 31 + p[2] * 17).rem_euclid(7)
    };
    if p[1] <= floor {
        ground.default_state()
    } else {
        Block::AIR.default_state()
    }
}
fn chunks(
    generator: &NoiseBasedChunkGenerator,
    center: [i32; 2],
    ground: Block,
    mode: i32,
) -> Vec<RegionChunk> {
    let mut chunks = Vec::new();
    for cx in center[0] - 2..=center[0] + 2 {
        for cz in center[1] - 2..=center[1] + 2 {
            let mut chunk = generator.new_chunk([cx, cz]);
            for x in 0..16 {
                for z in 0..16 {
                    for y in -64..=66 {
                        let state = initial(
                            [cx * 16 + i32::from(x), y, cz * 16 + i32::from(z)],
                            ground,
                            mode,
                        );
                        if state.block() != Block::AIR {
                            chunk.set_block(x, y, z, state);
                        }
                    }
                }
            }
            chunks.push(RegionChunk::new(chunk, ChunkStatus::Terrain));
        }
    }
    chunks
}
/// Only observes the controlled storage writes. No survival, collision, loot or
/// entity behavior is supplied by this adapter.
#[derive(Default)]
struct ObserveWrites(Mutex<Vec<Value>>);
impl WorldgenBlockBehavior for ObserveWrites {
    fn has_block_entity(&self, state: BlockState) -> bool {
        matches!(
            state.block(),
            Block::END_GATEWAY | Block::END_PORTAL | Block::CHEST
        )
    }
    fn post_process_pos(
        &self,
        state: BlockState,
        _: &WorldGenRegion<'_>,
        p: [i32; 3],
    ) -> Result<Option<[i32; 3]>, Error> {
        self.0
            .lock()
            .unwrap()
            .push(json!([p[0], p[1], p[2], state.id()]));
        Ok(None)
    }
}
fn missing_behavior(case: &Value) -> Option<&str> {
    if case.get("vanilla_pending").is_some() {
        return Some("controlled vanilla level dependency");
    }
    if case.get("pending").is_some() {
        return Some("cross-family children");
    }
    match case["config"]["type"]
        .as_str()
        .unwrap()
        .trim_start_matches("minecraft:")
    {
        "chorus_plant" if case["result"] == true => Some("ChorusFlowerBlock.generatePlant"),
        "end_podium" if case["config"]["active"] == true => {
            Some("Level.destroyBlock(dropResources=true)")
        }
        "bonus_chest" => Some("RandomizableContainer.setBlockEntityLootTable; torch canSurvive"),
        _ if case["entities"].as_array().is_some_and(|v| !v.is_empty()) => {
            Some("EndCrystal creation and FireBlock.getState")
        }
        _ if case["block_entities"]
            .as_array()
            .is_some_and(|v| !v.is_empty()) =>
        {
            Some("BlockEntity initialization; TheEndGatewayBlockEntity.setExitPosition")
        }
        _ => None,
    }
}
fn check_cases(select: impl Fn(&Value) -> bool) {
    let resources = resources();
    let generator = generator(&resources);
    let mut baselines = BTreeMap::new();
    for case in cases().iter().filter(|c| select(c)) {
        let p = std::array::from_fn(|i| case["origin"][i].as_i64().unwrap() as i32);
        let center = [p[0] >> 4, p[2] >> 4];
        let mode = case["mode"].as_i64().unwrap() as i32;
        let ground =
            Block::from_name(&format!("minecraft:{}", case["ground"].as_str().unwrap())).unwrap();
        let baseline = baselines
            .entry((center, ground.id(), mode))
            .or_insert_with(|| chunks(&generator, center, ground, mode));
        let mut chunks = baseline.clone();
        let observer = ObserveWrites::default();
        let mut loader = FeatureLoader::new(&resources).unwrap();
        let feature = loader
            .feature(&case["config"])
            .unwrap_or_else(|e| panic!("{}: {e}", case["name"]));
        let registry = loader.finish();
        let mut region = WorldGenRegion::new(
            &mut chunks,
            center,
            &[ChunkStatus::Terrain; 3],
            2,
            None,
            &observer,
            8675309,
            63,
        )
        .unwrap();
        let mut random =
            WorldgenRandom::new(XoroshiroRandomSource::new(case["seed"].as_i64().unwrap()));
        let result = feature
            .place(&mut FeatureContext {
                region: &mut region,
                random: &mut random,
                origin: p,
                generator: &generator,
                registry: &registry,
            })
            .unwrap_or_else(|e| panic!("{} origin={p:?}: {e}", case["name"]));
        let label = format!(
            "{} seed={} origin={p:?} mode={mode}",
            case["name"], case["seed"]
        );
        assert_eq!(result, case["result"].as_bool().unwrap(), "{label}");
        assert_eq!(
            random.next_long(),
            case["next"].as_i64().unwrap(),
            "RNG {label}"
        );
        let expected = case["writes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| Value::Array(row.as_array().unwrap()[..4].to_vec()))
            .collect::<Vec<_>>();
        let actual = observer.0.lock().unwrap();
        // Ordered writes catch traversal changes even when the final volume agrees.
        assert_eq!(actual.len(), expected.len(), "write count {label}");
        for (index, (actual, expected)) in actual.iter().zip(expected.iter()).enumerate() {
            assert_eq!(actual, expected, "write {index} {label}");
        }
        let mut final_blocks = BTreeMap::new();
        for row in &expected {
            let p = std::array::from_fn(|i| row[i].as_i64().unwrap() as i32);
            if (-64..=319).contains(&p[1]) {
                final_blocks.insert(p, row[3].as_u64().unwrap() as u16);
            }
        }
        for (p, state) in final_blocks {
            assert_eq!(
                region.block(p).unwrap().id(),
                state,
                "final block {p:?} {label}"
            );
        }
    }
}
#[test]
fn nether_end_blocks_results_and_rng_match_real_vanilla() {
    check_cases(|case| missing_behavior(case).is_none());
}
#[test]
#[ignore = "Needs shared game chorus, loot, collision, gateway, end crystal/fire and destroyBlock behaviors; cross-family children"]
fn pending_game_behavior_and_composite_children() {
    check_cases(|case| missing_behavior(case).is_some() && case.get("vanilla_pending").is_none());
}
