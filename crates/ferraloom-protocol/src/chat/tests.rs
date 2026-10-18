use super::*;

/// Native translatable argument equality remains distinct from its NBT wire
/// representation, which canonicalizes negative zero through NbtOps.
#[test]
fn native_translation_floating_equality() {
    use crate::component::{DataComponent, DataComponentPatch, ItemStack};

    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../component/float-fixtures.json")).unwrap();
    let rows = fixture["chat"]["rows"].as_array().unwrap();
    let values: Vec<_> = rows
        .iter()
        .map(|row| {
            let input = row["bits"].as_str().unwrap();
            let (kind, bits) = input.split_once(':').unwrap();
            let number = match kind {
                "float" => {
                    Numeric::Float(JavaF32::from_bits(u32::from_str_radix(bits, 16).unwrap()))
                }
                "double" => {
                    Numeric::Double(JavaF64::from_bits(u64::from_str_radix(bits, 16).unwrap()))
                }
                _ => panic!("unknown numeric kind {kind}"),
            };
            let component = Component::translatable("probe.number", vec![Argument::Number(number)]);
            let value = DataComponent::CustomName(Box::new(component.clone()));
            let mut writer = Writer::new();
            value.encode_value(&mut writer).unwrap();
            assert_eq!(
                writer.as_slice(),
                hex(row["wire"].as_str().unwrap()),
                "{input}"
            );
            assert_eq!(
                value.hash().ok().map(i64::from),
                row["hash"].as_i64(),
                "{input} HashOps"
            );
            let mut writer = Writer::new();
            writer.nbt(&value.to_nbt().unwrap()).unwrap();
            assert_eq!(
                writer.as_slice(),
                hex(row["nbt"].as_str().unwrap()),
                "{input} NBT"
            );
            let stack = |count| {
                ItemStack::new(
                    ferraloom_data::Item::STONE,
                    count,
                    DataComponentPatch {
                        added: vec![value.clone()],
                        removed: vec![],
                    },
                )
            };
            (component, stack(1), stack(7))
        })
        .collect();
    for (i, row) in rows.iter().enumerate() {
        let equality = row["equal"].as_str().unwrap().as_bytes();
        let stacks = row["stacks"].as_str().unwrap().as_bytes();
        assert_eq!(equality.len(), values.len());
        assert_eq!(stacks.len(), values.len());
        for (j, other) in values.iter().enumerate() {
            assert_eq!(
                values[i].0 == other.0,
                equality[j] == b'1',
                "{i}/{j} component"
            );
            assert_eq!(
                values[i].1.is_same_item_same_components(&other.2).unwrap(),
                stacks[j] == b'1',
                "{i}/{j} stack"
            );
        }
    }
}

