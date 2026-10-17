use ferraloom_data::datapack::{Pack, ResourceManager};
use ferraloom_worldgen::{
    biome::BiomeSource, climate::SearchContext, density::*, generator::*, router::NoiseRouter,
    settings::NoiseGeneratorSettings,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path, sync::Arc};

fn resources() -> ResourceManager {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder provides vanilla jar");
    ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()])
}
fn golden(bytes: &[u8]) -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn settings(resources: &ResourceManager, dimension: &str) -> Value {
    resources
        .require(
            &format!("worldgen/noise_settings/{dimension}.json")
                .parse()
                .unwrap(),
        )
        .unwrap()
        .json()
        .unwrap()
}
fn source(resources: &ResourceManager, dimension: &str) -> BiomeSource {
    let value = if dimension == "end" {
        json!({"type":"minecraft:the_end"})
    } else {
        json!({"type":"minecraft:multi_noise","preset":format!("minecraft:{dimension}")})
    };
    BiomeSource::decode(&value, resources).unwrap()
}
fn biome_hash(names: impl IntoIterator<Item = String>) -> String {
    let mut hash = Sha256::new();
    for name in names {
        hash.update(name.as_bytes());
        hash.update(b"\n");
    }
    format!("{:x}", hash.finalize())
}
#[test]
fn biome_points_and_quart_grids_match_vanilla() {
    let resources = resources();
    let registry = Arc::new(DensityRegistry::load(&resources).unwrap());
    let cases = golden(include_bytes!("golden/biomes.json.gz"));
    let points = [
        [0, -64, 0],
        [-1, 7, -17],
        [-17, -63, 31],
        [12345, 93, -54321],
        [-30000000, 319, 29999999],
        [33554431, -2032, -33554432],
        [-4097, 0, 8191],
    ];
    for case in cases.as_array().unwrap() {
        let seed = case["seed"].as_i64().unwrap();
        let dimension = case["dimension"].as_str().unwrap();
        let settings = settings(&resources, dimension);
        let mut compiler = DensityFunctionCompiler::new(
            registry.clone(),
            seed,
            settings["legacy_random_source"].as_bool().unwrap(),
        );
        let router = NoiseRouter::decode(&settings["noise_router"])
            .unwrap()
            .compile(&mut compiler)
            .unwrap();
        let source = source(&resources, dimension);
        let mut samplers = SamplerContext::uncached();
        let mut search = SearchContext::default();
        for (point, expected) in points.into_iter().zip(case["samples"].as_array().unwrap()) {
            assert_eq!(
                source
                    .sample(&compiler, router, &mut samplers, &mut search, point)
                    .to_string(),
                expected.as_str().unwrap(),
                "{dimension} seed={seed} quart={point:?}"
            );
        }
        let min = settings["noise"]["min_y"].as_i64().unwrap() as i32;
        let height = settings["noise"]["height"].as_u64().unwrap() as usize;
        let mut samplers = SamplerContext::cached();
        let grid = source.sample_quart_grid(
            &compiler,
            router,
            &mut samplers,
            &mut search,
            [-28, min / 4, 44],
            [4, height / 4, 4],
        );
        assert_eq!(
            biome_hash(grid.into_iter().map(|id| id.to_string())),
            case["grid"].as_str().unwrap(),
            "{dimension} seed={seed} quart grid"
        );
    }
}
fn assert_blocks(chunk: &GeneratedChunk, expected: &Value, description: &str) {
    let mut hash = Sha256::new();
    for z in 0..16 {
        for x in 0..16 {
            for y in chunk.min_y..chunk.min_y + chunk.height {
                hash.update(chunk.block(x, y, z).id().to_be_bytes());
            }
        }
    }
    for ([x, z], runs) in [[0, 0], [7, 11], [15, 15]]
        .into_iter()
        .zip(expected["columns"].as_array().unwrap())
    {
        let values = runs
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|run| {
                std::iter::repeat_n(
                    run[0].as_u64().unwrap() as u16,
                    run[1].as_u64().unwrap() as usize,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(values.len(), chunk.height as usize);
        for (offset, id) in values.into_iter().enumerate() {
            assert_eq!(
                chunk.block(x, chunk.min_y + offset as i32, z).id(),
                id,
                "{description} block=[{x},{},{z}]",
                chunk.min_y + offset as i32
            );
        }
    }
    assert_eq!(
        format!("{:x}", hash.finalize()),
        expected["hash"].as_str().unwrap(),
        "{description} full chunk"
    );
    for (actual, field) in [
        (&chunk.world_surface, "world_surface"),
        (&chunk.ocean_floor, "ocean_floor"),
    ] {
        let expected = expected[field]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap() as i32)
            .collect::<Vec<_>>();
        assert_eq!(actual.as_slice(), expected, "{description} {field}");
    }
}
#[test]
fn noise_and_surface_columns_all_dimensions_match_vanilla() {
    let resources = resources();
    let data = Arc::new(GenerationData::load(&resources).unwrap());
    let cases = golden(include_bytes!("golden/chunks.json.gz"));
    for case in cases.as_array().unwrap() {
        let seed = case["seed"].as_i64().unwrap();
        let dimension = case["dimension"].as_str().unwrap();
        let position = [
            case["chunk"][0].as_i64().unwrap() as i32,
            case["chunk"][1].as_i64().unwrap() as i32,
        ];
        let settings = NoiseGeneratorSettings::decode(&settings(&resources, dimension)).unwrap();
        let generator = NoiseBasedChunkGenerator::new(
            data.clone(),
            settings,
            source(&resources, dimension),
            seed,
        )
        .unwrap();
        let mut chunk = generator.new_chunk(position);
        generator.create_biomes(&mut chunk);
        let mut names = Vec::new();
        for z in 0..4 {
            for x in 0..4 {
                for y in 0..chunk.height / 4 {
                    names.push(
                        data.biome_names[chunk.biome([x, chunk.min_y / 4 + y, z]).0 as usize]
                            .to_string(),
                    );
                }
            }
        }
        assert_eq!(
            biome_hash(names),
            case["biomes"].as_str().unwrap(),
            "{dimension} seed={seed} chunk={position:?} biomes"
        );
        let mut noise = generator.noise_chunk(&chunk);
        generator.fill_from_noise(&mut chunk, &mut noise);
        let description = format!("{dimension} seed={seed} chunk={position:?}");
        assert_blocks(&chunk, &case["noise"], &format!("noise {description}"));
        generator.build_surface(&mut chunk, &mut noise);
        assert_blocks(&chunk, &case["surface"], &format!("surface {description}"));
    }
}
