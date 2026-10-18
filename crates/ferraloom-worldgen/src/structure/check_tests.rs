use super::*;
use crate::structure::processor::tests::generator;
use serde_json::{Value, json};
use std::io::Read;

#[test]
fn structure_checks_storage_and_probe_cache_lifecycle_match_vanilla() {
    let (resources, _) = generator();
    let registered = resources
        .registry("worldgen/structure")
        .into_keys()
        .collect();
    let allowed = StructurePlacement::DimensionOrigin;
    let rejected = StructurePlacement::decode(&json!({"type":"minecraft:random_spread","spacing":32,"separation":8,"salt":7,"frequency":0})).unwrap();
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("testdata/structure_checks.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    for case in serde_json::from_str::<Value>(&text)
        .unwrap()
        .as_array()
        .unwrap()
    {
        let mut check = StructureCheck::default();
        let mut scans = 0;
        let mut creates = 0;
        let position = [-7, 11];
        let tag = ferraloom_nbt::parse_compound(case["tag"].as_str().unwrap()).unwrap();
        for action in case["actions"].as_array().unwrap() {
            match action["op"].as_str().unwrap() {
                "load" => check.on_structure_load(
                    position,
                    action["counts"]
                        .as_object()
                        .unwrap()
                        .iter()
                        .filter_map(|(id, count)| {
                            Some((id.parse().unwrap(), count.as_i64()? as i32))
                        })
                        .collect(),
                ),
                "increment" => check.increment_reference(
                    position,
                    &action["id"].as_str().unwrap().parse().unwrap(),
                ),
                "query" => {
                    let id = action["id"].as_str().unwrap().parse().unwrap();
                    let result = check
                        .check_start(
                            position,
                            &id,
                            if action["reject"].as_bool().unwrap() {
                                &rejected
                            } else {
                                &allowed
                            },
                            99,
                            action["require"].as_bool().unwrap(),
                            &registered,
                            &mut |_| {
                                scans += 1;
                                match case["mode"].as_str().unwrap() {
                                    "failed" => StructureScan::Failed,
                                    "missing" => StructureScan::Missing,
                                    _ => StructureScan::Current(tag.clone()),
                                }
                            },
                            &mut |_, id| {
                                creates += 1;
                                Ok(id.to_string() == "test:possible")
                            },
                        )
                        .unwrap();
                    assert_eq!(
                        match result {
                            StructureCheckResult::StartPresent => "START_PRESENT",
                            StructureCheckResult::StartNotPresent => "START_NOT_PRESENT",
                            StructureCheckResult::ChunkLoadNeeded => "CHUNK_LOAD_NEEDED",
                        },
                        action["result"].as_str().unwrap(),
                        "{case}: {action}"
                    );
                }
                _ => unreachable!(),
            }
            assert_eq!(
                scans,
                action["scans"].as_i64().unwrap(),
                "scan calls: {action}"
            );
            assert_eq!(
                creates,
                action["creates"].as_i64().unwrap(),
                "probe calls: {action}"
            );
        }
    }
}