fn hex(text: &str) -> Vec<u8> {
    text.trim()
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn vanilla_network_components_and_nested_styles() {
    for bytes in [
        include_str!("../../tests/fixtures/component_0.hex"),
        include_str!("../../tests/fixtures/component_1.hex"),
        include_str!("../../tests/fixtures/component_2.hex"),
        include_str!("../../tests/fixtures/component_3.hex"),
        include_str!("../../tests/fixtures/component_4.hex"),
        include_str!("../../tests/fixtures/component_5.hex"),
    ] {
        let bytes = hex(bytes);
        let component = Component::from_bytes(&bytes).unwrap();
        assert_eq!(component.to_bytes().unwrap(), bytes, "{component:?}");
    }
}
#[test]
fn vanilla_json_components() {
    for json in [
        include_str!("../../tests/fixtures/component_0.json"),
        include_str!("../../tests/fixtures/component_1.json"),
        include_str!("../../tests/fixtures/component_2.json"),
        include_str!("../../tests/fixtures/component_3.json"),
        include_str!("../../tests/fixtures/component_4.json"),
        include_str!("../../tests/fixtures/component_5.json"),
    ] {
        let component = Component::from_json(json).unwrap();
        let expected: serde_json::Value = serde_json::from_str(json).unwrap();
        let actual: serde_json::Value =
            serde_json::from_str(&component.to_json().unwrap()).unwrap();
        assert_eq!(actual, expected);
    }
}
#[test]
fn lenient_syntax_and_strict_discriminators() {
    let component = Component::from_json_lenient("/*comment*/ {text:'hello'; bold:true}").unwrap();
    assert_eq!(component.contents, Contents::Text("hello".into()));
    assert_eq!(component.style.bold, Some(true));
    assert!(Component::from_json("{text:'hello'}").is_err());
    assert!(Component::from_json(r#"{"type":"unknown","text":"hello"}"#).is_err());
    assert!(Component::from_json(r#"{"text":"hello","extra":[]}"#).is_err());
    assert!(
        Component::from_json(
            r#"{"text":"hello","click_event":{"action":"open_file","path":"file"}}"#
        )
        .is_err()
    );
}

#[test]
fn vanilla_standalone_style_codec() {
    for fixture in [
        include_str!("../../tests/fixtures/style_1.hex"),
        include_str!("../../tests/fixtures/style_2.hex"),
    ] {
        let bytes = hex(fixture);
        let style = Style::from_bytes(&bytes).unwrap();
        assert_eq!(style.to_bytes().unwrap(), bytes);
        assert_eq!(Style::from_json(&style.to_json().unwrap()).unwrap(), style);
    }
    assert_eq!(Style::from_json("{}").unwrap(), Style::default());
    assert!(Style::from_json(r#"{"bold":0}"#).is_err());
}

#[test]
fn vanilla_component_and_style_dynamic_ops() {
    use crate::ops::{HashOps, JsonOps, NbtOps};
    for (json, hash, network) in [
        (
            include_str!("../../tests/fixtures/component_0.json"),
            include_str!("../../tests/fixtures/component_0.hash"),
            include_str!("../../tests/fixtures/component_0.hex"),
        ),
        (
            include_str!("../../tests/fixtures/component_1.json"),
            include_str!("../../tests/fixtures/component_1.hash"),
            include_str!("../../tests/fixtures/component_1.hex"),
        ),
        (
            include_str!("../../tests/fixtures/component_2.json"),
            include_str!("../../tests/fixtures/component_2.hash"),
            include_str!("../../tests/fixtures/component_2.hex"),
        ),
        (
            include_str!("../../tests/fixtures/component_3.json"),
            include_str!("../../tests/fixtures/component_3.hash"),
            include_str!("../../tests/fixtures/component_3.hex"),
        ),
        (
            include_str!("../../tests/fixtures/component_4.json"),
            include_str!("../../tests/fixtures/component_4.hash"),
            include_str!("../../tests/fixtures/component_4.hex"),
        ),
        (
            include_str!("../../tests/fixtures/component_5.json"),
            include_str!("../../tests/fixtures/component_5.hash"),
            include_str!("../../tests/fixtures/component_5.hex"),
        ),
    ] {
        let component = Component::from_json(json).unwrap();
        assert_eq!(
            component.encode_ops(&HashOps).unwrap().0,
            hash.trim().parse::<i32>().unwrap(),
            "{json}"
        );
        assert_eq!(
            component.encode_ops(&JsonOps).unwrap().to_string(),
            json.trim()
        );
        let mut encoded = Writer::new();
        encoded
            .nbt(&component.encode_ops(&NbtOps).unwrap())
            .unwrap();
        assert_eq!(encoded.as_slice(), hex(network));
    }
    for (fixture, hash) in [
        (
            include_str!("../../tests/fixtures/style_1.hex"),
            include_str!("../../tests/fixtures/style_1.hash"),
        ),
        (
            include_str!("../../tests/fixtures/style_2.hex"),
            include_str!("../../tests/fixtures/style_2.hash"),
        ),
    ] {
        let style = Style::from_bytes(&hex(fixture)).unwrap();
        assert_eq!(
            style.encode_ops(&HashOps).unwrap().0,
            hash.trim().parse::<i32>().unwrap()
        );
    }
}
