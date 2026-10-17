use ferraloom_data::datapack::{Pack, ResourceManager, decode_block_state};
use ferraloom_worldgen::{climate::Parameter, density::*, material::*, settings::*};
use serde_json::Value;
use std::{io::Read, path::Path, sync::Arc};

fn assert_float(value: f32, expected: &Value, description: &str) {
    let bits = expected.as_u64().unwrap() as u32;
    if f32::from_bits(bits).is_nan() {
        assert!(value.is_nan(), "{description}: {value}");
    } else {
        assert_eq!(value.to_bits(), bits, "{description}: {value}");
    }
}
#[test]
fn codecs_and_all_density_operations_match_vanilla() {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder provides vanilla jar");
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    let registry = Arc::new(DensityRegistry::load(&resources).unwrap());
    let mut compiler = DensityFunctionCompiler::new(registry, 0, false);
    let mut text = String::new();
    flate2::read::GzDecoder::new(include_bytes!("golden/codecs_and_operations.json.gz").as_slice())
        .read_to_string(&mut text)
        .unwrap();
    let cases: Value = serde_json::from_str(&text).unwrap();
    let points = [
        [0, -64, 0],
        [-1, 7, -17],
        [-17, -63, 31],
        [12345, 93, -54321],
        [-30000000, 319, 29999999],
        [33554431, -2032, -33554432],
        [-4097, 0, 8191],
    ];
    let volume = DensityVolume {
        size: [3, 5, 2],
        min: [-17, -63, 31],
        step: [4, 3, 7],
    };
    for case in cases.as_array().unwrap() {
        let input = &case["input"];
        let expected = case["accepted"].as_bool().unwrap();
        let codec = case["codec"].as_str().unwrap();
        let mut function = None;
        let accepted = match codec {
            "density" => {
                function = DensityFunction::decode(input)
                    .and_then(|f| compiler.compile(&f))
                    .ok();
                function.is_some()
            }
            "noise" => NoiseSpec::decode(input).is_ok(),
            "noise_settings" => NoiseSettings::decode(input).is_ok(),
            "block_state" => {
                let state = decode_block_state(input);
                if expected {
                    assert_eq!(
                        u64::from(state.as_ref().unwrap().id()),
                        case["state"].as_u64().unwrap(),
                        "block state {input}"
                    );
                }
                state.is_ok()
            }
            "parameter" => Parameter::decode(input).is_ok(),
            "anchor" => VerticalAnchor::decode(input).is_ok(),
            "condition" => MaterialCondition::decode(input).is_ok(),
            "material" => MaterialRule::decode(input).is_ok(),
            _ => unreachable!(),
        };
        assert_eq!(accepted, expected, "{codec} acceptance of {input}");
        if let Some(function) = function {
            let range = compiler.range(function);
            assert_float(
                range.min,
                &case["range"][0],
                &format!("minimum range {input}"),
            );
            assert_float(
                range.max,
                &case["range"][1],
                &format!("maximum range {input}"),
            );
            let mut context = SamplerContext::uncached();
            for (point, bits) in points.into_iter().zip(case["values"].as_array().unwrap()) {
                assert_float(
                    compiler.sample_value(function, &mut context, point),
                    bits,
                    &format!("scalar {input} point={point:?}"),
                );
            }
            let mut context = SamplerContext::cached();
            compiler.sample_value(function, &mut context, volume.min);
            let values = compiler.sample_volume(function, &mut context, volume);
            for (index, (value, bits)) in values
                .into_iter()
                .zip(case["volume"].as_array().unwrap())
                .enumerate()
            {
                assert_float(
                    value,
                    bits,
                    &format!("volume {input} point={:?}", volume.position(index)),
                );
            }
            for (point, bits) in [[-17, -63, 31], [-13, -60, 38], [17, -7, 31], [-17, -63, 31]]
                .into_iter()
                .zip(case["cached"].as_array().unwrap())
            {
                assert_float(
                    compiler.sample_value(function, &mut context, point),
                    bits,
                    &format!("cached {input} point={point:?}"),
                );
            }
        }
    }
}
