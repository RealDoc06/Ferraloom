use super::*;
use crate::{
    generator::flat::{FlatLevelGeneratorSettings, FlatLevelSource},
    structure::processor::tests::generator,
};
use serde_json::Value;
use std::io::Read;
#[test]
fn registered_structure_generation_matches_vanilla_with_biome_rejection() {
    let (resources, noise) = generator();
    let flat = FlatLevelSource::new(
        noise.data.clone(),
        FlatLevelGeneratorSettings::default_settings(),
    )
    .unwrap();
    let mut registry = StructureRegistry::load(&resources, &noise.data).unwrap();
    // The Java harness supplies a controlled valid-biome predicate instead of
    // each structure's builtin biome set, isolating Structure#generate itself.
    for settings in registry.structures.values_mut() {
        settings.biomes = vec!["minecraft:plains".parse().unwrap()];
    }
    let structures = StructureGenerators::load(&resources, registry, &mut |_| {
        Err(Error("no inline features".into()))
    })
    .unwrap();
    let pools =
        StructurePools::load(&resources, &mut |_| Err(Error("no inline features".into()))).unwrap();
    let templates = StructureTemplateManager::load_all(&resources).unwrap();
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("testdata/structure_generation.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    let cases: Value = serde_json::from_str(&text).unwrap();
    for case in cases.as_array().unwrap() {
        let seed = case["seed"].as_i64().unwrap();
        let chunk = std::array::from_fn(|i| case["chunk"][i].as_i64().unwrap() as i32);
        let accept = case["accept"].as_bool().unwrap();
        let mut biome = |_quart| {
            Ok(if accept {
                "minecraft:plains"
            } else {
                "minecraft:ocean"
            }
            .parse()
            .unwrap())
        };
        let nether = (case["id"] == "minecraft:nether_fossil"
            || case["id"] == "minecraft:end_city")
            .then(|| {
                crate::generator::NoiseBasedChunkGenerator::new(
                    noise.data.clone(),
                    crate::settings::NoiseGeneratorSettings::decode(
                        &resources
                            .require(
                                &if case["id"] == "minecraft:end_city" {
                                    "worldgen/noise_settings/end.json"
                                } else {
                                    "worldgen/noise_settings/nether.json"
                                }
                                .parse()
                                .unwrap(),
                            )
                            .unwrap()
                            .json()
                            .unwrap(),
                    )
                    .unwrap(),
                    crate::biome::BiomeSource::Fixed("minecraft:plains".parse().unwrap()),
                    seed,
                )
                .unwrap()
            });
        let generator: &dyn ChunkGeneratorAccess = match &nether {
            Some(nether) => nether,
            None => &flat,
        };
        let mut context = StructureGenerationContext {
            seed,
            chunk,
            min_y: -64,
            height: 384,
            generator,
            templates: &templates,
            pools: &pools,
            biome: &mut biome,
        };
        let start = structures
            .generate(
                &case["id"].as_str().unwrap().parse().unwrap(),
                7,
                &mut context,
            )
            .unwrap();
        assert_eq!(
            start.to_nbt(chunk).unwrap(),
            ferraloom_nbt::parse_compound(case["nbt"].as_str().unwrap()).unwrap(),
            "{case}"
        );
        let loaded = StructureStart::from_nbt(
            &start.to_nbt(chunk).unwrap(),
            &structures.registry.terrain_settings(),
            &templates,
            &resources,
            &mut |_| Err(Error("no inline feature".into())),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            loaded.to_nbt(chunk).unwrap(),
            ferraloom_nbt::parse_compound(case["loaded"].as_str().unwrap()).unwrap()
        );
        if let Some(bounds) = case.get("bounds") {
            assert_eq!(
                start.bounding_box().unwrap(),
                super::super::BoundingBox::new(
                    std::array::from_fn(|i| bounds[i].as_i64().unwrap() as i32),
                    std::array::from_fn(|i| bounds[i + 3].as_i64().unwrap() as i32)
                )
            );
        }
    }
}

#[derive(Debug)]
struct AsymmetricHeightTerrain<'a>(&'a FlatLevelSource);
impl ChunkGeneratorAccess for AsymmetricHeightTerrain<'_> {
    fn data(&self) -> &crate::generator::GenerationData {
        self.0.data()
    }
    fn biome_source(&self) -> &crate::biome::BiomeSource {
        self.0.biome_source()
    }
    fn min_y(&self) -> i32 {
        self.0.min_y()
    }
    fn gen_depth(&self) -> i32 {
        self.0.gen_depth()
    }
    fn sea_level(&self) -> i32 {
        self.0.sea_level()
    }
    fn get_base_height(
        &self,
        x: i32,
        z: i32,
        _map: Heightmap,
        _min_y: i32,
        _height: i32,
    ) -> Result<i32, Error> {
        Ok(64
            + x.wrapping_mul(3)
                .wrapping_add(z.wrapping_mul(5))
                .rem_euclid(32))
    }
    fn get_base_column(
        &self,
        x: i32,
        z: i32,
        min_y: i32,
        height: i32,
    ) -> Result<Option<crate::generator::NoiseColumn>, Error> {
        self.0.get_base_column(x, z, min_y, height)
    }
}
#[test]
fn oversized_datapack_shipwreck_heights_match_vanilla_asymmetric_corners() {
    let (resources, noise) = generator();
    let flat = FlatLevelSource::new(
        noise.data.clone(),
        FlatLevelGeneratorSettings::default_settings(),
    )
    .unwrap();
    let terrain = AsymmetricHeightTerrain(&flat);
    let mut registry = StructureRegistry::load(&resources, &noise.data).unwrap();
    for settings in registry.structures.values_mut() {
        settings.biomes = vec!["minecraft:plains".parse().unwrap()];
    }
    let structures = StructureGenerators::load(&resources, registry, &mut |_| {
        Err(Error("no inline features".into()))
    })
    .unwrap();
    let pools =
        StructurePools::load(&resources, &mut |_| Err(Error("no inline features".into()))).unwrap();
    let mut templates = StructureTemplateManager::load_all(&resources).unwrap();
    for (id, template) in &mut templates.templates {
        if id.path().starts_with("shipwreck/") {
            *template = Arc::new(super::super::template::StructureTemplate {
                size: [35, 7, 11],
                ..Default::default()
            });
        }
    }
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("testdata/shipwreck_heights.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    let cases: Value = serde_json::from_str(&text).unwrap();
    for case in cases.as_array().unwrap() {
        let chunk = std::array::from_fn(|i| case["chunk"][i].as_i64().unwrap() as i32);
        let mut biome = |_quart| Ok("minecraft:plains".parse().unwrap());
        let mut context = StructureGenerationContext {
            seed: case["seed"].as_i64().unwrap(),
            chunk,
            min_y: -64,
            height: 384,
            generator: &terrain,
            templates: &templates,
            pools: &pools,
            biome: &mut biome,
        };
        let start = structures
            .generate(
                &case["id"].as_str().unwrap().parse().unwrap(),
                7,
                &mut context,
            )
            .unwrap();
        assert_eq!(
            start.to_nbt(chunk).unwrap(),
            ferraloom_nbt::parse_compound(case["nbt"].as_str().unwrap()).unwrap(),
            "{case}"
        );
    }
}
