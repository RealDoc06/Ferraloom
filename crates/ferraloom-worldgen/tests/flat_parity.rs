use ferraloom_data::{
    Block, BlockState,
    datapack::{Pack, ResourceManager},
};
use ferraloom_util::random::WorldgenRandom;
use ferraloom_worldgen::{
    Error, Identifier,
    decoration::{DecorationPlan, GenerationStep, StructureDecoration},
    feature::{Feature, FeatureContext, FeatureLoader, PlacedFeature},
    generator::{flat::*, preset::DebugLevelSource, *},
    region::{ChunkStatus, Heightmap, RegionChunk, WorldGenRegion, WorldgenBlockBehavior},
    settings::DimensionType,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path, sync::Arc};

fn golden(bytes: &[u8]) -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn resources() -> ResourceManager {
    ResourceManager::new(vec![
        Pack::vanilla_jar(Path::new(
            &std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap(),
        ))
        .unwrap(),
    ])
}
fn dimension(resources: &ResourceManager) -> DimensionType {
    DimensionType::decode(
        &resources
            .require(&"dimension_type/overworld.json".parse().unwrap())
            .unwrap()
            .json()
            .unwrap(),
    )
    .unwrap()
}
fn assert_chunk(chunk: &GeneratedChunk, expected: &Value) {
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
        expected["hash"].as_str().unwrap()
    );
    assert_eq!(
        serde_json::to_value(chunk.world_surface.as_slice()).unwrap(),
        expected["world_surface"]
    );
    assert_eq!(
        serde_json::to_value(chunk.ocean_floor.as_slice()).unwrap(),
        expected["ocean_floor"]
    );
}
#[derive(Debug)]
struct UnplacedFeature;
impl Feature for UnplacedFeature {
    fn place(&self, _: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        Err(Error(
            "fixture named features are inspected, never placed".into(),
        ))
    }
}
struct FlatBehavior;
impl WorldgenBlockBehavior for FlatBehavior {
    fn post_process_pos(
        &self,
        _: BlockState,
        _: &WorldGenRegion<'_>,
        _: [i32; 3],
    ) -> Result<Option<[i32; 3]>, Error> {
        Err(Error(
            "Unsupported: dynamic postprocessing in flat fixture".into(),
        ))
    }
}
struct NoStructures;
impl StructureDecoration for NoStructures {
    fn place_step(
        &mut self,
        _: GenerationStep,
        _: &mut WorldGenRegion<'_>,
        _: &mut WorldgenRandom,
        _: i64,
    ) -> Result<(), Error> {
        Ok(())
    }
}

