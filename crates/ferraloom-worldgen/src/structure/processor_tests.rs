use super::*;
use crate::structure::template::PlacementRandom;
use crate::{
    biome::BiomeSource,
    generator::{GenerationData, NoiseBasedChunkGenerator},
    region::{ChunkStatus, RegionChunk, WorldgenBlockBehavior},
    settings::NoiseGeneratorSettings,
};
use ferraloom_data::datapack::{Pack, ResourceManager};
use ferraloom_util::random::{Algorithm, Random, WorldgenRandom};
use std::{io::Read, path::Path, sync::Arc};
fn golden(bytes: &[u8]) -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn point(v: &Value) -> [i32; 3] {
    std::array::from_fn(|i| v[i].as_i64().unwrap() as i32)
}
pub(crate) fn generator() -> (ResourceManager, NoiseBasedChunkGenerator) {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder provides jar");
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    let data = Arc::new(GenerationData::load(&resources).unwrap());
    let definition = resources
        .require(&"worldgen/noise_settings/overworld.json".parse().unwrap())
        .unwrap()
        .json()
        .unwrap();
    let generator = NoiseBasedChunkGenerator::new(
        data,
        NoiseGeneratorSettings::decode(&definition).unwrap(),
        BiomeSource::Fixed("plains".parse().unwrap()),
        0,
    )
    .unwrap();
    (resources, generator)
}
pub(crate) struct StaticBlocks;
impl WorldgenBlockBehavior for StaticBlocks {
    fn has_block_entity(&self, _state: BlockState) -> bool {
        false
    }
    fn post_process_pos(
        &self,
        _state: BlockState,
        _region: &WorldGenRegion<'_>,
        _p: [i32; 3],
    ) -> Result<Option<[i32; 3]>, Error> {
        Ok(None)
    }
    fn is_outline_full_block(
        &self,
        _region: &WorldGenRegion<'_>,
        state: BlockState,
        _p: [i32; 3],
    ) -> Result<bool, Error> {
        if state.data().context_dependent_shape {
            return Err(Error("test only covers static outlines".into()));
        }
        Ok(state
            .data()
            .outline
            .boxes()
            .iter()
            .any(|b| b.min == [0.0; 3] && b.max == [1.0; 3]))
    }
}
#[test]
fn per_block_processors_and_modifiers_match_vanilla_with_rng() {
    let (resources, generator) = generator();
    let tags = BlockTags::load(&resources).unwrap();
    let cases = golden(include_bytes!("testdata/processors.json.gz"));
    for case in cases["blocks"].as_array().unwrap() {
        let processor = StructureProcessor::decode(&case["definition"]).unwrap();
        let mut settings = StructurePlaceSettings::default();
        if case["external"].as_bool().unwrap() {
            settings.random =
                PlacementRandom::Fixed(Random::Legacy(LegacyRandomSource::new(12345)));
        }
        let mut chunks = [[-1, -1], [-1, 0], [0, -1], [0, 0]]
            .map(|p| RegionChunk::new(generator.new_chunk(p), ChunkStatus::Terrain));
        for chunk in &mut chunks {
            chunk.chunk.world_surface = [67; 256];
        }
        if case["lava"].as_bool().unwrap() {
            for sample in case["samples"].as_array().unwrap() {
                let p = point(&sample["pos"]);
                let chunk = chunks
                    .iter_mut()
                    .find(|c| c.chunk.position == [p[0] >> 4, p[2] >> 4])
                    .unwrap();
                chunk.chunk.set_block(
                    (p[0] & 15) as u8,
                    p[1],
                    (p[2] & 15) as u8,
                    Block::LAVA.default_state(),
                );
            }
        }
        for chunk in &mut chunks {
            chunk.chunk.world_surface = [67; 256];
        }
        let region = WorldGenRegion::new(
            &mut chunks,
            [0, 0],
            &[ChunkStatus::Terrain; 2],
            1,
            None,
            &StaticBlocks,
            99,
            63,
        )
        .unwrap();
        let mut shared = WorldgenRandom::new(Algorithm::Legacy.new_instance(0));
        for sample in case["samples"].as_array().unwrap() {
            let p = point(&sample["pos"]);
            let state = BlockState::from_id(sample["state"].as_u64().unwrap() as u16).unwrap();
            let nbt = ferraloom_nbt::parse_compound(
                "{final_state:'minecraft:oak_stairs[facing=east,half=top]',marker:17}",
            )
            .unwrap();
            let info = StructureBlockInfo {
                position: p,
                state,
                nbt: Some(nbt),
            };
            let result = processor
                .process_block(
                    &region,
                    [7, 64, 7],
                    [3, 65, 4],
                    [1, 2, 3],
                    info,
                    &mut settings,
                    &tags,
                    &mut shared,
                )
                .unwrap();
            assert_eq!(
                result.as_ref().map_or(-1, |v| i32::from(v.state.id())),
                sample["result"].as_i64().unwrap() as i32,
                "{processor:?} external={} sample={sample}",
                case["external"]
            );
            if let Some(result) = result {
                assert_eq!(result.position, point(&sample["result_pos"]));
                assert_eq!(
                    result.nbt,
                    sample["nbt"]
                        .as_str()
                        .map(|s| ferraloom_nbt::parse_compound(s).unwrap()),
                    "{processor:?} nbt {sample}"
                );
            }
        }
        let next = match &mut settings.random {
            PlacementRandom::Fixed(random) => random.next_int(),
            _ => LegacyRandomSource::new(12345).next_int(),
        };
        assert_eq!(
            next,
            case["next"].as_i64().unwrap() as i32,
            "{processor:?} settings RNG"
        );
    }
    for case in cases["modifiers"].as_array().unwrap() {
        let modifier = RuleBlockEntityModifier::decode(&case["definition"]).unwrap();
        let mut random = LegacyRandomSource::new(99);
        let tag = case["existing"]
            .as_bool()
            .unwrap()
            .then(|| ferraloom_nbt::parse_compound("{nested:{a:3,c:9},old:1}").unwrap());
        assert_eq!(
            modifier.apply(&mut random, tag),
            case["result"]
                .as_str()
                .map(|s| ferraloom_nbt::parse_compound(s).unwrap()),
            "{modifier:?}"
        );
        assert_eq!(random.next_int(), case["next"].as_i64().unwrap() as i32);
    }
}
#[test]
fn every_builtin_processor_list_decodes() {
    let (resources, _) = generator();
    for (id, resource) in resources.registry("worldgen/processor_list") {
        StructureProcessor::decode_list(&resource.json().unwrap())
            .unwrap_or_else(|e| panic!("{id}: {e}"));
    }
}

#[test]
fn jigsaw_replacement_parser_matches_vanilla_prefix_and_properties() {
    for case in golden(include_bytes!("testdata/replacements.json.gz"))
        .as_array()
        .unwrap()
    {
        assert_eq!(
            parse_jigsaw_replacement(case["input"].as_str().unwrap()).map(BlockState::id),
            case["state"].as_u64().map(|v| v as u16),
            "{case}"
        );
    }
}
