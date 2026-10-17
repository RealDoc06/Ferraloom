use super::*;
use crate::{
    biome::BiomeSource,
    generator::{GenerationData, NoiseBasedChunkGenerator},
    region::{ChunkStatus, RegionChunk, WorldGenRegion, WorldgenBlockBehavior},
    settings::NoiseGeneratorSettings,
};
use ferraloom_data::datapack::{Pack, ResourceManager};
use ferraloom_util::random::{Random, WorldgenRandom, XoroshiroRandomSource};
use std::{io::Read, path::Path};

// The controlled vanilla registry contains the patch. Its implementation is
// owned by the vegetation family; preserve registry presence and fail explicitly.
#[derive(Debug)]
struct PendingMossPatch;
impl Feature for PendingMossPatch {
    fn place(&self, _: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        Err(Error(
            "Unsupported: configured feature minecraft:pale_moss_patch".into(),
        ))
    }
}
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
    if terrain == "existing_leaves" {
        if [x, y, z] == [2, 4, 0] {
            return Block::OAK_LOG.default_state();
        }
        if (3..=6).contains(&y) && (1..=3).contains(&x) && (-2..=2).contains(&z) {
            return Block::OAK_LEAVES
                .default_state()
                .with(properties::DISTANCE, 2)
                .unwrap();
        }
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
    let mut registry = loader.finish();
    registry.features.insert(
        "pale_moss_patch".parse().unwrap(),
        Arc::new(PendingMossPatch),
    );
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
        let mut random = WorldgenRandom::new(Random::Xoroshiro(XoroshiroRandomSource::new(seed)));
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
                            | "Unsupported: configured feature minecraft:pale_moss_patch"
                    ),
                    "unexpected pending behavior {label}: {message}"
                );
                assert!(!require_pending, "{label} pending: {message}");
                let prefix = match message.as_str() {
                    "Unsupported: update_shape" => Some("before_edge"),
                    "Unsupported: populate_worldgen_beehive" => Some("before_bees"),
                    "Unsupported: destroy_block" => Some("before_destroy"),
                    "Unsupported: configured feature minecraft:pale_moss_patch" => {
                        Some("before_external")
                    }
                    _ => None,
                };
                if let Some(prefix) = prefix {
                    assert_eq!(
                        random.next_long(),
                        case[format!("{prefix}_next")].as_i64().unwrap(),
                        "{prefix} RNG {label}"
                    );
                    assert_eq!(
                        placed(&chunks, origin, terrain, ground),
                        case[prefix],
                        "{prefix} blocks {label}"
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
        "Tree-family fixture cases compared: {passed}; placement prefix geometry/RNG checked in {geometry} pending tree cases; PENDING (not passing): {pending:?}"
    );
    assert!(passed > 0);
}
#[test]
fn supported_controlled_level_cases_match_vanilla() {
    run_cases(false);
}
#[test]
#[ignore = "pending game bridge: update_shape, state_has_sturdy_face, populate_worldgen_beehive, destroy_block; pale_moss_patch and root-system subfeatures"]
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

#[test]
fn leaf_and_mushroom_property_identity_matches_every_vanilla_state() {
    let mut text = String::new();
    flate2::read::GzDecoder::new(include_bytes!("state_properties.json.gz").as_slice())
        .read_to_string(&mut text)
        .unwrap();
    let data: Value = serde_json::from_str(&text).unwrap();
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    let loader = FeatureLoader::new(&resources).unwrap();
    let support = tag(&loader, "prevents_nearby_leaf_decay").unwrap();
    for (id, distance) in data["leaf_distances"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let state = BlockState::from_id(id as u16).unwrap();
        assert_eq!(
            serde_json::to_value(optional_leaf_distance(state, &support)).unwrap(),
            *distance,
            "leaf distance of state {id}"
        );
        for (i, axis) in Axis::VALUES.into_iter().enumerate() {
            assert_eq!(
                u64::from(axis_state(state, axis).id()),
                data["pillar_axes"][id][i].as_u64().unwrap(),
                "pillar axis of state {id}"
            );
        }
        assert_eq!(
            plants::has_sides(state),
            data["mushroom_sides"][id].as_bool().unwrap(),
            "mushroom faces of state {id}"
        );
    }
}
