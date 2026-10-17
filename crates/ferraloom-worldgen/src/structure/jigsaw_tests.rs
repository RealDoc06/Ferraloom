use super::*;
use crate::{
    biome::BiomeSource,
    generator::{GenerationData, NoiseBasedChunkGenerator},
    placement::providers::WorldGenerationContext,
    settings::NoiseGeneratorSettings,
    structure::{
        jigsaw_structure::JigsawStructure, pool::StructurePools, processor::tests::generator,
    },
};
use serde_json::Value;
use std::{collections::BTreeMap, io::Read};
fn point(v: &Value) -> [i32; 3] {
    std::array::from_fn(|i| v[i].as_i64().unwrap() as i32)
}
fn element_name(e: &StructurePoolElement) -> String {
    match e {
        StructurePoolElement::Empty => "empty".into(),
        StructurePoolElement::Single {
            location, legacy, ..
        } => format!("{}:{location}", if *legacy { "legacy" } else { "single" }),
        StructurePoolElement::Feature { .. } => "feature".into(),
        StructurePoolElement::List { elements, .. } => format!(
            "[{}]",
            elements
                .iter()
                .map(|e| element_name(e))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}
#[test]
fn every_jigsaw_structure_piece_and_junction_matches_vanilla_flat_and_noise() {
    let (resources, _) = generator();
    let pools = StructurePools::load(&resources, &mut |_| {
        Err(Error("reference test needs no inline feature".into()))
    })
    .unwrap();
    let templates = StructureTemplateManager::load(&resources, pools.templates()).unwrap();
    let definitions = resources
        .registry("worldgen/structure")
        .into_iter()
        .filter_map(|(id, r)| {
            let value = r.json().unwrap();
            (value["type"] == "minecraft:jigsaw").then_some((id, value))
        })
        .map(|(id, value)| {
            (
                id,
                JigsawStructure::decode(&value, &resources, &mut |_| {
                    Err(Error("no inline features".into()))
                })
                .unwrap(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("testdata/jigsaw.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    let cases: Value = serde_json::from_str(&text).unwrap();
    let data = Arc::new(GenerationData::load(&resources).unwrap());
    let settings = resources
        .require(&"worldgen/noise_settings/overworld.json".parse().unwrap())
        .unwrap()
        .json()
        .unwrap();
    for seed in [0, -1, 8675309] {
        let generator = NoiseBasedChunkGenerator::new(
            data.clone(),
            NoiseGeneratorSettings::decode(&settings).unwrap(),
            BiomeSource::Fixed("plains".parse().unwrap()),
            seed,
        )
        .unwrap();
        for case in cases
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["seed"].as_i64() == Some(seed))
        {
            let flat = case["terrain"] == "flat";
            let name: Identifier = case["structure"].as_str().unwrap().parse().unwrap();
            let structure = &definitions[&name];
            let chunk = [
                case["chunk"][0].as_i64().unwrap() as i32,
                case["chunk"][1].as_i64().unwrap() as i32,
            ];
            let context = WorldGenerationContext::new(
                if flat { 0 } else { -64 },
                384,
                -64,
                384,
                if flat { -63 } else { 63 },
            );
            let mut height_cache = BTreeMap::new();
            let result = structure
                .find_generation_point(
                    &pools,
                    &templates,
                    seed,
                    chunk,
                    context,
                    -64,
                    384,
                    &mut |x, z, map| {
                        if flat {
                            return Ok(-60);
                        }
                        if let Some(&height) = height_cache.get(&(x, z, map as u8)) {
                            return Ok(height);
                        }
                        let height = generator.get_first_free_height(x, z, map, -64, 384)?;
                        height_cache.insert((x, z, map as u8), height);
                        Ok(height)
                    },
                    &mut |_, _, _, _| Ok(true),
                )
                .unwrap();
            let description = format!(
                "{name} terrain={} seed={seed} chunk={chunk:?}",
                case["terrain"]
            );
            let Some(result) = result else {
                assert!(
                    case.get("position").is_none(),
                    "{description}: missing start"
                );
                continue;
            };
            assert_eq!(
                result.position,
                point(&case["position"]),
                "{description}: start pos"
            );
            let expected = case["pieces"].as_array().unwrap();
            assert_eq!(result.pieces.len(), expected.len(), "{description}: pieces");
            for (index, (piece, expected)) in result.pieces.iter().zip(expected).enumerate() {
                let description = format!("{description} piece={index}");
                assert_eq!(
                    element_name(&piece.element),
                    expected["element"].as_str().unwrap(),
                    "{description}: element"
                );
                assert_eq!(
                    piece.position,
                    point(&expected["position"]),
                    "{description}: pos"
                );
                assert_eq!(
                    piece.rotation as u64,
                    expected["rotation"].as_u64().unwrap(),
                    "{description}: rotation"
                );
                assert_eq!(
                    piece.ground_level_delta,
                    expected["ground"].as_i64().unwrap() as i32,
                    "{description}: ground"
                );
                let bounds = &expected["bounds"];
                assert_eq!(
                    piece.bounds,
                    BoundingBox::new(
                        point(bounds),
                        [
                            bounds[3].as_i64().unwrap() as i32,
                            bounds[4].as_i64().unwrap() as i32,
                            bounds[5].as_i64().unwrap() as i32
                        ]
                    ),
                    "{description}: bounds"
                );
                let junctions = expected["junctions"].as_array().unwrap();
                assert_eq!(
                    piece.junctions.len(),
                    junctions.len(),
                    "{description}: junction count"
                );
                for (j, expected) in piece.junctions.iter().zip(junctions) {
                    assert_eq!(
                        j.source,
                        point(&expected["source"]),
                        "{description}: junction source"
                    );
                    assert_eq!(
                        j.delta_y,
                        expected["delta_y"].as_i64().unwrap() as i32,
                        "{description}: junction dy"
                    );
                    assert_eq!(
                        j.destination_projection,
                        Projection::decode(&expected["projection"]).unwrap(),
                        "{description}: junction projection"
                    );
                }
            }
        }
    }
}
