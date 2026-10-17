use super::*;
use crate::{
    region::{ChunkStatus, RegionChunk},
    structure::processor::tests::{StaticBlocks, generator},
};
use ferraloom_util::random::{Algorithm, RandomSource};
use serde_json::Value;
use std::io::Read;

#[test]
fn structure_registry_step_indices_and_empty_starts_rng_match_vanilla() {
    let (resources, generator) = generator();
    let registry = StructureStep::load(&resources).unwrap();
    let templates = StructureTemplateManager::load(&resources, Vec::new()).unwrap();
    let tags = BlockTags::load(&resources).unwrap();
    let features = FeatureRegistry::default();
    let starts = BTreeMap::new();
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("testdata/decoration.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    for case in serde_json::from_str::<Value>(&text)
        .unwrap()
        .as_array()
        .unwrap()
    {
        let step = GenerationStep::ALL[case["step"].as_u64().unwrap() as usize];
        let entries = registry
            .iter()
            .filter(|s| s.step == step)
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            entries.iter().map(|s| s.id.to_string()).collect::<Vec<_>>(),
            case["ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
        );
        let mut chunks = [RegionChunk::new(
            generator.new_chunk([-7, 11]),
            ChunkStatus::Terrain,
        )];
        let mut region = WorldGenRegion::new(
            &mut chunks,
            [-7, 11],
            &[ChunkStatus::Terrain],
            0,
            None,
            &StaticBlocks,
            0,
            63,
        )
        .unwrap();
        for prefix in 0..=entries.len() {
            let mut random = WorldgenRandom::new(Algorithm::Xoroshiro.new_instance(17));
            let seed = random.set_decoration_seed(case["seed"].as_i64().unwrap(), -112, 176);
            JigsawStructureDecoration {
                registry: &entries[..prefix],
                starts: &starts,
                templates: &templates,
                tags: &tags,
                generator: &generator,
                features: &features,
            }
            .place_step(step, &mut region, &mut random, seed)
            .unwrap();
            assert_eq!(
                random.next_int(),
                case["next"][prefix].as_i64().unwrap() as i32,
                "step={step:?} prefix={prefix} seed={}",
                case["seed"]
            );
        }
    }
}
