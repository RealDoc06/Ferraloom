use super::*;
use crate::{biome::BiomeSource, generator::*, settings::NoiseGeneratorSettings};
use serde_json::json;
use std::{io::Read, path::Path, sync::Arc};

fn golden(bytes: &[u8]) -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn point(v: &Value) -> [i32; 2] {
    [v[0].as_i64().unwrap() as i32, v[1].as_i64().unwrap() as i32]
}
#[test]
fn all_frequency_methods_match_vanilla() {
    for case in golden(include_bytes!("testdata/frequencies.json.gz"))
        .as_array()
        .unwrap()
    {
        let method = match case["method"].as_str().unwrap() {
            "default" => FrequencyReductionMethod::Default,
            "legacy_type_1" => FrequencyReductionMethod::LegacyType1,
            "legacy_type_2" => FrequencyReductionMethod::LegacyType2,
            "legacy_type_3" => FrequencyReductionMethod::LegacyType3,
            _ => unreachable!(),
        };
        let seed = case["seed"].as_i64().unwrap();
        let probability = case["probability"].as_f64().unwrap() as f32;
        let mut values = case["values"].as_array().unwrap().iter();
        for x in (-97..=97).step_by(7) {
            for z in (-83..=83).step_by(11) {
                assert_eq!(
                    method.should_generate(seed, 1234567, [x, z], probability),
                    values.next().unwrap().as_bool().unwrap(),
                    "{method:?} seed={seed} p={probability} chunk=[{x},{z}]"
                );
            }
        }
    }
}
#[test]
fn built_in_placement_chunks_candidates_and_stronghold_rings_match_vanilla() {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder provides jar");
    let resources = ResourceManager::new(vec![
        ferraloom_data::datapack::Pack::vanilla_jar(Path::new(&jar)).unwrap(),
    ]);
    let data = Arc::new(GenerationData::load(&resources).unwrap());
    let definition = resources
        .require(&"worldgen/noise_settings/overworld.json".parse().unwrap())
        .unwrap()
        .json()
        .unwrap();
    let cases = golden(include_bytes!("testdata/placements.json.gz"));
    for seed in [0, -1, 8675309, i64::MIN, i64::MAX] {
        let generator = NoiseBasedChunkGenerator::new(
            data.clone(),
            NoiseGeneratorSettings::decode(&definition).unwrap(),
            BiomeSource::Fixed("plains".parse().unwrap()),
            seed,
        )
        .unwrap();
        let mut state = ChunkGeneratorStructureState::load(&resources, seed, [37, -49]).unwrap();
        // Java's fixed plains source supports the stronghold set's biomes.
        for set in state.sets.values() {
            if let StructurePlacement::ConcentricRings(r) = &set.placement {
                state.rings.push((
                    r.clone(),
                    r.generate_positions(seed, |p, b, random| {
                        find_ring_biome(&generator, p, b, random)
                    })
                    .unwrap(),
                ));
            }
        }
        for case in cases
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["seed"].as_i64() == Some(seed))
        {
            let set_id: Identifier = case["set"].as_str().unwrap().parse().unwrap();
            let placement = &state.sets[&set_id].placement;
            if let StructurePlacement::RandomSpread(p) = placement {
                for c in case["candidates"].as_array().unwrap() {
                    assert_eq!(
                        p.potential_structure_chunk(seed, point(c)),
                        [c[2].as_i64().unwrap() as i32, c[3].as_i64().unwrap() as i32],
                        "{set_id} seed={seed} candidate {c}"
                    );
                }
            }
            if let StructurePlacement::ConcentricRings(p) = placement {
                assert_eq!(
                    state.rings.iter().find(|(r, _)| r == p).unwrap().1,
                    case["rings"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(point)
                        .collect::<Vec<_>>(),
                    "{set_id} seed={seed} rings"
                );
            }
            let mut chunks = Vec::new();
            for x in -64..=64 {
                for z in -64..=64 {
                    if state.is_structure_chunk(placement, [x, z]).unwrap() {
                        chunks.push([x, z]);
                    }
                }
            }
            assert_eq!(
                chunks,
                case["placement_chunks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(point)
                    .collect::<Vec<_>>(),
                "{set_id} seed={seed}"
            );
        }
    }
}
#[test]
fn codec_boundaries_and_exclusion_cycles_are_errors() {
    for (spacing, separation) in [(0, 0), (20, 20), (1, 2), (4097, 2)] {
        assert!(StructurePlacement::decode(&json!({"type":"minecraft:random_spread","spacing":spacing,"separation":separation,"salt":0})).is_err());
    }
    let placement=StructurePlacement::decode(&json!({"type":"minecraft:random_spread","spacing":1,"separation":0,"salt":0,"exclusion_zone":{"other_set":"test:cycle","chunk_count":1}})).unwrap();
    let mut state = ChunkGeneratorStructureState {
        seed: 0,
        origin: [0; 2],
        sets: BTreeMap::new(),
        rings: Vec::new(),
    };
    state.sets.insert(
        "test:cycle".parse().unwrap(),
        StructureSet {
            structures: Vec::new(),
            placement: placement.clone(),
        },
    );
    assert!(
        state
            .is_structure_chunk(&placement, [0, 0])
            .unwrap_err()
            .0
            .contains("cyclic")
    );
}
