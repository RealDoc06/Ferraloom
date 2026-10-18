use super::*;
use serde_json::Value;
use std::io::Read;
#[test]
fn reference_set_growth_reload_duplicates_and_iteration_match_vanilla() {
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("testdata/reference_sets.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    for case in serde_json::from_str::<Value>(&text)
        .unwrap()
        .as_array()
        .unwrap()
    {
        let input = case["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect::<Vec<_>>();
        let mut set = if case["from_array"].as_bool().unwrap() {
            StructureReferences::from_array(&input)
        } else {
            StructureReferences::default()
        };
        for (&value, added) in input.iter().zip(case["added"].as_array().unwrap()) {
            assert_eq!(set.insert(value), added.as_bool().unwrap(), "{case}");
            assert!(set.contains(value));
        }
        let expected = case["order"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(set.iter().collect::<Vec<_>>(), expected, "{case}");
        assert_eq!(set.len(), expected.len());
        assert_eq!(set.is_empty(), expected.is_empty());
    }
}