#[test]
fn flat_chunks_steps_columns_and_real_void_features_match_vanilla() {
    let resources = resources();
    let data = Arc::new(GenerationData::load(&resources).unwrap());
    let dimension = dimension(&resources);
    let mut loader = FeatureLoader::new(&resources).unwrap();
    loader.load_biome_features().unwrap();
    let mut base = loader.finish();
    for id in resources.registry("worldgen/placed_feature").keys() {
        base.placed.insert(
            id.clone(),
            Arc::new(PlacedFeature {
                id: Some(id.clone()),
                feature: Arc::new(UnplacedFeature),
                placement: Vec::new(),
            }),
        );
    }
    let cases = golden(include_bytes!("golden/flat_chunks.json.gz"));
    for case in cases.as_array().unwrap() {
        let settings = FlatLevelGeneratorSettings::decode(&case["config"], &resources).unwrap();
        let generator = FlatLevelSource::new(data.clone(), settings).unwrap();
        let layers = generator
            .layers()
            .iter()
            .map(|state| state.map(BlockState::id))
            .collect::<Vec<_>>();
        assert_eq!(
            serde_json::to_value(layers).unwrap(),
            case["layers"],
            "{}",
            case["name"]
        );
        let adjusted = generator.adjust_feature_registry(&base).unwrap();
        let original = generator
            .settings
            .layers
            .iter()
            .flat_map(|layer| {
                std::iter::repeat_n(layer.block.default_state(), layer.height as usize)
            })
            .collect::<Vec<_>>();
        let steps = adjusted.biome_features[&generator.settings.biome]
            .iter()
            .map(|step| {
                step.iter()
                    .map(|id| {
                        if id.namespace() == "ferraloom" && id.path().starts_with("flat_layer/") {
                            let height: usize =
                                id.path().split('/').nth(1).unwrap().parse().unwrap();
                            json!([height, original[height].id()])
                        } else {
                            json!(id.to_string())
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(json!(steps), case["steps"], "{}", case["name"]);
        for query in case["queries"].as_array().unwrap() {
            let min_y = query["bounds"][0].as_i64().unwrap() as i32;
            let height = query["bounds"][1].as_i64().unwrap() as i32;
            assert_eq!(
                i64::from(generator.get_spawn_height(min_y, height)),
                query["spawn"].as_i64().unwrap()
            );
            let heights = Heightmap::ALL.map(|map| {
                generator
                    .get_base_height(37, -21, map, min_y, height)
                    .unwrap()
            });
            assert_eq!(json!(heights), query["heights"]);
            let column = generator
                .get_base_column(37, -21, min_y, height)
                .unwrap()
                .unwrap();
            let values = (min_y - 1..=min_y + height)
                .map(|y| column.get_block(y).id())
                .collect::<Vec<_>>();
            assert_eq!(json!(values), query["column"]);
        }
        for expected in case["chunks"].as_array().unwrap() {
            let p = std::array::from_fn(|i| expected["position"][i].as_i64().unwrap() as i32);
            let chunk = generator.build_terrain(p, dimension.clone());
            assert_chunk(&chunk, &expected["terrain"]);
            if !expected["features"].is_null() {
                let registry = generator.feature_registry(&resources).unwrap();
                let plan =
                    DecorationPlan::new(&registry, &generator.biome_source().possible_biomes())
                        .unwrap();
                let mut chunks = Vec::new();
                for x in p[0] - 1..=p[0] + 1 {
                    for z in p[1] - 1..=p[1] + 1 {
                        chunks.push(RegionChunk::new(
                            generator.build_terrain([x, z], dimension.clone()),
                            ChunkStatus::Terrain,
                        ));
                    }
                }
                let mut region = WorldGenRegion::new(
                    &mut chunks,
                    p,
                    &[ChunkStatus::Terrain; 2],
                    1,
                    None,
                    &FlatBehavior,
                    8675309,
                    -63,
                )
                .unwrap();
                plan.apply_biome_decoration(&mut region, &generator, &registry, &mut NoStructures)
                    .unwrap();
                assert_chunk(
                    &region.chunk(p, ChunkStatus::Empty).unwrap().chunk,
                    &expected["features"],
                );
            }
        }
    }
}

#[test]
fn every_world_preset_and_dedicated_level_type_matches_vanilla() {
    let resources = resources();
    let data = Arc::new(GenerationData::load(&resources).unwrap());
    for case in golden(include_bytes!("golden/flat_codecs.json.gz"))
        .as_array()
        .unwrap()
    {
        let config = &case["config"];
        assert_eq!(
            FlatLevelGeneratorSettings::decode(config, &resources).is_ok(),
            case["success"].as_bool().unwrap(),
            "{config}"
        );
        let partial = FlatLevelGeneratorSettings::decode_partial(config, &resources);
        assert_eq!(
            partial.is_ok(),
            case["partial"].as_bool().unwrap(),
            "{config}"
        );
        if let Ok(settings) = partial {
            assert_eq!(settings.encode(), case["decoded"], "{config}");
        }
    }
    for case in golden(include_bytes!("golden/world_presets.json.gz"))
        .as_array()
        .unwrap()
    {
        let name: Identifier = case["name"].as_str().unwrap().parse().unwrap();
        let preset = WorldPreset::load(&name, &resources, data.clone(), 0).unwrap();
        assert_eq!(preset.definition, case["definition"]);
        assert_eq!(
            preset
                .dimensions
                .dimensions
                .iter()
                .take(3)
                .map(|stem| stem.name.to_string())
                .collect::<Vec<_>>(),
            [
                "minecraft:overworld",
                "minecraft:the_nether",
                "minecraft:the_end"
            ]
        );
    }
    for case in golden(include_bytes!("golden/level_types.json.gz"))
        .as_array()
        .unwrap()
    {
        let preset = WorldPreset::from_server_properties(
            case["level_type"].as_str().unwrap(),
            &json!({}),
            &resources,
            data.clone(),
            0,
        )
        .unwrap();
        assert_eq!(
            preset.definition, case["definition"],
            "{}",
            case["level_type"]
        );
    }
    let inline = resources
        .require(&"worldgen/biome/plains.json".parse().unwrap())
        .unwrap()
        .json()
        .unwrap();
    let settings = json!({"layers":[{"height":1,"block":"stone"}],"biome":inline});
    assert!(
        WorldPreset::from_server_properties("flat", &settings, &resources, data, 0)
            .unwrap_err()
            .0
            .starts_with("Unsupported: inline biome")
    );
    assert!(
        ferraloom_worldgen::biome::BiomeSource::decode(
            &json!({"type":"fixed","biome":settings["biome"]}),
            &resources
        )
        .unwrap_err()
        .0
        .starts_with("Unsupported: inline biome")
    );
}

#[test]
fn debug_state_grid_matches_every_vanilla_display_cell_and_edges() {
    let cases = golden(include_bytes!("golden/debug_grid.json.gz"));
    for (x, column) in cases.as_array().unwrap().iter().enumerate() {
        for (z, state) in column.as_array().unwrap().iter().enumerate() {
            assert_eq!(
                u64::from(
                    DebugLevelSource::block_state_for(x as i32 * 2 - 1, z as i32 * 2 - 1).id()
                ),
                state.as_u64().unwrap()
            );
        }
    }
    assert_eq!(
        DebugLevelSource::block_state_for(0, 1),
        Block::AIR.default_state()
    );
    assert_eq!(
        DebugLevelSource::block_state_for(1, 0),
        Block::AIR.default_state()
    );
    assert_eq!(
        DebugLevelSource::block_state_for(2, 3),
        Block::AIR.default_state()
    );
}
