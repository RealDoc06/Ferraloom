use super::*;
use crate::{
    biome::BiomeSource,
    generator::{GenerationData, NoiseBasedChunkGenerator},
    region::{ChunkStatus, RegionChunk, WorldGenRegion, WorldgenBlockBehavior},
    settings::NoiseGeneratorSettings,
};
use ferraloom_data::datapack::{Pack, ResourceManager};
use ferraloom_util::random::{WorldgenRandom, XoroshiroRandomSource};
use std::{io::Read, path::Path};

struct UnsupportedBehavior;
impl WorldgenBlockBehavior for UnsupportedBehavior {
    fn has_block_entity(&self, s: BlockState) -> bool {
        s.has_block_entity()
    }
    fn post_process_pos(
        &self,
        _s: BlockState,
        _r: &WorldGenRegion<'_>,
        _p: Pos,
    ) -> Result<Option<Pos>, Error> {
        // All non-tree cases compared here place mushroom/fungus blocks. They
        // never request postprocessing; context-sensitive cases remain pending.
        Err(Error("Unsupported: post_process_pos".into()))
    }
}
fn initial(p: Pos, origin: Pos, terrain: &str, ground: BlockState) -> BlockState {
    if !(-64..320).contains(&p[1]) {
        return Block::VOID_AIR.default_state();
    }
    let [x, y, z] = std::array::from_fn::<_, 3, _>(|i| p[i] - origin[i]);
    let slope = if terrain == "slope" {
        x.div_euclid(3)
    } else {
        0
    };
    if y < slope {
        return if y == slope - 1 {
            ground
        } else {
            Block::STONE.default_state()
        };
    }
    if terrain == "cave" && (4..=7).contains(&y) {
        return if y == 7 {
            ground
        } else {
            Block::STONE.default_state()
        };
    }
    if terrain == "water" && y < 3 {
        return Block::WATER.default_state();
    }
    if terrain == "blocked" && (1..=30).contains(&y) && x.abs() <= 12 && z.abs() <= 12
        || terrain == "partial"
            && (3..=8).contains(&y)
            && (1..=3).contains(&x)
            && (-2..=1).contains(&z)
    {
        return Block::STONE.default_state();
    }
    Block::AIR.default_state()
}
fn cases() -> Vec<Value> {
    let mut s = String::new();
    flate2::read::GzDecoder::new(include_bytes!("cases.json.gz").as_slice())
        .read_to_string(&mut s)
        .unwrap();
    serde_json::from_str(&s).unwrap()
}
fn run_cases(require_pending: bool) {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder provides vanilla jar");
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    let data = Arc::new(GenerationData::load(&resources).unwrap());
    let settings = NoiseGeneratorSettings::decode(
        &resources
            .require(&"worldgen/noise_settings/overworld.json".parse().unwrap())
            .unwrap()
            .json()
            .unwrap(),
    )
    .unwrap();
    let generator = NoiseBasedChunkGenerator::new(
        data,
        settings,
        BiomeSource::Fixed("plains".parse().unwrap()),
        8675309,
    )
    .unwrap();
    let cases = cases();
    let mut loader = FeatureLoader::new(&resources).unwrap();
    let mut features = std::collections::BTreeMap::new();
    for case in &cases {
        let name = case["feature"].as_str().unwrap();
        if !features.contains_key(name) {
            features.insert(
                name.to_owned(),
                loader.feature(&Value::String(name.into())).map_err(|e| e.0),
            );
        }
    }
    let registry = loader.finish();
    let mut baselines = std::collections::BTreeMap::new();
    let mut passed = 0;
    let mut geometry = 0;
    let mut pending = std::collections::BTreeMap::<String, usize>::new();
    for case in cases {
        let name = case["feature"].as_str().unwrap();
        let feature = match &features[name] {
            Ok(f) => f,
            Err(message) => {
                assert_eq!(name, "minecraft:rooted_sulfur_spring");
                assert!(
                    message.starts_with("unsupported feature type minecraft:"),
                    "{message}"
                );
                assert!(!require_pending, "{name} pending: {message}");
                *pending.entry(message.clone()).or_default() += 1;
                continue;
            }
        };
        let origin = std::array::from_fn::<_, 3, _>(|i| case["origin"][i].as_i64().unwrap() as i32);
        let ground = BlockState::from_id(case["ground"].as_u64().unwrap() as u16).unwrap();
        let terrain = case["terrain"].as_str().unwrap();
        let center = [origin[0] >> 4, origin[2] >> 4];
        let baseline = baselines
            .entry((origin, terrain.to_owned(), ground.id()))
            .or_insert_with(|| {
                let mut chunks = Vec::new();
                for cx in center[0] - 3..=center[0] + 3 {
                    for cz in center[1] - 3..=center[1] + 3 {
                        let mut chunk = generator.new_chunk([cx, cz]);
                        for x in 0..16 {
                            for z in 0..16 {
                                for y in -64..=origin[1] + 31 {
                                    let s = initial(
                                        [cx * 16 + i32::from(x), y, cz * 16 + i32::from(z)],
                                        origin,
                                        terrain,
                                        ground,
                                    );
                                    if s.block() != Block::AIR {
                                        chunk.set_block(x, y, z, s);
                                    }
                                }
                            }
                        }
                        chunks.push(RegionChunk::new(chunk, ChunkStatus::Terrain));
                    }
                }
                chunks
            });
        let mut chunks = baseline.clone();
        let mut region = WorldGenRegion::new(
            &mut chunks,
            center,
            &[ChunkStatus::Terrain; 4],
            3,
            None,
            &UnsupportedBehavior,
            8675309,
            63,
        )
        .unwrap();
        let seed = case["seed"].as_i64().unwrap();
        let mut random = WorldgenRandom::new(XoroshiroRandomSource::new(seed));
        let result = feature.place(&mut FeatureContext {
            region: &mut region,
            random: &mut random,
            origin,
            generator: &generator,
            registry: &registry,
        });
        let label = format!("{name} {seed} {origin:?} {terrain}");
        match result {
            Err(Error(message)) if message.starts_with("Unsupported:") => {
                assert!(
                    matches!(
                        message.as_str(),
                        "Unsupported: update_shape"
                            | "Unsupported: populate_worldgen_beehive"
                            | "Unsupported: state_has_sturdy_face"
                            | "Unsupported: destroy_block"
                            | "Unsupported: post_process_pos"
                            | "Unsupported: configured feature minecraft:pale_moss_patch"
                    ),
                    "unexpected pending behavior {label}: {message}"
                );
                assert!(!require_pending, "{label} pending: {message}");
                if message == "Unsupported: update_shape"
                    && !name.contains("pale_oak")
                    && !name.starts_with("minecraft:rooted_")
                {
                    assert_eq!(
                        random.next_long(),
                        case["next"].as_i64().unwrap(),
                        "pre-edge RNG {label}"
                    );
                    assert_eq!(
                        placed(&chunks, origin, terrain, ground),
                        case["before_edge"],
                        "pre-edge blocks {label}"
                    );
                    geometry += 1;
                }
                *pending.entry(message).or_default() += 1;
                continue;
            }
            Err(e) => panic!("{label}: {e}"),
            Ok(result) => assert_eq!(result, case["success"].as_bool().unwrap(), "{label}"),
        }
        assert_eq!(
            random.next_long(),
            case["next"].as_i64().unwrap(),
            "RNG {label}"
        );
        assert_eq!(
            placed(&chunks, origin, terrain, ground),
            case["blocks"],
            "blocks {label}"
        );
        assert!(
            chunks.iter().all(|chunk| chunk.block_entities.is_empty()),
            "unexpected entities {label}"
        );
        assert_eq!(
            case["block_entities"],
            serde_json::json!([]),
            "entities {label}"
        );
        passed += 1;
    }
    eprintln!(
        "Tree-family fixture cases compared: {passed}; pre-edge geometry/RNG checked in {geometry} pending tree cases; PENDING (not passing): {pending:?}"
    );
    assert!(passed > 0);
}
#[test]
fn supported_controlled_level_cases_match_vanilla() {
    run_cases(false);
}
#[test]
#[ignore = "pending game bridge: update_shape, post_process_pos, state_has_sturdy_face, populate_worldgen_beehive, destroy_block; pale_moss_patch feature"]
fn pending_controlled_level_cases_require_game_behavior() {
    run_cases(true);
}

