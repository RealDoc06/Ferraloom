//! Run the live status executor against the committed normal-world oracles.
use super::generation::*;
use ferraloom_data::datapack::{Pack, ResourceManager};
use ferraloom_game::level::generation::{ChunkPyramid, WorldGenContext};
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
fn resources() -> ResourceManager {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder provides vanilla jar");
    ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()])
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
            let dimension_type = DimensionType::decode(
                &resources
                    .require(
                        &format!("minecraft:dimension_type/{dimension_name}.json")
                            .parse()
                            .unwrap(),
                    )
                    .unwrap()
                    .json()
                    .unwrap(),
            )
            .unwrap();
            let context =
                WorldGenContext::load(&resources, generator, dimension_type, seed, true).unwrap();
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
                    let id = start["structure"].as_str().unwrap().parse().unwrap();
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
            let dimension_type = DimensionType::decode(
                &resources
                    .require(
                        &format!("minecraft:dimension_type/{dimension}.json")
                            .parse()
                            .unwrap(),
                    )
                    .unwrap()
                    .json()
                    .unwrap(),
            )
            .unwrap();
            let context = Arc::new(
                WorldGenContext::load(&resources, generator, dimension_type, seed, true).unwrap(),
            );
            let registries = crate::registries::RegistrySet::load(&resources).unwrap();
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
