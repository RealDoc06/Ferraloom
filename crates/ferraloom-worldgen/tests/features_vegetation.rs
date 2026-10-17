//! Real-jar placement fixtures. Unsupported game behavior is explicitly pending.
use ferraloom_data::{
    Block, BlockClass, BlockState,
    datapack::{Pack, ResourceManager},
};
use ferraloom_util::random::{RandomSource, WorldgenRandom, XoroshiroRandomSource};
use ferraloom_worldgen::{
    Error,
    biome::BiomeSource,
    feature::{FeatureContext, FeatureLoader},
    generator::{GenerationData, NoiseBasedChunkGenerator},
    region::{ChunkStatus, LightLayer, RegionChunk, WorldGenRegion, WorldgenBlockBehavior},
    settings::NoiseGeneratorSettings,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read, path::Path, sync::Arc};

// This matches the Java controlled level's storage-only setBlock implementation.
// It does not supply plant survival or pretend to implement any game behavior.
struct ControlledStorage;
impl WorldgenBlockBehavior for ControlledStorage {
    fn has_block_entity(&self, state: BlockState) -> bool {
        state.block().is_instance_of(BlockClass::EntityBlock)
    }
    fn post_process_pos(
        &self,
        _: BlockState,
        _: &WorldGenRegion<'_>,
        _: [i32; 3],
    ) -> Result<Option<[i32; 3]>, Error> {
        Ok(None)
    }
}
fn cases() -> Vec<Value> {
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../src/feature/vegetation/cases.json.gz").as_slice(),
    )
    .read_to_end(&mut bytes)
    .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
