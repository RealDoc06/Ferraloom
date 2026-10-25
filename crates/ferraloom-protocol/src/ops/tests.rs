use crate::component::{CustomData, Persistent};
use ferraloom_nbt::{NbtIo, Tag};
use serde_json::Value;

#[test]
fn native_custom_data_codec_copy_preserves_hash_map_order() {
    let rows: Vec<Value> =
        serde_json::from_str(include_str!("../../tests/java/ops/vectors.json")).unwrap();
    for row in &rows {
        let source = row["source"].as_str().unwrap();
        let input = Tag::Compound(ferraloom_nbt::parse_compound(source).unwrap());
        let value = CustomData::from_nbt(&input).unwrap();
        let encoded = value.to_nbt().unwrap();
        let keys = encoded
            .as_compound()
            .unwrap()
            .iter_java_order()
            .map(|(key, _)| key.to_string_checked().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(serde_json::json!(keys), row["keys"], "{source}");
        let mut bytes = Vec::new();
        NbtIo::write_any_tag(&mut bytes, &encoded).unwrap();
        let wire = bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(wire, row["nbt"].as_str().unwrap(), "{source}");
        assert_eq!(
            value.to_json().unwrap().to_string(),
            row["json"].as_str().unwrap(),
            "{source}"
        );
    }
    assert_eq!(rows.len(), 8);
}
