use ferraloom_data::datapack::{Pack, ResourceManager};
use ferraloom_util::random::{
    LegacyRandomSource, RandomSource, WorldgenRandom, XoroshiroRandomSource,
};
use ferraloom_worldgen::{carver::*, placement::providers::*};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};

fn golden(bytes: &[u8]) -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
#[test]
fn all_provider_types_match_vanilla_samples_and_random_consumption() {
    let cases = golden(include_bytes!("golden/providers.json.gz"));
    let context = WorldGenerationContext {
        min_y: -64,
        height: 384,
        sea_level: 63,
    };
    for case in cases.as_array().unwrap() {
        let seed = case["seed"].as_i64().unwrap();
        let mut random = WorldgenRandom::new(XoroshiroRandomSource::new(seed));
        let config = &case["config"];
        let samples = case["samples"].as_array().unwrap();
        match case["type"].as_str().unwrap() {
            "int" => {
                let p = IntProvider::decode(config).unwrap();
                for expected in samples {
                    assert_eq!(
                        i64::from(p.sample(&mut random)),
                        expected.as_i64().unwrap(),
                        "seed={seed} {config}"
                    );
                }
            }
            "float" => {
                let p = FloatProvider::decode(config).unwrap();
                for expected in samples {
                    assert_eq!(
                        u64::from(p.sample(&mut random).to_bits()),
                        expected.as_u64().unwrap(),
                        "seed={seed} {config}"
                    );
                }
            }
            "height" => {
                let p = HeightProvider::decode(config).unwrap();
                for expected in samples {
                    assert_eq!(
                        i64::from(p.sample(&mut random, context)),
                        expected.as_i64().unwrap(),
                        "seed={seed} {config}"
                    );
                }
            }
            _ => panic!("unknown golden provider kind"),
        }
        assert_eq!(
            random.next_long(),
            case["next"].as_i64().unwrap(),
            "random consumption for {config}"
        );
    }
}
fn mask_hash(mask: &CarvingMask) -> String {
    let mut hash = Sha256::new();
    mask.visit(|x, z, bottom, top| {
        for n in [i32::from(x), i32::from(z), bottom, top] {
            hash.update(n.to_be_bytes());
        }
    });
    format!("{:x}", hash.finalize())
}
#[test]
fn every_builtin_carver_mask_matches_vanilla() {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder provides vanilla jar");
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    let registry = CarverRegistry::load(&resources).unwrap();
    let cases = golden(include_bytes!("golden/carvers.json.gz"));
    for case in cases.as_array().unwrap() {
        let seed = case["seed"].as_i64().unwrap();
        let name = case["carver"].as_str().unwrap();
        let carver = &registry.carvers[&name.parse().unwrap()];
        let p = [
            case["chunk"][0].as_i64().unwrap() as i32,
            case["chunk"][1].as_i64().unwrap() as i32,
        ];
        let context = if name.contains("nether") {
            WorldGenerationContext {
                min_y: 0,
                height: 128,
                sea_level: 32,
            }
        } else {
            WorldGenerationContext {
                min_y: -64,
                height: 384,
                sea_level: 63,
            }
        };
        let mut mask = CarvingMask::new(context.min_y + 1, context.min_y + context.height - 8);
        let mut random = WorldgenRandom::new(LegacyRandomSource::new(0));
        for dx in -8..=8 {
            for dz in -8..=8 {
                let source = [p[0] + dx, p[1] + dz];
                random.set_large_feature_seed(seed, source[0], source[1]);
                if carver.is_start_chunk(&mut random) {
                    carver.carve(context, &mut random, p, source, &mut mask);
                }
            }
        }
        assert_eq!(
            mask_hash(&mask),
            case["hash"].as_str().unwrap(),
            "{name} seed={seed} chunk={p:?}"
        );
        assert_eq!(
            random.next_long(),
            case["next"].as_i64().unwrap(),
            "{name} RNG seed={seed}"
        );
    }
}