fn point(value: &Value) -> [i32; 3] {
    std::array::from_fn(|i| value[i].as_i64().unwrap() as i32)
}
fn initial(terrain: &str, p: [i32; 3]) -> BlockState {
    let [x, y, z] = p;
    let ground = match terrain {
        "slope" => 61 + (x + 2 * z).rem_euclid(5),
        "water" => 57,
        _ => 63,
    };
    let b = if !(-64..320).contains(&y) {
        Block::VOID_AIR
    } else if y < ground {
        Block::STONE
    } else if y == ground {
        match terrain {
            "snow" => Block::SNOW_BLOCK,
            "water" => Block::SAND,
            _ => Block::GRASS_BLOCK,
        }
    } else if terrain == "water" && y <= 62 {
        if x.rem_euclid(7) == 0 && z.rem_euclid(7) == 0 {
            Block::PACKED_ICE
        } else {
            Block::WATER
        }
    } else {
        Block::AIR
    };
    b.default_state()
}
fn setup() -> (ResourceManager, NoiseBasedChunkGenerator) {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies vanilla jar");
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
    let source =
        BiomeSource::decode(&json!({"type":"fixed","biome":"plains"}), &resources).unwrap();
    let generator = NoiseBasedChunkGenerator::new(data, settings, source, 8675309).unwrap();
    (resources, generator)
}
fn terrain_chunks(generator: &NoiseBasedChunkGenerator, terrain: &str) -> Vec<RegionChunk> {
    let biome = generator.data.biome_ids[&if terrain == "snow" {
        "snowy_plains"
    } else {
        "plains"
    }
    .parse()
    .unwrap()];
    let mut chunks = Vec::new();
    for cx in -2..=2 {
        for cz in -2..=2 {
            let mut chunk = generator.new_chunk([cx, cz]);
            for x in 0..16 {
                for z in 0..16 {
                    for y in -64..=65 {
                        let s =
                            initial(terrain, [cx * 16 + i32::from(x), y, cz * 16 + i32::from(z)]);
                        if s.block() != Block::AIR {
                            chunk.set_block(x, y, z, s);
                        }
                    }
                }
            }
            for section in &mut chunk.sections {
                for x in 0..4 {
                    for y in 0..4 {
                        for z in 0..4 {
                            section.biomes_mut().set(x, y, z, biome);
                        }
                    }
                }
            }
            chunks.push(RegionChunk::new(chunk, ChunkStatus::Features));
        }
    }
    chunks
}
fn known_pending(error: &str) -> bool {
    [
        "block survival behavior is unavailable",
        "sturdy face behavior is unavailable",
        "solid block behavior is unavailable",
        "Unsupported: VineBlock.isAcceptableNeighbour",
        "Unsupported: DoublePlantBlock.placeAt",
        "Unsupported: MossyCarpetBlock.placeAt and level random",
    ]
    .contains(&error)
        || [
            "sequence",
            "weighted_random_selector",
            "random_selector",
            "simple_random_selector",
            "template",
        ]
        .iter()
        .any(|kind| error == format!("unsupported feature type minecraft:{kind}"))
}
fn replay(require_pending: bool) {
    let (resources, generator) = setup();
    let mut loader = FeatureLoader::new(&resources).unwrap();
    let bases = ["land", "water", "slope", "snow"]
        .map(|terrain| (terrain, terrain_chunks(&generator, terrain)))
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let fixtures = cases();
    let mut passing = 0;
    let mut pending = BTreeMap::<String, usize>::new();
    for case in &fixtures {
        let label = format!(
            "{} {} seed={} origin={}",
            case["name"], case["terrain"], case["seed"], case["origin"]
        );
        if let Some(error) = case["vanilla_pending"].as_str() {
            assert!(
                error.contains("getLevel"),
                "unexpected vanilla fixture failure {label}: {error}"
            );
            if require_pending {
                panic!("pending vanilla controlled-level support: {label}: {error}");
            }
            *pending
                .entry(format!("vanilla template ServerLevel access: {error}"))
                .or_default() += 1;
            continue;
        }
        let feature = match loader.feature(&case["config"]) {
            Ok(f) => f,
            Err(error) if known_pending(&error.0) && !require_pending => {
                *pending.entry(error.0).or_default() += 1;
                continue;
            }
            Err(error) => panic!("{label}: {error}"),
        };
        let mut chunks = bases[case["terrain"].as_str().unwrap()].clone();
        let mut random =
            WorldgenRandom::new(XoroshiroRandomSource::new(case["seed"].as_i64().unwrap()));
        let origin = point(&case["origin"]);
        let center = [origin[0] >> 4, origin[2] >> 4];
        let mut region = WorldGenRegion::new(
            &mut chunks,
            center,
            &[ChunkStatus::Features; 5],
            4,
            None,
            &ControlledStorage,
            8675309,
            63,
        )
        .unwrap();
        region.set_has_skylight(true);
        let registry = Default::default();
        let mut ctx = FeatureContext {
            region: &mut region,
            random: &mut random,
            origin,
            generator: &generator,
            registry: &registry,
        };
        let result = match feature.place(&mut ctx) {
            Ok(value) => value,
            Err(error) if known_pending(&error.0) && !require_pending => {
                *pending.entry(error.0).or_default() += 1;
                continue;
            }
            Err(error) => panic!("{label}: {error}"),
        };
        assert_eq!(result, case["success"].as_bool().unwrap(), "result {label}");
        assert_eq!(
            random.next_long(),
            case["next"].as_i64().unwrap(),
            "random {label}"
        );
        let mut blocks = BTreeMap::new();
        let mut ticks = Vec::new();
        let mut fluid_ticks = Vec::new();
        for chunk in &chunks {
            for &(p, _, state) in &chunk.poi_changes {
                blocks.insert(p, state.id());
            }
            for tick in chunk.block_ticks.ticks() {
                let [x, y, z] = tick.position;
                ticks.push(json!([x, y, z, tick.kind.protocol_id(), tick.priority]));
            }
            for tick in chunk.fluid_ticks.ticks() {
                let [x, y, z] = tick.position;
                fluid_ticks.push(json!([x, y, z, tick.kind.protocol_id(), tick.priority]));
            }
        }
        let actual = blocks
            .into_iter()
            .map(|([x, y, z], state)| json!([x, y, z, state]))
            .collect::<Vec<_>>();
        assert_eq!(json!(actual), case["blocks"], "blocks {label}");
        assert_eq!(json!(ticks), case["ticks"], "ticks {label}");
        assert_eq!(
            json!(fluid_ticks),
            case["fluid_ticks"],
            "fluid ticks {label}"
        );
        passing += 1;
    }
    eprintln!(
        "{passing} complete vanilla placement cases match; {} pending cases: {pending:?}",
        pending.values().sum::<usize>()
    );
    assert!(passing > 0);
}
#[test]
fn supported_vegetation_aquatic_surface_cases_match_real_vanilla() {
    replay(false);
}
#[test]
#[ignore = "pending game canSurvive/sturdy-face/vine/double-plant/moss-carpet behavior and composite/template families"]
fn pending_vegetation_game_behavior_cases() {
    replay(true);
}

#[test]
fn unlit_region_brightness_matches_vanilla_engine_before_and_after_emitter() {
    let (_, generator) = setup();
    let mut chunks = terrain_chunks(&generator, "land");
    let mut region = WorldGenRegion::new(
        &mut chunks,
        [0, 0],
        &[ChunkStatus::Features; 5],
        4,
        None,
        &ControlledStorage,
        0,
        63,
    )
    .unwrap();
    assert!(
        region
            .brightness(LightLayer::Sky, [0, 70, 0])
            .unwrap_err()
            .0
            .contains("skylight context")
    );
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../src/feature/vegetation/unlit_light.json.gz").as_slice(),
    )
    .read_to_end(&mut bytes)
    .unwrap();
    let samples: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    for sample in samples {
        region.set_has_skylight(sample["has_sky"].as_bool().unwrap());
        if sample["emitter"].as_bool().unwrap() {
            region
                .set_block([0, 70, 0], Block::GLOWSTONE.default_state(), 2)
                .unwrap();
        }
        let p = point(&sample["pos"]);
        assert_eq!(
            u64::from(region.brightness(LightLayer::Block, p).unwrap()),
            sample["block"].as_u64().unwrap()
        );
        assert_eq!(
            u64::from(region.brightness(LightLayer::Sky, p).unwrap()),
            sample["sky"].as_u64().unwrap()
        );
    }
}
