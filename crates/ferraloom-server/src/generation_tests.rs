//! Run the live status executor against the committed normal-world oracles.
use super::generation::*;
use crate::{jobs::JobPool, world::RegistryAdapter};
use ferraloom_data::datapack::{Pack, ResourceManager};
use ferraloom_game::level::generation::{ChunkPyramid, ChunkStatus, ProtoChunk, WorldGenContext};
use ferraloom_storage::{self as storage, IoWorker};
use ferraloom_util::ChunkPos;
use ferraloom_worldgen::{
    generator::{ChunkGenerator, GenerationData},
    settings::DimensionType,
};
use std::{io::Read, path::Path, sync::Arc};

fn fixtures(bytes: &[u8]) -> serde_json::Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
pub(crate) fn resources() -> Arc<ResourceManager> {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder provides vanilla jar");
    std::sync::Arc::new(ResourceManager::new(vec![
        Pack::vanilla_jar(Path::new(&jar)).unwrap(),
    ]))
}
pub(crate) fn generation_context(
    resources: &Arc<ResourceManager>,
    generator: Arc<ChunkGenerator>,
    dimension: &str,
    seed: i64,
) -> WorldGenContext {
    let value = resources
        .require(
            &format!("minecraft:dimension_type/{dimension}.json")
                .parse()
                .unwrap(),
        )
        .unwrap()
        .json()
        .unwrap();
    let environment =
        Arc::new(ferraloom_worldgen::environment::EnvironmentDefinitions::load(resources).unwrap());
    let dimension_environment = environment.dimension(&value).unwrap();
    WorldGenContext::load(
        resources,
        generator,
        DimensionType::decode(&value).unwrap(),
        seed,
        true,
        environment,
        dimension_environment,
    )
    .unwrap()
    .with_registry_context(Arc::new(
        crate::registries::RegistrySet::load(resources.clone()).unwrap(),
    ))
}
#[test]
fn live_structure_start_executor_matches_6534_vanilla_chunks() {
    let resources = resources();
    let data = Arc::new(GenerationData::load(&resources).unwrap());
    let mut count = 0;
    for bytes in [
        &include_bytes!(
            "../../ferraloom-worldgen/src/structure/testdata/structure_starts_0.json.gz"
        )[..],
        &include_bytes!(
            "../../ferraloom-worldgen/src/structure/testdata/structure_starts_8675309.json.gz"
        )[..],
    ] {
        for dimension in fixtures(bytes).as_array().unwrap() {
            let seed = dimension["seed"].as_i64().unwrap();
            let generator = Arc::new(
                ChunkGenerator::decode(&dimension["generator"], &resources, data.clone(), seed)
                    .unwrap(),
            );
            let dimension_name = dimension["dimension"]
                .as_str()
                .unwrap()
                .split(':')
                .nth(1)
                .unwrap();
            let context = generation_context(&resources, generator, dimension_name, seed);
            let mut region_chunks = [context.empty_chunk([0, 0]).unwrap()];
            let region = context
                .region(
                    &mut region_chunks,
                    [0, 0],
                    ChunkPyramid::generation().step_to(ChunkStatus::StructureStarts),
                )
                .unwrap();
            assert_eq!(
                region
                    .get_dimension_value(
                        ferraloom_worldgen::environment::EnvironmentAttributes::FAST_LAVA,
                    )
                    .unwrap(),
                context.dimension.fast_lava,
            );
            for expected in dimension["chunks"].as_array().unwrap() {
                let pos = std::array::from_fn(|i| expected["chunk"][i].as_i64().unwrap() as i32);
                let mut chunks = [context.empty_chunk(pos).unwrap()];
                context
                    .apply(
                        &mut chunks,
                        pos,
                        ChunkPyramid::generation().step_to(ChunkStatus::StructureStarts),
                    )
                    .unwrap();
                assert_eq!(
                    chunks[0].structures.starts.len(),
                    expected["starts"].as_array().unwrap().len(),
                    "seed {seed} {pos:?}"
                );
                for start in expected["starts"].as_array().unwrap() {
                    let id = start["structure"]
                        .as_str()
                        .unwrap()
                        .parse::<ferraloom_worldgen::Identifier>()
                        .unwrap();
                    let actual = chunks[0].structures.starts[&id].to_nbt(pos).unwrap();
                    let expected =
                        ferraloom_nbt::parse_compound(start["nbt"].as_str().unwrap()).unwrap();
                    assert_eq!(actual, expected, "seed {seed} {pos:?} {id}");
                }
                count += 1;
            }
        }
    }
    assert_eq!(count, 6534);
}