#[test]
fn full_terrain_with_carvers_matches_vanilla() {
    use ferraloom_worldgen::{biome::BiomeSource, generator::*, settings::NoiseGeneratorSettings};
    use serde_json::json;
    use std::sync::Arc;
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    let data = Arc::new(GenerationData::load(&resources).unwrap());
    let cases = golden(include_bytes!("golden/carved_chunks.json.gz"));
    for case in cases.as_array().unwrap() {
        let seed = case["seed"].as_i64().unwrap();
        let dimension = case["dimension"].as_str().unwrap();
        let value = resources
            .require(
                &format!("worldgen/noise_settings/{dimension}.json")
                    .parse()
                    .unwrap(),
            )
            .unwrap()
            .json()
            .unwrap();
        let settings = NoiseGeneratorSettings::decode(&value).unwrap();
        let source = BiomeSource::decode(
            &if dimension == "end" {
                json!({"type":"minecraft:the_end"})
            } else {
                json!({"type":"minecraft:multi_noise","preset":format!("minecraft:{dimension}")})
            },
            &resources,
        )
        .unwrap();
        let generator =
            NoiseBasedChunkGenerator::new(data.clone(), settings, source, seed).unwrap();
        let p = [
            case["chunk"][0].as_i64().unwrap() as i32,
            case["chunk"][1].as_i64().unwrap() as i32,
        ];
        let chunk = generator.build_terrain(p, None).unwrap();
        let mut hash = Sha256::new();
        for z in 0..16 {
            for x in 0..16 {
                for y in chunk.min_y..chunk.min_y + chunk.height {
                    hash.update(chunk.block(x, y, z).id().to_be_bytes());
                }
            }
        }
        assert_eq!(
            format!("{:x}", hash.finalize()),
            case["terrain"]["hash"].as_str().unwrap(),
            "terrain {dimension} {seed} {p:?}"
        );
        assert_eq!(
            serde_json::to_value(chunk.world_surface.as_slice()).unwrap(),
            case["terrain"]["world_surface"]
        );
        assert_eq!(
            serde_json::to_value(chunk.ocean_floor.as_slice()).unwrap(),
            case["terrain"]["ocean_floor"]
        );
    }
}