fn placed(chunks: &[RegionChunk], origin: Pos, terrain: &str, ground: BlockState) -> Value {
    let mut actual = std::collections::BTreeMap::new();
    for chunk in chunks {
        for &(p, _, _) in &chunk.poi_changes {
            let s = chunk
                .chunk
                .block((p[0] & 15) as u8, p[1], (p[2] & 15) as u8);
            if s != initial(p, origin, terrain, ground) {
                actual.insert(p, s.id());
            } else {
                actual.remove(&p);
            }
        }
    }
    serde_json::to_value(
        actual
            .into_iter()
            .map(|(p, s)| serde_json::json!([p[0], p[1], p[2], s]))
            .collect::<Vec<_>>(),
    )
    .unwrap()
}

#[test]
fn feature_sizes_match_vanilla_at_layer_boundaries() {
    let mut text = String::new();
    flate2::read::GzDecoder::new(include_bytes!("sizes.json.gz").as_slice())
        .read_to_string(&mut text)
        .unwrap();
    let cases: Vec<Value> = serde_json::from_str(&text).unwrap();
    for case in cases {
        let size = FeatureSize::decode(&case["config"]).unwrap();
        let mut actual = Vec::new();
        for h in [0, 1, 5, 16, 80] {
            for y in -1..=h + 1 {
                actual.push(size.at(h, y));
            }
        }
        assert_eq!(serde_json::to_value(actual).unwrap(), case["sizes"]);
        assert_eq!(
            size.min_clipped,
            case["config"]["min_clipped_height"]
                .as_i64()
                .map(|n| n as i32)
        );
    }
}
#[test]
fn decorator_position_order_matches_java_hash_sets() {
    let mut text = String::new();
    flate2::read::GzDecoder::new(include_bytes!("position_sets.json.gz").as_slice())
        .read_to_string(&mut text)
        .unwrap();
    let cases: Vec<Value> = serde_json::from_str(&text).unwrap();
    for case in cases {
        let input: Vec<Pos> = serde_json::from_value(case["input"].clone()).unwrap();
        let mut positions = Positions::default();
        for p in input {
            positions.insert(p);
        }
        assert_eq!(
            serde_json::to_value(positions.iter().collect::<Vec<_>>()).unwrap(),
            case["output"]
        );
    }
}
