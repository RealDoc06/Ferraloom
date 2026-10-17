use ferraloom_data::datapack::{Pack, ResourceManager};
use ferraloom_worldgen::{density::*, router::*};
use serde_json::Value;
use std::{io::Read, path::Path, sync::Arc};

fn golden(bytes: &[u8]) -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
#[test]
fn density_functions_and_router_volumes_match_vanilla_bits() {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies real vanilla jar");
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    let registry = Arc::new(DensityRegistry::load(&resources).unwrap());
    let cases = golden(include_bytes!("golden/density.json.gz"));
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
        let settings = resources
            .require(
                &format!("worldgen/noise_settings/{dimension}.json")
                    .parse()
                    .unwrap(),
            )
            .unwrap()
            .json()
            .unwrap();
        let mut compiler = DensityFunctionCompiler::new(
            registry.clone(),
            seed,
            settings["legacy_random_source"].as_bool().unwrap(),
        );
        let router = NoiseRouter::decode(&settings["noise_router"])
            .unwrap()
            .compile(&mut compiler)
            .unwrap();
        for (name, values) in case["functions"].as_object().unwrap() {
            let id = compiler.compile_reference(&name.parse().unwrap()).unwrap();
            let mut context = SamplerContext::uncached();
            for (point, expected) in points.into_iter().zip(values.as_array().unwrap()) {
                let actual = compiler.sample_value(id, &mut context, point);
                assert_eq!(
                    actual.to_bits(),
                    expected.as_u64().unwrap() as u32,
                    "seed={seed} dimension={dimension} function={name} at {point:?}, actual={actual}"
                );
            }
        }
        let mut context = SamplerContext::cached();
        let volume = DensityVolume {
            size: [3, 5, 2],
            min: [-17, -63, 31],
            step: [4, 3, 7],
        };
        for (name, id) in ROUTER_FIELDS.iter().zip(router.functions) {
            let actual = compiler.sample_volume(id, &mut context, volume);
            let expected = case["volumes"][name].as_array().unwrap();
            for (i, (actual, expected)) in actual.into_iter().zip(expected).enumerate() {
                assert_eq!(
                    actual.to_bits(),
                    expected.as_u64().unwrap() as u32,
                    "volume seed={seed} dimension={dimension} {name} at {:?}, actual={actual}",
                    volume.position(i)
                );
            }
        }
    }
}