#[test]
#[ignore = "Strict FEATURES requires registered game block shape/survival/liquid, block-entity and structure-entity callbacks"]
fn twenty_full_feature_chunks_match_vanilla_blocks_and_structure_nbt() {
    // These oracles intentionally stop at FEATURES. FULL region-file parity
    // additionally covers light/postprocessing and entity-storage handoff.
    let resources = resources();
    let data = Arc::new(GenerationData::load(&resources).unwrap());
    let mut count = 0;
    for bytes in [
        &include_bytes!(
            "../../ferraloom-worldgen/src/structure/testdata/generated_structures_0.json.gz"
        )[..],
        &include_bytes!(
            "../../ferraloom-worldgen/src/structure/testdata/generated_structures_8675309.json.gz"
        )[..],
    ] {
        for expected in fixtures(bytes).as_array().unwrap() {
            let seed = expected["seed"].as_i64().unwrap();
            let dimension = expected["dimension"]
                .as_str()
                .unwrap()
                .split(':')
                .nth(1)
                .unwrap();
            let generator_json = serde_json::json!({"type":"minecraft:noise","settings":format!("minecraft:{}",match dimension {"the_end"=>"end","the_nether"=>"nether",_=>"overworld"}),"biome_source":if dimension=="the_end"{serde_json::json!({"type":"minecraft:the_end"})}else{serde_json::json!({"type":"minecraft:multi_noise","preset":if dimension=="the_nether"{"minecraft:nether"}else{"minecraft:overworld"}})}});
            let generator = Arc::new(
                ChunkGenerator::decode(&generator_json, &resources, data.clone(), seed).unwrap(),
            );
            let context = Arc::new(generation_context(&resources, generator, dimension, seed));
            let registries = crate::registries::RegistrySet::load(resources.clone()).unwrap();
            let registry = RegistryAdapter::new(&registries).unwrap();
            let directory = tempfile::tempdir().unwrap();
            let mut io =
                IoWorker::new(directory.path(), storage::RegionFileVersion::Deflate, false)
                    .unwrap();
            let mut state =
                GenerationState::new(context, Arc::new(JobPool::new("golden-light", 1).unwrap()));
            let pos = ChunkPos::new(
                expected["chunk"][0].as_i64().unwrap() as i32,
                expected["chunk"][1].as_i64().unwrap() as i32,
            );
            let actual = state
                .generate(0, pos, ChunkStatus::Features, &registry, &io, None)
                .unwrap();
            let min_y = expected["min_y"].as_i64().unwrap() as i32;
            for (index, block) in expected["blocks"].as_array().unwrap().iter().enumerate() {
                let y = min_y + (index / 256) as i32;
                let section = &actual.terrain.sections[((y - min_y) >> 4) as usize];
                assert_eq!(
                    u64::from(
                        section
                            .get_block_state(
                                (index & 15) as u8,
                                (y & 15) as u8,
                                ((index >> 4) & 15) as u8
                            )
                            .id()
                    ),
                    block.as_u64().unwrap(),
                    "{pos:?} index {index}"
                );
            }
            let actual_starts = actual
                .terrain
                .disk
                .structures
                .as_ref()
                .unwrap()
                .starts
                .as_ref()
                .unwrap();
            let expected_starts = expected["starts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| ferraloom_nbt::parse_compound(s.as_str().unwrap()).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(actual_starts.len(), expected_starts.len());
            for start in expected_starts {
                let id = start.get_string("id").unwrap();
                assert_eq!(actual_starts.get(id), Some(&start));
            }
            let mut block_entities = actual
                .terrain
                .disk
                .block_entities
                .clone()
                .unwrap_or_default();
            let mut expected_entities = expected["block_entities"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| ferraloom_nbt::parse_compound(s.as_str().unwrap()).unwrap())
                .collect::<Vec<_>>();
            let position = |tag: &ferraloom_nbt::CompoundTag| {
                [
                    tag.get_int_or("x", 0),
                    tag.get_int_or("y", 0),
                    tag.get_int_or("z", 0),
                ]
            };
            block_entities.sort_by_key(position);
            expected_entities.sort_by_key(position);
            assert_eq!(block_entities, expected_entities);
            io.close().unwrap();
            count += 1;
        }
    }
    assert_eq!(count, 20);
}

#[test]
fn saved_proto_resumes_and_full_reload_preserves_owned_payload_and_tick_delays() {
    let resources = resources();
    let data = Arc::new(GenerationData::load(&resources).unwrap());
    let generator=Arc::new(ChunkGenerator::decode(&serde_json::json!({"type":"minecraft:flat","settings":{"layers":[{"height":1,"block":"minecraft:bedrock"},{"height":3,"block":"minecraft:stone"}],"biome":"minecraft:plains","features":false,"lakes":false,"structure_overrides":[]}}),&resources,data,8675309).unwrap());
    let context = Arc::new(generation_context(
        &resources,
        generator,
        "overworld",
        8675309,
    ));
    let registry =
        RegistryAdapter::new(&crate::registries::RegistrySet::load(resources.clone()).unwrap())
            .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut io =
        IoWorker::new(directory.path(), storage::RegionFileVersion::Deflate, false).unwrap();
    let light = Arc::new(JobPool::new("resume-light", 1).unwrap());
    let pos = ChunkPos::new(-2, 3);
    // ChunkMap#createEmptyChunk leaves all flat layers for buildTerrain.
    let empty = ProtoChunk::empty(&context, pos).unwrap();
    for section in &empty.region.chunk.sections {
        assert_eq!(
            section.states().counts(),
            vec![(ferraloom_data::Block::AIR.default_state(), 4096)]
        );
        assert_eq!(section.biomes().counts().len(), 1);
    }
    assert_eq!(
        empty.region.chunk.world_surface,
        [context.dimension.min_y; 256]
    );
    assert_eq!(
        empty.region.chunk.ocean_floor,
        [context.dimension.min_y; 256]
    );
    let mut original = GenerationState::new(context.clone(), light.clone());
    let full = original
        .generate(0, pos, ChunkStatus::Full, &registry, &io, None)
        .unwrap();
    assert_eq!(
        full.terrain.sections[0].get_block_state(0, 0, 0),
        ferraloom_data::Block::BEDROCK.default_state()
    );
    assert_eq!(
        full.terrain.sections[0].get_block_state(0, 3, 0),
        ferraloom_data::Block::STONE.default_state()
    );
    for (neighbor, disk) in &full.neighbors {
        io.store(*neighbor, Some(disk.write().unwrap()))
            .wait()
            .unwrap();
    }
    for status in [
        ChunkStatus::Terrain,
        ChunkStatus::InitializeLight,
        ChunkStatus::Full,
    ] {
        let mut disk = full.terrain.disk.clone();
        disk.status = Some(ferraloom_game::level::generation::status_name(status).into());
        disk.block_ticks = Some(vec![storage::SavedTick {
            id: Some("minecraft:stone".into()),
            x: Some(pos.x * 16 + 1),
            y: Some(-62),
            z: Some(pos.z * 16 + 1),
            delay: Some(17),
            priority: Some(-2),
            ..Default::default()
        }]);
        let entity = ferraloom_nbt::parse_compound(
            "{id:\"minecraft:chest\",x:-31,y:-60,z:49,custom:[B;1b,-1b],unknown:-0.0f}",
        )
        .unwrap();
        disk.block_entities = Some(vec![entity.clone()]);
        disk.extra
            .insert("future_field", ferraloom_nbt::Tag::Long(42));
        io.store(pos, Some(disk.write().unwrap())).wait().unwrap();
        let mut reloaded = GenerationState::new(context.clone(), light.clone());
        let actual = reloaded
            .generate(1, pos, ChunkStatus::Full, &registry, &io, None)
            .unwrap();
        assert_eq!(
            actual
                .terrain
                .disk
                .status
                .as_ref()
                .unwrap()
                .to_string_lossy(),
            "minecraft:full"
        );
        assert_eq!(
            actual.terrain.disk.block_entities.as_deref(),
            Some(&[entity][..])
        );
        let tick = &actual.terrain.disk.block_ticks.as_ref().unwrap()[0];
        assert_eq!((tick.delay, tick.priority), (Some(17), Some(-2)));
        assert_eq!(actual.terrain.disk.extra.get_long("future_field"), Some(42));
        assert_eq!(actual.terrain.disk.heightmaps.as_ref().unwrap().len(), 4);
        for (expected, actual) in full.terrain.sections.iter().zip(&actual.terrain.sections) {
            for index in 0..4096 {
                let x = (index & 15) as u8;
                let y = (index >> 8) as u8;
                let z = ((index >> 4) & 15) as u8;
                assert_eq!(
                    expected.get_block_state(x, y, z),
                    actual.get_block_state(x, y, z)
                );
            }
        }
    }
    let mut corrupt = full.terrain.disk.clone();
    corrupt.sections.as_mut().unwrap()[0].sky_light = Some(storage::ByteArray(vec![0]));
    io.store(pos, Some(corrupt.write().unwrap()))
        .wait()
        .unwrap();
    let mut failed = GenerationState::new(context, light);
    let first = failed
        .generate(2, pos, ChunkStatus::Full, &registry, &io, None)
        .err()
        .unwrap();
    assert!(format!("{first:#}").contains("invalid saved light length"));
    let dependent = failed
        .generate(3, pos, ChunkStatus::Full, &registry, &io, None)
        .err()
        .unwrap();
    assert!(format!("{dependent:#}").contains("generation dependency failed"));
    io.close().unwrap();
}

#[test]
fn flat_stronghold_rings_use_zero_seed_and_fixed_biome_search() {
    let resources = resources();
    let data = Arc::new(GenerationData::load(&resources).unwrap());
    let generator=Arc::new(ChunkGenerator::decode(&serde_json::json!({"type":"minecraft:flat","settings":{"layers":[{"height":1,"block":"minecraft:bedrock"}],"biome":"minecraft:plains","features":false,"lakes":false,"structure_overrides":["minecraft:strongholds"]}}),&resources,data,8675309).unwrap());
    let context = generation_context(&resources, generator, "overworld", 8675309);
    let cases = fixtures(include_bytes!(
        "../../ferraloom-worldgen/src/structure/testdata/placements.json.gz"
    ));
    let expected = cases
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["seed"] == 0 && case["set"] == "minecraft:strongholds")
        .unwrap()["rings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pos| std::array::from_fn(|i| pos[i].as_i64().unwrap() as i32))
        .collect::<Vec<[i32; 2]>>();
    assert_eq!(context.structure_state.rings.len(), 1);
    assert_eq!(context.structure_state.rings[0].1, expected);
    assert_eq!(context.structure_state.seed, 8675309);
}
