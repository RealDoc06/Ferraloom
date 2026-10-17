use ferraloom_data::datapack::{Pack, ResourceManager};
use ferraloom_util::random::{
    LegacyRandomSource, RandomSource, WorldgenRandom, XoroshiroRandomSource,
};
use ferraloom_worldgen::{carver::*, placement::providers::*};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};

fn golden(bytes: &[u8]) -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
#[test]
fn all_provider_types_match_vanilla_samples_and_random_consumption() {
    let cases = golden(include_bytes!("golden/providers.json.gz"));
    let context = WorldGenerationContext {
        min_y: -64,
        height: 384,
        sea_level: 63,
    };
    for case in cases.as_array().unwrap() {
        let seed = case["seed"].as_i64().unwrap();
        let mut random = WorldgenRandom::new(XoroshiroRandomSource::new(seed));
        let config = &case["config"];
        let samples = case["samples"].as_array().unwrap();
        match case["type"].as_str().unwrap() {
            "int" => {
                let p = IntProvider::decode(config).unwrap();
                for expected in samples {
                    assert_eq!(
                        i64::from(p.sample(&mut random)),
                        expected.as_i64().unwrap(),
                        "seed={seed} {config}"
                    );
                }
            }
            "float" => {
                let p = FloatProvider::decode(config).unwrap();
                for expected in samples {
                    assert_eq!(
                        u64::from(p.sample(&mut random).to_bits()),
                        expected.as_u64().unwrap(),
                        "seed={seed} {config}"
                    );
                }
            }
            "height" => {
                let p = HeightProvider::decode(config).unwrap();
                for expected in samples {
                    assert_eq!(
                        i64::from(p.sample(&mut random, context)),
                        expected.as_i64().unwrap(),
                        "seed={seed} {config}"
                    );
                }
            }
            _ => panic!("unknown golden provider kind"),
        }
        assert_eq!(
            random.next_long(),
            case["next"].as_i64().unwrap(),
            "random consumption for {config}"
        );
    }
}
fn mask_hash(mask: &CarvingMask) -> String {
    let mut hash = Sha256::new();
    mask.visit(|x, z, bottom, top| {
        for n in [i32::from(x), i32::from(z), bottom, top] {
            hash.update(n.to_be_bytes());
        }
    });
    format!("{:x}", hash.finalize())
}
#[test]
fn every_builtin_carver_mask_matches_vanilla() {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder provides vanilla jar");
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    let registry = CarverRegistry::load(&resources).unwrap();
    let cases = golden(include_bytes!("golden/carvers.json.gz"));
    for case in cases.as_array().unwrap() {
        let seed = case["seed"].as_i64().unwrap();
        let name = case["carver"].as_str().unwrap();
        let carver = &registry.carvers[&name.parse().unwrap()];
        let p = [
            case["chunk"][0].as_i64().unwrap() as i32,
            case["chunk"][1].as_i64().unwrap() as i32,
        ];
        let context = if name.contains("nether") {
            WorldGenerationContext {
                min_y: 0,
                height: 128,
                sea_level: 32,
            }
        } else {
            WorldGenerationContext {
                min_y: -64,
                height: 384,
                sea_level: 63,
            }
        };
        let mut mask = CarvingMask::new(context.min_y + 1, context.min_y + context.height - 8);
        let mut random = WorldgenRandom::new(LegacyRandomSource::new(0));
        for dx in -8..=8 {
            for dz in -8..=8 {
                let source = [p[0] + dx, p[1] + dz];
                random.set_large_feature_seed(seed, source[0], source[1]);
                if carver.is_start_chunk(&mut random) {
                    carver.carve(context, &mut random, p, source, &mut mask);
                }
            }
        }
        assert_eq!(
            mask_hash(&mask),
            case["hash"].as_str().unwrap(),
            "{name} seed={seed} chunk={p:?}"
        );
        assert_eq!(
            random.next_long(),
            case["next"].as_i64().unwrap(),
            "{name} RNG seed={seed}"
        );
    }
}
