//! Actual normal-world STRUCTURE_STARTS captures, rather than isolated generators.
use super::*;
use crate::{
    climate::SearchContext,
    density::SamplerContext,
    generator::{ChunkGenerator, ChunkGeneratorAccess},
    structure::{
        pipeline::{ChunkStructures, create_structures},
        placement::{ChunkGeneratorStructureState, StructurePlacement, find_ring_biome},
    },
};
use std::collections::BTreeSet;

#[test]
fn normal_world_structure_start_status_matches_vanilla_across_dimensions_and_seeds() {
    let (resources, base) = generator();
    let registry = StructureRegistry::load(&resources, &base.data).unwrap();
    let structures = StructureGenerators::load(&resources, registry, &mut |_| {
        Err(Error("unexpected inline feature".into()))
    })
    .unwrap();
    let pools = StructurePools::load(&resources, &mut |_| {
        Err(Error("unexpected inline feature".into()))
    })
    .unwrap();
    let templates = StructureTemplateManager::load_all(&resources).unwrap();
    for (bytes, generated) in [
        (
            &include_bytes!("testdata/structure_starts_0.json.gz")[..],
            &include_bytes!("testdata/generated_structures_0.json.gz")[..],
        ),
        (
            &include_bytes!("testdata/structure_starts_8675309.json.gz")[..],
            &include_bytes!("testdata/generated_structures_8675309.json.gz")[..],
        ),
    ] {
        let mut text = String::new();
        flate2::read::GzDecoder::new(bytes)
            .read_to_string(&mut text)
            .unwrap();
        let dimensions: Value = serde_json::from_str(&text).unwrap();
        let mut text = String::new();
        flate2::read::GzDecoder::new(generated)
            .read_to_string(&mut text)
            .unwrap();
        let generated: Value = serde_json::from_str(&text).unwrap();
        for dimension in dimensions.as_array().unwrap() {
            let seed = dimension["seed"].as_i64().unwrap();
            let decoded = ChunkGenerator::decode(
                &dimension["generator"],
                &resources,
                base.data.clone(),
                seed,
            )
            .unwrap();
            let ChunkGenerator::Noise(noise) = decoded else {
                panic!("normal preset must use a noise generator");
            };
            let origin = noise.get_origin().unwrap();
            assert_eq!(
                origin,
                std::array::from_fn(|i| dimension["origin"][i].as_i64().unwrap() as i32),
                "seed={seed} dimension={}",
                dimension["dimension"]
            );
            let mut state = ChunkGeneratorStructureState::load(&resources, seed, origin).unwrap();
            let possible = structures
                .registry
                .possible_structure_sets(&state, &noise.source.possible_biomes())
                .unwrap();
            let expected_sets = dimension["sets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().parse::<Identifier>().unwrap())
                .collect::<BTreeSet<_>>();
            assert_eq!(
                possible.iter().cloned().collect::<BTreeSet<_>>(),
                expected_sets
            );
            for entry in dimension["rings"].as_array().unwrap() {
                let id = entry["set"]
                    .as_str()
                    .unwrap()
                    .parse::<Identifier>()
                    .unwrap();
                let StructurePlacement::ConcentricRings(ring) = &state.sets[&id].placement else {
                    panic!("expected concentric ring placement");
                };
                let positions = ring
                    .generate_positions(seed, |p, allowed, random| {
                        find_ring_biome(&noise, p, allowed, random)
                    })
                    .unwrap();
                let expected = entry["positions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| std::array::from_fn(|i| p[i].as_i64().unwrap() as i32))
                    .collect::<Vec<[i32; 2]>>();
                assert_eq!(positions, expected, "seed={seed} ring={id}");
                state.rings.push((ring.clone(), positions));
            }
            // These located starts cover distant structures absent from the
            // origin area, including the End's outer islands and strongholds.
            for expected in generated
                .as_array()
                .unwrap()
                .iter()
                .filter(|case| case["dimension"] == dimension["dimension"])
            {
                let chunk =
                    std::array::from_fn(|i| expected["origin_chunk"][i].as_i64().unwrap() as i32);
                let id = expected["structure"]
                    .as_str()
                    .unwrap()
                    .parse::<Identifier>()
                    .unwrap();
                let mut sampler = SamplerContext::cached();
                let mut search = SearchContext::default();
                let mut biome = |quart| {
                    Ok(noise.source.sample(
                        &noise.random.compiler,
                        noise.random.router,
                        &mut sampler,
                        &mut search,
                        quart,
                    ))
                };
                let actual = structures
                    .generate(
                        &id,
                        0,
                        &mut StructureGenerationContext {
                            seed,
                            chunk,
                            min_y: dimension["min_y"].as_i64().unwrap() as i32,
                            height: dimension["height"].as_i64().unwrap() as i32,
                            generator: noise.as_ref(),
                            templates: &templates,
                            pools: &pools,
                            biome: &mut biome,
                        },
                    )
                    .unwrap();
                assert_start_nbt(
                    &actual.to_nbt(chunk).unwrap(),
                    &ferraloom_nbt::parse_compound(expected["start_before"].as_str().unwrap())
                        .unwrap(),
                    &format!("located seed={seed} structure={id} chunk={chunk:?}"),
                );
            }
            for expected in dimension["chunks"].as_array().unwrap() {
                let chunk = std::array::from_fn(|i| expected["chunk"][i].as_i64().unwrap() as i32);
                let mut actual = ChunkStructures::new(chunk);
                let mut sampler = SamplerContext::cached();
                let mut search = SearchContext::default();
                let mut biome = |quart| {
                    Ok(noise.source.sample(
                        &noise.random.compiler,
                        noise.random.router,
                        &mut sampler,
                        &mut search,
                        quart,
                    ))
                };
                create_structures(&state, &possible, &mut actual, &mut |id, references| {
                    structures.generate(
                        id,
                        references,
                        &mut StructureGenerationContext {
                            seed,
                            chunk,
                            min_y: dimension["min_y"].as_i64().unwrap() as i32,
                            height: dimension["height"].as_i64().unwrap() as i32,
                            generator: noise.as_ref(),
                            templates: &templates,
                            pools: &pools,
                            biome: &mut biome,
                        },
                    )
                })
                .unwrap();
                let expected = expected["starts"].as_array().unwrap();
                let label = format!(
                    "seed={seed} dimension={} chunk={chunk:?}",
                    dimension["dimension"]
                );
                assert_eq!(
                    actual
                        .starts
                        .keys()
                        .map(ToString::to_string)
                        .collect::<BTreeSet<_>>(),
                    expected
                        .iter()
                        .map(|s| s["structure"].as_str().unwrap().to_owned())
                        .collect::<BTreeSet<_>>(),
                    "{label}"
                );
                for start in expected {
                    let id = start["structure"]
                        .as_str()
                        .unwrap()
                        .parse::<Identifier>()
                        .unwrap();
                    assert_start_nbt(
                        &actual.starts[&id].to_nbt(chunk).unwrap(),
                        &ferraloom_nbt::parse_compound(start["nbt"].as_str().unwrap()).unwrap(),
                        &format!("{label} structure={id}"),
                    );
                }
            }
        }
    }
}
