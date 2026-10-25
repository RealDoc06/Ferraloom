use super::*;
use crate::loot::{LootItemFunction, SlotSource};
use serde_json::Value;

fn scalar(kind: &str, input: &impl DecodeInput) -> Result<String> {
    Ok(match kind {
        "bool" => boolean(input)?.to_string(),
        "int" => int(input)?.to_string(),
        "long" => long(input)?.to_string(),
        "float" => format!("{:08x}", float(input)?.to_bits()),
        "double" => format!("{:016x}", double(input)?.to_bits()),
        "string" => string(input)?.to_owned(),
        _ => panic!("unknown scalar fixture"),
    })
}
fn vectors() -> Value {
    serde_json::from_str(include_str!("../../../tests/java/loot/input/vectors.json")).unwrap()
}

#[test]
fn native_json_and_nbt_scalar_coercions_remain_format_specific() {
    let vectors = vectors();
    let rows = vectors["primitives"].as_array().unwrap();
    for row in rows {
        let source = row["source"].as_str().unwrap();
        let kind = row["type"].as_str().unwrap();
        let result = match row["format"].as_str().unwrap() {
            "json" => scalar(kind, &serde_json::from_str::<Value>(source).unwrap()),
            "nbt" => scalar(kind, &NbtOps::new(&ferraloom_nbt::parse(source).unwrap())),
            _ => panic!("unknown ops fixture"),
        };
        assert_eq!(
            result.is_ok(),
            row["ok"].as_bool().unwrap(),
            "{row}: {result:?}"
        );
        if let Ok(value) = result {
            assert_eq!(value, row["value"].as_str().unwrap(), "{row}");
        }
    }
    assert_eq!(rows.len(), 192);
}

#[test]
fn native_nbt_modifiers_sources_and_nested_predicates_use_the_shared_schema() {
    let vectors = vectors();
    let rows = vectors["definitions"].as_array().unwrap();
    for row in rows {
        let source = row["source"].as_str().unwrap();
        let input = ferraloom_nbt::parse(source).unwrap();
        let ok = row["ok"].as_bool().unwrap();
        match row["kind"].as_str().unwrap() {
            "modifier" => {
                let actual = LootItemFunction::decode_nbt(&input);
                assert_eq!(actual.is_ok(), ok, "{source}: {actual:?}");
                if let Ok(actual) = actual {
                    let expected = LootItemFunction::decode(&row["canonical_json"]).unwrap();
                    assert_eq!(actual, expected, "{source}");
                }
            }
            "source" => {
                let actual = SlotSource::decode_nbt(&input);
                assert_eq!(actual.is_ok(), ok, "{source}: {actual:?}");
                if let Ok(actual) = actual {
                    let expected = SlotSource::decode(&row["canonical_json"]).unwrap();
                    assert_eq!(actual, expected, "{source}");
                }
            }
            _ => panic!("unknown codec fixture"),
        }
    }
    assert_eq!(rows.len(), 39);
}
