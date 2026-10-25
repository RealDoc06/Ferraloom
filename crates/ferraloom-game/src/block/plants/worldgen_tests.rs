//! Real-jar placement fixtures. Unsupported game behavior is explicitly pending.
use crate::block::GameWorldgenBlockBehavior;
use ferraloom_data::{
    Block, BlockState,
    datapack::{Pack, ResourceManager},
};
use ferraloom_util::random::{Random, RandomSource, WorldgenRandom, XoroshiroRandomSource};
use ferraloom_worldgen::{
    biome::BiomeSource,
    feature::{FeatureContext, FeatureLoader, FeatureRandom, FeatureRandomSelector, WorldGenLevel},
    generator::{GenerationData, NoiseBasedChunkGenerator},
    region::{ChunkStatus, RegionChunk, WorldGenRegion},
    settings::NoiseGeneratorSettings,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read, path::Path, sync::Arc};

// The original worldgen fixture adapter supplies storage only. Keep it here to
// measure exactly which previously pending cases the real game bridge completes.
struct StorageOnly;
impl ferraloom_worldgen::region::WorldgenBlockBehavior for StorageOnly {
    fn post_process_pos(
        &self,
        _state: BlockState,
        _region: &WorldGenRegion<'_>,
        _position: [i32; 3],
    ) -> Result<Option<[i32; 3]>, ferraloom_worldgen::Error> {
        Ok(None)
    }
}
fn cases() -> Vec<Value> {
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../ferraloom-worldgen/src/feature/vegetation/cases.json.gz")
            .as_slice(),
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
pub(super) fn setup() -> (ResourceManager, NoiseBasedChunkGenerator) {
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
    if error.contains("UnsupportedBlockMethod(") || error.contains("UnsupportedWorldgenEnvironment")
    {
        return true;
    }
    [
        "block survival behavior is unavailable",
        "sturdy face behavior is unavailable",
        "solid block behavior is unavailable",
        "Unsupported: VineBlock.isAcceptableNeighbour",
        "Unsupported: DoublePlantBlock.placeAt",
        "Unsupported: MossyCarpetBlock.placeAt and level random",
        "Unsupported: load_block_entity",
        "Unsupported: place_liquid",
        "Unsupported: update_shape",
        "Unsupported: update_from_neighbor_shapes",
    ]
    .contains(&error)
}
#[derive(Default)]
struct Coverage {
    passing: usize,
    pending: usize,
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
    let mut unpended = 0;
    let mut survival_unpended = 0;
    let mut pending = BTreeMap::<String, usize>::new();
    let mut coverage = BTreeMap::<&str, Coverage>::new();
    for case in &fixtures {
        let coverage = coverage.entry(case["name"].as_str().unwrap()).or_default();
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
            coverage.pending += 1;
            continue;
        }
        let feature = match loader.feature(&case["config"]) {
            Ok(f) => f,
            Err(error) if known_pending(&error.0) && !require_pending => {
                *pending.entry(error.0).or_default() += 1;
                coverage.pending += 1;
                continue;
            }
            Err(error) => panic!("{label}: {error}"),
        };

        let original_pending = {
            let mut chunks = bases[case["terrain"].as_str().unwrap()].clone();
            let origin = point(&case["origin"]);
            let registry_context = crate::level::LevelContext::fixture(0, 63);
            let mut region = WorldGenRegion::new(
                &mut chunks,
                [origin[0] >> 4, origin[2] >> 4],
                &[ChunkStatus::Features; 5],
                4,
                None,
                &StorageOnly,
                8675309,
                63,
            )
            .unwrap();
            region.set_registry_context(registry_context.registry_context().unwrap());
            region.set_has_skylight(true);
            let mut random = WorldgenRandom::new(Random::Xoroshiro(XoroshiroRandomSource::new(
                case["seed"].as_i64().unwrap(),
            )));
            let registry = Default::default();
            let mut context = FeatureContext {
                level: WorldGenLevel::Region(&mut region),
                random: FeatureRandomSelector::External(FeatureRandom::Region(&mut random)),
                origin,
                generator: &generator,
                registry: &registry,
            };
            match feature.place(&mut context) {
                Ok(_) => None,
                Err(error) => {
                    assert!(
                        known_pending(&error.0),
                        "unexpected storage-only error {label}: {error}"
                    );
                    Some(error.0)
                }
            }
        };
        let mut chunks = bases[case["terrain"].as_str().unwrap()].clone();
        let mut random = WorldgenRandom::new(Random::Xoroshiro(XoroshiroRandomSource::new(
            case["seed"].as_i64().unwrap(),
        )));
        let origin = point(&case["origin"]);
        let center = [origin[0] >> 4, origin[2] >> 4];
        let registry_context = crate::level::LevelContext::fixture(0, 63);
        let mut region = WorldGenRegion::new(
            &mut chunks,
            center,
            &[ChunkStatus::Features; 5],
            4,
            None,
            &GameWorldgenBlockBehavior,
            8675309,
            63,
        )
        .unwrap();
        region.set_registry_context(registry_context.registry_context().unwrap());
        region.set_has_skylight(true);
        region.set_block_tags(generator.data.block_tags.clone());
        region.set_fluid_tags(generator.data.fluid_tags.clone());
        region.set_random(Random::Xoroshiro(XoroshiroRandomSource::new(7331)));
        let registry = Default::default();
        let mut ctx = FeatureContext {
            level: WorldGenLevel::Region(&mut region),
            random: FeatureRandomSelector::External(FeatureRandom::Region(&mut random)),
            origin,
            generator: &generator,
            registry: &registry,
        };
        let result = match feature.place(&mut ctx) {
            Ok(value) => value,
            Err(error) if known_pending(&error.0) && !require_pending => {
                *pending.entry(error.0).or_default() += 1;
                coverage.pending += 1;
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
        assert_eq!(
            region.random_mut().unwrap().next_long(),
            case["level_next"].as_i64().unwrap(),
            "level random {label}"
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
        if let Some(error) = original_pending {
            unpended += 1;
            if error == "block survival behavior is unavailable" {
                survival_unpended += 1;
            }
        }
        passing += 1;
        coverage.passing += 1;
    }
    eprintln!(
        "Real game bridge un-pended {unpended} cases, including {survival_unpended} canSurvive cases"
    );
    for (name, coverage) in coverage {
        eprintln!(
            "{name}: {} passing, {} pending",
            coverage.passing, coverage.pending
        );
    }
    eprintln!(
        "{passing} complete vanilla placement cases match; {} pending cases: {pending:?}",
        pending.values().sum::<usize>()
    );
    assert!(
        unpended > 2000,
        "real shared survival should complete most pending vegetation fixtures"
    );
}
#[test]
fn vegetation_fixtures_use_real_game_bridge() {
    replay(false);
}

#[test]
fn worldgen_bonemeal_targets_follow_native_non_server_branch_without_rng() {
    use super::{BonemealSource, bonemealable_behavior};
    use crate::block::LevelAccessRef;
    use ferraloom_util::{BlockPos, random::LegacyRandomSource};
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/non-server-targets.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let (_, generator) = setup();
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
    let pos = BlockPos::new(8, 5, 8);
    let mut count = 0;
    for line in text.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
        region.set_random(Random::Legacy(LegacyRandomSource::new(42)));
        let behavior = bonemealable_behavior(state.block()).unwrap();
        assert_eq!(
            behavior
                .is_valid_bonemeal_target(
                    state,
                    pos,
                    &mut LevelAccessRef::Worldgen(&mut region),
                    BonemealSource::Interaction
                )
                .unwrap(),
            fields[2] == "true",
            "{line}"
        );
        assert_eq!(
            region.random_mut().unwrap().next_long(),
            fields[3].parse::<i64>().unwrap(),
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 62);
}