fn fixture_block(p: [i32; 3]) -> ferraloom_data::BlockState {
    use ferraloom_data::{Block, properties};
    if p == [2, 70, 2] {
        return Block::OAK_STAIRS
            .default_state()
            .with(properties::WATERLOGGED, true)
            .unwrap()
            .with(properties::FACING, properties::Facing::East)
            .unwrap();
    }
    let y = p[1];
    if !(-64..320).contains(&y) {
        return Block::VOID_AIR.default_state();
    }
    if y == -64 {
        return Block::BEDROCK.default_state();
    }
    if y == 63 && p[0] & 3 == 0 {
        return Block::WATER.default_state();
    }
    if (0..=63).contains(&y) || y < -10 {
        return Block::STONE.default_state();
    }
    Block::AIR.default_state()
}
fn fixture_generator(
    resources: &ResourceManager,
) -> ferraloom_worldgen::generator::NoiseBasedChunkGenerator {
    use ferraloom_worldgen::{biome::BiomeSource, generator::*, settings::NoiseGeneratorSettings};
    let data = std::sync::Arc::new(GenerationData::load(resources).unwrap());
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
fn fixture_chunks(
    generator: &ferraloom_worldgen::generator::NoiseBasedChunkGenerator,
    center: [i32; 2],
    radius: i32,
) -> Vec<ferraloom_worldgen::region::RegionChunk> {
    use ferraloom_worldgen::region::{ChunkStatus, RegionChunk};
    let mut chunks = Vec::new();
    for cx in center[0] - radius..=center[0] + radius {
        for cz in center[1] - radius..=center[1] + radius {
            let mut chunk = generator.new_chunk([cx, cz]);
            for x in 0..16 {
                for z in 0..16 {
                    for y in -64..=70 {
                        let state =
                            fixture_block([cx * 16 + i32::from(x), y, cz * 16 + i32::from(z)]);
                        if state.block() != ferraloom_data::Block::AIR {
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
struct FixtureBehavior;
impl ferraloom_worldgen::region::WorldgenBlockBehavior for FixtureBehavior {
    fn has_block_entity(&self, _: ferraloom_data::BlockState) -> bool {
        false
    }
    fn post_process_pos(
        &self,
        _: ferraloom_data::BlockState,
        _: &ferraloom_worldgen::region::WorldGenRegion<'_>,
        _: [i32; 3],
    ) -> Result<Option<[i32; 3]>, ferraloom_worldgen::Error> {
        Ok(None)
    }
}
#[derive(Debug)]
struct RecordingFeature;
impl ferraloom_worldgen::feature::Feature for RecordingFeature {
    fn place(
        &self,
        context: &mut ferraloom_worldgen::feature::FeatureContext<'_, '_>,
    ) -> Result<bool, ferraloom_worldgen::Error> {
        let mut tag = ferraloom_nbt::CompoundTag::new();
        tag.insert(
            "position",
            ferraloom_nbt::Tag::IntArray(context.origin.to_vec()),
        );
        tag.insert(
            "random",
            ferraloom_nbt::Tag::Long(context.random.next_long()),
        );
        context.region.add_entity(context.region.center(), tag)?;
        Ok(context.origin[1] & 1 == 0)
    }
}
#[test]
fn placement_stack_positions_and_rng_match_vanilla() {
    use ferraloom_util::random::Random;
    use ferraloom_worldgen::{feature::*, region::*};
    use serde_json::json;
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    let generator = fixture_generator(&resources);
    let cases = golden(include_bytes!("golden/placement.json.gz"));
    let baselines = [[0, 0], [-7, 11]].map(|center| fixture_chunks(&generator, center, 1));
    for case in cases.as_array().unwrap() {
        let p = std::array::from_fn::<_, 3, _>(|i| case["origin"][i].as_i64().unwrap() as i32);
        let center = [p[0] >> 4, p[2] >> 4];
        let mut chunks = baselines[usize::from(center != [0, 0])].clone();
        let mut loader = FeatureLoader::new(&resources).unwrap();
        loader
            .register("ferraloom:record".parse().unwrap(), |_, _| {
                Ok(std::sync::Arc::new(RecordingFeature))
            })
            .unwrap();
        let placed = loader
            .placed(&json!({"feature":{"type":"ferraloom:record"},"placement":case["placement"]}))
            .unwrap();
        let registry = loader.finish();
        let mut region = WorldGenRegion::new(
            &mut chunks,
            center,
            &[ChunkStatus::Terrain; 2],
            1,
            None,
            &FixtureBehavior,
            8675309,
            63,
        )
        .unwrap();
        let seed = case["seed"].as_i64().unwrap();
        let mut random = WorldgenRandom::new(Random::Xoroshiro(XoroshiroRandomSource::new(seed)));
        let result = placed
            .place(
                &mut FeatureContext {
                    region: &mut region,
                    random: &mut random,
                    origin: p,
                    generator: &generator,
                    registry: &registry,
                },
                false,
            )
            .unwrap();
        assert_eq!(result, case["success"].as_bool().unwrap());
        assert_eq!(
            random.next_long(),
            case["next"].as_i64().unwrap(),
            "{} seed={seed}",
            case["placement"]
        );
        let tags = &region.chunk(center, ChunkStatus::Empty).unwrap().entities;
        let actual = tags.iter().map(|tag| json!({"position":tag.get_int_array("position").unwrap(),"random":tag.get_long("random").unwrap()})).collect::<Vec<_>>();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            case["positions"],
            "{} seed={seed} origin={p:?}",
            case["placement"]
        );
    }
}
#[test]
fn all_state_providers_match_vanilla_optional_values_and_rng() {
    use ferraloom_util::random::Random;
    use ferraloom_worldgen::{feature::*, region::*};
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    let generator = fixture_generator(&resources);
    let baseline = fixture_chunks(&generator, [0, 0], 2);
    let cases = golden(include_bytes!("golden/state_providers.json.gz"));
    let points = [
        [0, 70, 0],
        [2, 70, 2],
        [-12, -3, 16],
        [15, 63, 15],
        [-17, -60, -17],
    ];
    for case in cases.as_array().unwrap() {
        let mut loader = FeatureLoader::new(&resources).unwrap();
        let provider = loader.state_provider(&case["config"]).unwrap();
        let registry = loader.finish();
        let mut chunks = baseline.clone();
        let mut region = WorldGenRegion::new(
            &mut chunks,
            [0, 0],
            &[ChunkStatus::Terrain; 3],
            1,
            None,
            &FixtureBehavior,
            8675309,
            63,
        )
        .unwrap();
        let seed = case["seed"].as_i64().unwrap();
        let mut random = WorldgenRandom::new(Random::Xoroshiro(XoroshiroRandomSource::new(seed)));
        let mut context = FeatureContext {
            region: &mut region,
            random: &mut random,
            origin: [0, 70, 0],
            generator: &generator,
            registry: &registry,
        };
        for i in 0..32 {
            let p = points[i % points.len()];
            let optional = provider
                .get_optional_state(&mut context, p)
                .unwrap()
                .map(|s| s.id());
            assert_eq!(
                serde_json::to_value(optional).unwrap(),
                case["optional"][i],
                "{} seed={seed} point={p:?}",
                case["config"]
            );
            assert_eq!(
                u64::from(provider.get_state(&mut context, p).unwrap().id()),
                case["samples"][i].as_u64().unwrap(),
                "{} seed={seed} point={p:?}",
                case["config"]
            );
        }
        assert_eq!(
            random.next_long(),
            case["next"].as_i64().unwrap(),
            "{} seed={seed}",
            case["config"]
        );
    }
}
#[test]
fn feature_sorter_matches_vanilla_order_and_cycle_detection() {
    let cases = golden(include_bytes!("golden/feature_sorter.json.gz"));
    for case in cases.as_array().unwrap() {
        let sources: Vec<Vec<Vec<usize>>> =
            serde_json::from_value(case["sources"].clone()).unwrap();
        let sorted = ferraloom_worldgen::decoration::sorter::build_features_per_step(&sources);
        if case["cycle"] == true {
            assert!(sorted.is_err());
        } else {
            assert_eq!(
                serde_json::to_value(
                    sorted
                        .unwrap()
                        .into_iter()
                        .map(|s| s.features)
                        .collect::<Vec<_>>()
                )
                .unwrap(),
                case["steps"]
            );
        }
    }
}

#[derive(Debug)]
struct DecorationRecord(usize);
impl ferraloom_worldgen::feature::Feature for DecorationRecord {
    fn place(
        &self,
        context: &mut ferraloom_worldgen::feature::FeatureContext<'_, '_>,
    ) -> Result<bool, ferraloom_worldgen::Error> {
        let mut tag = ferraloom_nbt::CompoundTag::new();
        tag.insert(
            "position",
            ferraloom_nbt::Tag::IntArray(context.origin.to_vec()),
        );
        tag.insert("feature", ferraloom_nbt::Tag::Int(self.0 as i32));
        tag.insert(
            "random",
            ferraloom_nbt::Tag::Long(context.random.next_long()),
        );
        context.region.add_entity(context.region.center(), tag)?;
        Ok(true)
    }
}
#[test]
fn actual_vanilla_biome_decoration_matches_feature_indices_seeds_and_filters() {
    use ferraloom_worldgen::{
        decoration::*,
        feature::*,
        placement::{modifier::PlacementModifier, providers::IntProvider},
        region::*,
    };
    use serde_json::json;
    use std::sync::Arc;
    struct Structures(Vec<usize>);
    impl StructureDecoration for Structures {
        fn place_step(
            &mut self,
            step: GenerationStep,
            region: &mut WorldGenRegion<'_>,
            random: &mut WorldgenRandom,
            decoration_seed: i64,
        ) -> Result<(), ferraloom_worldgen::Error> {
            self.0.push(
                region
                    .chunk(region.center(), ChunkStatus::Empty)?
                    .entities
                    .len(),
            );
            // Exercise structure RNG use before features. Feature seeds must
            // remain independent of how many structures or pieces consume RNG.
            random.set_feature_seed(decoration_seed, 17, step as i32);
            for _ in 0..=step as usize {
                random.next_long();
            }
            Ok(())
        }
    }
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    let generator = fixture_generator(&resources);
    let mut registry = FeatureRegistry::default();
    let ids = (0..6)
        .map(|i| format!("ferraloom:test_{i}").parse().unwrap())
        .collect::<Vec<_>>();
    for (i, id) in ids.iter().enumerate() {
        registry.placed.insert(
            id.clone(),
            Arc::new(PlacedFeature {
                id: Some(id.clone()),
                feature: Arc::new(DecorationRecord(i)),
                placement: vec![
                    PlacementModifier::Count(IntProvider::Constant(2)),
                    PlacementModifier::Biome,
                ],
            }),
        );
    }
    let mut a = vec![vec![]; 10];
    a[0] = vec![ids[0].clone(), ids[1].clone()];
    a[6] = vec![ids[2].clone()];
    a[9] = vec![ids[3].clone()];
    let mut b = vec![vec![]; 10];
    b[0] = vec![ids[0].clone(), ids[4].clone()];
    b[6] = vec![ids[2].clone(), ids[5].clone()];
    b[9] = vec![ids[3].clone()];
    registry.biome_features.insert("plains".parse().unwrap(), a);
    registry.biome_features.insert("forest".parse().unwrap(), b);
    let cases = golden(include_bytes!("golden/decoration.json.gz"));
    for case in cases.as_array().unwrap() {
        let possible = case["possible"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().parse().unwrap())
            .collect::<Vec<_>>();
        let plan = DecorationPlan::new(&registry, &possible).unwrap();
        let p = [
            case["chunk"][0].as_i64().unwrap() as i32,
            case["chunk"][1].as_i64().unwrap() as i32,
        ];
        let seed = case["seed"].as_i64().unwrap();
        let mixed = case["mixed"].as_bool().unwrap();
        let mut chunks = Vec::new();
        for x in p[0] - 1..=p[0] + 1 {
            for z in p[1] - 1..=p[1] + 1 {
                let mut chunk = generator.new_chunk([x, z]);
                let biome = generator.data.biome_ids[&if mixed && (x + z) & 1 != 0 {
                    "forest"
                } else {
                    "plains"
                }
                .parse()
                .unwrap()];
                for section in &mut chunk.sections {
                    for x in 0..4 {
                        for y in 0..4 {
                            for z in 0..4 {
                                section.biomes_mut().set(x, y, z, biome);
                            }
                        }
                    }
                }
                chunks.push(RegionChunk::new(chunk, ChunkStatus::Terrain));
            }
        }
        let mut region = WorldGenRegion::new(
            &mut chunks,
            p,
            &[ChunkStatus::Terrain; 2],
            1,
            None,
            &FixtureBehavior,
            seed,
            63,
        )
        .unwrap();
        let mut structures = Structures(Vec::new());
        plan.apply_biome_decoration(&mut region, &generator, &registry, &mut structures)
            .unwrap();
        assert_eq!(structures.0.len(), 11);
        assert_eq!(structures.0[0], 0);
        let output = region.chunk(p,ChunkStatus::Empty).unwrap().entities.iter().map(|t|json!({"feature":t.get_int("feature").unwrap(),"position":t.get_int_array("position").unwrap(),"random":t.get_long("random").unwrap()})).collect::<Vec<_>>();
        assert_eq!(
            serde_json::to_value(output).unwrap(),
            case["output"],
            "seed={seed} chunk={p:?} mixed={mixed}"
        );
    }
}
#[test]
fn region_write_bounds_bulk_writes_and_tick_deduplication() {
    use ferraloom_data::Block;
    use ferraloom_worldgen::region::*;
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    let generator = fixture_generator(&resources);
    let mut chunks = vec![RegionChunk::new(
        generator.new_chunk([0, 0]),
        ChunkStatus::Terrain,
    )];
    let mut region = WorldGenRegion::new(
        &mut chunks,
        [0, 0],
        &[ChunkStatus::Terrain; 2],
        0,
        None,
        &FixtureBehavior,
        0,
        63,
    )
    .unwrap();
    assert!(region.block([16, 0, 0]).is_err());
    assert!(
        !region
            .set_block([16, 0, 0], Block::STONE.default_state(), 2)
            .unwrap()
    );
    assert!(
        region
            .set_block([0, 0, 0], Block::STONE.default_state(), 2)
            .unwrap()
    );
    assert_eq!(region.height_at(Heightmap::WorldSurface, 0, 0).unwrap(), 1);
    region
        .set_block_in_section([0, 4, 0], Block::GOLD_BLOCK.default_state())
        .unwrap();
    assert_eq!(region.height_at(Heightmap::WorldSurface, 0, 0).unwrap(), 1);
    assert_eq!(region.block([0, 4, 0]).unwrap().block(), Block::GOLD_BLOCK);
    assert_eq!(
        region
            .chunk([0, 0], ChunkStatus::Empty)
            .unwrap()
            .poi_changes
            .len(),
        1
    );
    for priority in [2, -1, 0] {
        region
            .schedule_block_tick(SavedTick {
                position: [0, 0, 0],
                kind: Block::STONE,
                priority,
            })
            .unwrap();
    }
    let ticks = region
        .chunk([0, 0], ChunkStatus::Empty)
        .unwrap()
        .block_ticks
        .ticks();
    assert_eq!(ticks.len(), 1);
    assert_eq!(ticks[0].priority, 2);
    assert!(
        region
            .is_outline_full_block(Block::STONE.default_state(), [0, 0, 0])
            .unwrap_err()
            .0
            .starts_with("Unsupported")
    );
}

#[test]
fn biome_source_encounter_order_and_all_builtin_feature_indices_match_vanilla() {
    use ferraloom_worldgen::{
        biome::BiomeSource, climate::SearchContext, decoration::DecorationPlan,
        density::SamplerContext, feature::FeatureLoader, generator::*,
        settings::NoiseGeneratorSettings,
    };
    use std::sync::Arc;
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    let data = Arc::new(GenerationData::load(&resources).unwrap());
    let mut loader = FeatureLoader::new(&resources).unwrap();
    loader.load_biome_features().unwrap();
    let registry = loader.finish();
    let cases = golden(include_bytes!("golden/biome_order.json.gz"));
    let points = [
        [0, 0, 0],
        [-1, -64, -1],
        [7, 25, -31],
        [i32::MAX, 0, i32::MAX],
        [i32::MIN, 1, -13456],
    ];
    for case in cases.as_array().unwrap() {
        let source = BiomeSource::decode(&case["config"], &resources).unwrap();
        let possible = source.possible_biomes();
        assert_eq!(
            serde_json::to_value(possible.iter().map(ToString::to_string).collect::<Vec<_>>())
                .unwrap(),
            case["possible"]
        );
        let plan = DecorationPlan::new(&registry, &possible).unwrap();
        assert_eq!(
            serde_json::to_value(
                plan.steps
                    .iter()
                    .map(|s| s
                        .features
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>())
                    .collect::<Vec<_>>()
            )
            .unwrap(),
            case["features"],
            "{}",
            case["config"]
        );
        if possible.is_empty() {
            continue;
        }
        let dimension = case["dimension"].as_str().unwrap();
        let value = resources
            .require(
                &format!("worldgen/noise_settings/{dimension}.json")
                    .parse()
                    .unwrap(),
            )
            .unwrap()
            .json()
            .unwrap();
        let generator = NoiseBasedChunkGenerator::new(
            data.clone(),
            NoiseGeneratorSettings::decode(&value).unwrap(),
            source,
            8675309,
        )
        .unwrap();
        let mut samplers = SamplerContext::uncached();
        let mut search = SearchContext::default();
        for (p, expected) in points.into_iter().zip(case["samples"].as_array().unwrap()) {
            assert_eq!(
                generator
                    .source
                    .sample(
                        &generator.random.compiler,
                        generator.random.router,
                        &mut samplers,
                        &mut search,
                        p
                    )
                    .to_string(),
                expected.as_str().unwrap(),
                "{} {p:?}",
                case["config"]
            );
        }
    }
}
#[test]
fn base_heights_and_single_block_noise_columns_match_vanilla() {
    use ferraloom_worldgen::{
        biome::BiomeSource, generator::*, region::Heightmap, settings::NoiseGeneratorSettings,
    };
    use serde_json::json;
    use std::{collections::BTreeMap, sync::Arc};
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    let data = Arc::new(GenerationData::load(&resources).unwrap());
    let cases = golden(include_bytes!("golden/base_columns.json.gz"));
    let mut generators = BTreeMap::new();
    for case in cases.as_array().unwrap() {
        let seed = case["seed"].as_i64().unwrap();
        let dimension = case["dimension"].as_str().unwrap();
        let generator = generators.entry((seed,dimension.to_owned())).or_insert_with(|| {
            let value = resources.require(&format!("worldgen/noise_settings/{dimension}.json").parse().unwrap()).unwrap().json().unwrap();
            let source = BiomeSource::decode(&if dimension=="end" {json!({"type":"minecraft:the_end"})} else {json!({"type":"minecraft:multi_noise","preset":format!("minecraft:{dimension}")})},&resources).unwrap();
            NoiseBasedChunkGenerator::new(data.clone(),NoiseGeneratorSettings::decode(&value).unwrap(),source,seed).unwrap()
        });
        let p = [
            case["position"][0].as_i64().unwrap() as i32,
            case["position"][1].as_i64().unwrap() as i32,
        ];
        let bounds = [
            case["bounds"][0].as_i64().unwrap() as i32,
            case["bounds"][1].as_i64().unwrap() as i32,
        ];
        for (i, map) in [
            Heightmap::WorldSurface,
            Heightmap::OceanFloor,
            Heightmap::MotionBlocking,
            Heightmap::MotionBlockingNoLeaves,
        ]
        .into_iter()
        .enumerate()
        {
            let height = generator
                .get_base_height(p[0], p[1], map, bounds[0], bounds[1])
                .unwrap();
            assert_eq!(
                i64::from(height),
                case["heights"][i].as_i64().unwrap(),
                "{dimension} seed={seed} {p:?} {bounds:?} {map:?}"
            );
            assert_eq!(
                generator
                    .get_first_free_height(p[0], p[1], map, bounds[0], bounds[1])
                    .unwrap(),
                height
            );
            assert_eq!(
                generator
                    .get_first_occupied_height(p[0], p[1], map, bounds[0], bounds[1])
                    .unwrap(),
                height - 1
            );
        }
        let column = generator
            .get_base_column(p[0], p[1], bounds[0], bounds[1])
            .unwrap();
        let actual = column.map(|column| {
            (bounds[0] - 1..=bounds[0] + bounds[1])
                .map(|y| column.get_block(y).id())
                .collect::<Vec<_>>()
        });
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            case["column"],
            "{dimension} seed={seed} {p:?} {bounds:?}"
        );
    }
}

#[test]
fn old_chunk_carving_filter_matches_vanilla_jitter_and_boundaries() {
    use ferraloom_worldgen::blending::{Blender, BlendingData};
    let cases = golden(include_bytes!("golden/carving_filter.json.gz"));
    for case in cases.as_array().unwrap() {
        let old = case["old"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                let n = |i| entry[i].as_i64().unwrap() as i32;
                (
                    [n(0), n(1)],
                    BlendingData {
                        min_section: n(2),
                        max_section: n(3),
                        heights: [f32::MAX; 16],
                        densities: std::array::from_fn(|_| None),
                        biomes: std::array::from_fn(|_| None),
                    },
                )
            })
            .collect::<Vec<_>>();
        let blender = Blender::new([0, 0], old).unwrap();
        let mut actual = Vec::new();
        for x in (0..16).step_by(3) {
            for z in (0..16).step_by(3) {
                for y in (-68..264).step_by(7) {
                    actual.push(blender.can_carve([0, 0], x, y, z));
                }
            }
        }
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            case["samples"],
            "old generation {:?}",
            case["old"]
        );
    }
}
