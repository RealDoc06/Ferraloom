use super::*;
use ferraloom_util::random::LegacyRandomSource;
use std::{io::Read, path::Path};
fn point(v: &Value) -> [i32; 3] {
    std::array::from_fn(|i| v[i].as_i64().unwrap() as i32)
}
#[test]
fn all_rule_and_position_predicates_match_vanilla_and_random_consumption() {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder provides jar");
    let resources = ResourceManager::new(vec![
        ferraloom_data::datapack::Pack::vanilla_jar(Path::new(&jar)).unwrap(),
    ]);
    let tags = BlockTags::load(&resources).unwrap();
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("testdata/rules.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    let cases: Value = serde_json::from_str(&text).unwrap();
    for case in cases["blocks"].as_array().unwrap() {
        let test = RuleTest::decode(&case["definition"]).unwrap();
        let mut random = LegacyRandomSource::new(case["seed"].as_i64().unwrap());
        for sample in case["results"].as_array().unwrap() {
            let state = BlockState::from_id(sample["state"].as_u64().unwrap() as u16).unwrap();
            assert_eq!(
                test.test(state, point(&sample["pos"]), &tags, &mut random),
                sample["result"].as_bool().unwrap(),
                "{test:?} {sample}"
            );
            assert_eq!(
                random.next_int(),
                sample["next"].as_i64().unwrap() as i32,
                "{test:?} RNG after {sample}"
            );
        }
    }
    for case in cases["positions"].as_array().unwrap() {
        let test = PosRuleTest::decode(&case["definition"]).unwrap();
        let mut random = LegacyRandomSource::new(case["seed"].as_i64().unwrap());
        for sample in case["results"].as_array().unwrap() {
            assert_eq!(
                test.test(
                    point(&sample["pos"]),
                    point(&sample["reference"]),
                    &mut random
                ),
                sample["result"].as_bool().unwrap(),
                "{test:?} {sample}"
            );
            assert_eq!(
                random.next_int(),
                sample["next"].as_i64().unwrap() as i32,
                "{test:?} RNG after {sample}"
            );
        }
    }
}
