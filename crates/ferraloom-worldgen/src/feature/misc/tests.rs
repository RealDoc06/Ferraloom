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
use ferraloom_util::random::{Random, RandomSource, WorldgenRandom, XoroshiroRandomSource};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read, path::Path, sync::Arc};

fn cases() -> Vec<Value> {
    let mut text = String::new();
    flate2::read::GzDecoder::new(include_bytes!("testdata/nether_end.json.gz").as_slice())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn resources() -> ResourceManager {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder provides vanilla jar");
    let mut pack = Pack::new("template feature fixtures");
    let template = ferraloom_nbt::parse(include_str!("testdata/template.snbt")).unwrap();
    let mut empty = template.clone();
    empty.as_compound_mut().unwrap().insert(
        "blocks",
        ferraloom_nbt::Tag::List(ferraloom_nbt::ListTag::new()),
    );
    for (name, tag) in [("solid", template), ("empty", empty)] {
        let mut bytes = Vec::new();
        ferraloom_nbt::NbtIo::write(
            &mut bytes,
            ferraloom_nbt::Compression::Gzip,
            &ferraloom_nbt::RootFormat::Named("".into()),
            &tag,
        )
        .unwrap();
        pack.insert(
            format!("ferraloom:structure/{name}.nbt").parse().unwrap(),
            bytes,
        );
    }
    ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap(), pack])
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
/// Fixture storage supplies no shared game behavior. Cached block facts still
/// follow the production WorldGenRegion path.
struct FixtureBehavior;
impl WorldgenBlockBehavior for FixtureBehavior {
    fn post_process_pos(
        &self,
        _: BlockState,
        _: &WorldGenRegion<'_>,
        _: [i32; 3],
    ) -> Result<Option<[i32; 3]>, Error> {
        Err(Error("Unsupported: contextual post_process_pos".into()))
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
            Some("EndPortalBlockEntity initialization")
        }
        "template" if case["name"] == "custom:template_solid" => {
            Some("StructureTemplate neighbor shape updates")
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
            .entry((center, ground.protocol_id(), mode))
            .or_insert_with(|| chunks(&generator, center, ground, mode));
        let mut chunks = baseline.clone();
        let behavior = FixtureBehavior;
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
            &behavior,
            8675309,
            63,
        )
        .unwrap();
        let mut random = WorldgenRandom::new(Random::Xoroshiro(XoroshiroRandomSource::new(
            case["seed"].as_i64().unwrap(),
        )));
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
        // Production chunks retain mutation order in their POI stream, without
        // adding a test observer to the block behavior bridge.
        let mut expected_by_chunk: BTreeMap<[i32; 2], Vec<Value>> = BTreeMap::new();
        for row in &expected {
            let chunk = [
                row[0].as_i64().unwrap() as i32 >> 4,
                row[2].as_i64().unwrap() as i32 >> 4,
            ];
            expected_by_chunk
                .entry(chunk)
                .or_default()
                .push(row.clone());
        }
        for cx in center[0] - 2..=center[0] + 2 {
            for cz in center[1] - 2..=center[1] + 2 {
                let chunk = region.chunk([cx, cz], ChunkStatus::Empty).unwrap();
                let actual = chunk
                    .poi_changes
                    .iter()
                    .map(|(p, _, state)| json!([p[0], p[1], p[2], state.id()]))
                    .collect::<Vec<_>>();
                let expected = expected_by_chunk.remove(&[cx, cz]).unwrap_or_default();
                assert_eq!(
                    actual.len(),
                    expected.len(),
                    "write count chunk={:?} {label}",
                    [cx, cz]
                );
                for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
                    assert_eq!(
                        actual,
                        expected,
                        "write {index} chunk={:?} {label}",
                        [cx, cz]
                    );
                }
            }
        }
        assert!(
            expected_by_chunk.is_empty(),
            "fixture writes outside borrowed chunks: {label}"
        );
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

        let expected_entities = case["block_entities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                let p = std::array::from_fn::<_, 3, _>(|i| {
                    entry["position"][i].as_i64().unwrap() as i32
                });
                (
                    p,
                    ferraloom_nbt::parse_compound(entry["nbt"].as_str().unwrap()).unwrap(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut actual_entities = BTreeMap::new();
        let mut crystals = Vec::new();
        for cx in center[0] - 2..=center[0] + 2 {
            for cz in center[1] - 2..=center[1] + 2 {
                let chunk = region.chunk([cx, cz], ChunkStatus::Empty).unwrap();
                for (&p, tag) in &chunk.block_entities {
                    let mut tag = tag.clone();
                    for key in ["id", "x", "y", "z"] {
                        tag.remove(key);
                    }
                    actual_entities.insert(p, tag);
                }
                crystals.extend(chunk.entities.iter());
            }
        }
        assert_eq!(actual_entities, expected_entities, "block entities {label}");
        let expected_crystals = case["entities"].as_array().unwrap();
        assert_eq!(
            crystals.len(),
            expected_crystals.len(),
            "end crystals {label}"
        );
        for (actual, expected) in crystals.iter().zip(expected_crystals) {
            assert_eq!(
                actual.get_string("id").unwrap().to_string_lossy(),
                expected["id"].as_str().unwrap(),
                "{label}"
            );
            let pos = actual.get_list("Pos").unwrap();
            for i in 0..3 {
                assert_eq!(
                    pos[i].as_double().unwrap(),
                    expected["position"][i].as_f64().unwrap(),
                    "crystal position {label}"
                );
            }
            assert_eq!(
                actual.get_list("Rotation").unwrap()[0]
                    .as_float()
                    .unwrap()
                    .to_bits(),
                (expected["yaw"].as_f64().unwrap() as f32).to_bits(),
                "crystal yaw {label}"
            );
            assert_eq!(
                actual.get_boolean_or("Invulnerable", false),
                expected["invulnerable"].as_bool().unwrap(),
                "{label}"
            );
            if expected["beam"].is_null() {
                assert!(actual.get("beam_target").is_none(), "{label}");
            } else {
                assert_eq!(
                    actual.get_int_array("beam_target").unwrap(),
                    &std::array::from_fn::<_, 3, _>(
                        |i| expected["beam"][i].as_i64().unwrap() as i32
                    ),
                    "{label}"
                );
            }
        }
    }
}
#[test]
fn nether_end_blocks_results_and_rng_match_real_vanilla() {
    check_cases(|case| missing_behavior(case).is_none() && case["config"]["type"] != "template");
}
#[test]
fn template_empty_placement_uses_shared_selection_and_palette_rng() {
    check_cases(|case| missing_behavior(case).is_none() && case["config"]["type"] == "template");
}
#[test]
#[ignore = "Needs shared game chorus, loot, torch survival, portal/gateway block entities, end crystal/fire and template shape behaviors; cross-family children"]
fn pending_game_behavior_and_composite_children() {
    check_cases(|case| missing_behavior(case).is_some() && case.get("vanilla_pending").is_none());
}
