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
            let mut random = WorldgenRandom::new(Algorithm::Xoroshiro.new_instance(17));
            let seed = random.set_decoration_seed(case["seed"].as_i64().unwrap(), -112, 176);
            RegionStructureDecoration::new(
                &entries[..prefix],
                &templates,
                &tags,
                &generator,
                &features,
            )
            .place_step(step, &mut region, &mut random, seed)
            .unwrap();
            assert_eq!(
                random.next_int(),
                case["next"][prefix].as_i64().unwrap() as i32
            );
        }
    }
}

#[test]
fn live_start_is_restored_with_piece_mutations_when_game_bridge_is_unsupported() {
    use crate::structure::{
        TerrainAdjustment,
        pipeline::ChunkStructures,
        start::{StructurePiece, StructureStart},
        structures::buried_treasure::BuriedTreasurePiece,
    };
    use ferraloom_data::Block;
    let (resources, generator) = generator();
    let templates = StructureTemplateManager::load_all(&resources).unwrap();
    let tags = BlockTags::load(&resources).unwrap();
    let features = FeatureRegistry::default();
    let id: Identifier = "minecraft:buried_treasure".parse().unwrap();
    let mut chunk = generator.new_chunk([0, 0]);
    chunk.set_block(9, 22, 9, Block::STONE.default_state());
    chunk.ocean_floor.fill(26);
    let mut chunk = RegionChunk::new(chunk, ChunkStatus::Terrain);
    chunk.structures = ChunkStructures::new([0, 0]);
    chunk.structures.starts.insert(
        id.clone(),
        StructureStart::new(
            id.clone(),
            ferraloom_data::registries::worldgen_structure_type::Id::from_name(
                "minecraft:buried_treasure",
            )
            .unwrap(),
            [0, 0],
            0,
            TerrainAdjustment::None,
            vec![StructurePiece::BuriedTreasure(BuriedTreasurePiece::new([
                9, 90, 9,
            ]))],
        ),
    );
    chunk
        .structures
        .references
        .entry(id.clone())
        .or_default()
        .insert(0);
    let mut chunks = [chunk];
    let mut region = WorldGenRegion::new(
        &mut chunks,
        [0, 0],
        &[ChunkStatus::Terrain],
        0,
        None,
        &StaticBlocks,
        0,
        63,
    )
    .unwrap();
    let registry = [StructureStep {
        id: id.clone(),
        step: GenerationStep::SurfaceStructures,
    }];
    let mut decoration =
        RegionStructureDecoration::new(&registry, &templates, &tags, &generator, &features);
    let mut random = WorldgenRandom::new(Algorithm::Xoroshiro.new_instance(0));
    let error = decoration
        .place_step(
            GenerationStep::SurfaceStructures,
            &mut region,
            &mut random,
            0,
        )
        .unwrap_err();
    assert!(error.0.contains("RandomizableContainer"));
    let start = &region
        .chunk([0, 0], ChunkStatus::StructureStarts)
        .unwrap()
        .structures
        .starts[&id];
    assert_eq!(start.pieces[0].bounds(), BoundingBox::point([9, 23, 9]));
    assert_eq!(region.block([9, 23, 9]).unwrap().block(), Block::CHEST);
}

#[test]
fn worldgen_region_stream_matches_vanilla_factory_and_cellar_draws() {
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("testdata/region_random.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    let cases: Value = serde_json::from_str(&text).unwrap();
    for case in cases.as_array().unwrap() {
        let compiler = crate::density::DensityFunctionCompiler::new(
            std::sync::Arc::new(crate::density::DensityRegistry::default()),
            case["seed"].as_i64().unwrap(),
            case["legacy"].as_bool().unwrap(),
        );
        let chunk = std::array::from_fn(|i| case["chunk"][i].as_i64().unwrap() as i32);
        let mut random = region_random(&compiler, chunk);
        for sample in case["values"].as_array().unwrap() {
            assert_eq!(random.next_boolean(), sample["variant"].as_bool().unwrap());
            for expected in sample["roof"].as_array().unwrap() {
                assert_eq!(
                    u64::from(random.next_float().to_bits()),
                    expected.as_u64().unwrap()
                );
            }
        }
        assert_eq!(random.next_int(), case["next"].as_i64().unwrap() as i32);
    }
}
