use super::*;
use crate::{
    Identifier, Reader, Wire, Writer,
    codec::{RegistryContext, StreamCodec},
    entity_data::EntityDataItems,
};
use serde_json::Value;

pub(crate) struct Registries(Value);
impl RegistryContext for Registries {
    fn entry_id(&self, registry: &Identifier, entry: &Identifier) -> Option<i32> {
        self.0
            .get(registry.as_str())?
            .as_array()?
            .iter()
            .position(|v| v.as_str() == Some(entry.as_str()))
            .map(|id| id as i32)
    }
    fn entry_key(&self, registry: &Identifier, id: i32) -> Option<Identifier> {
        Identifier::parse(
            self.0
                .get(registry.as_str())?
                .as_array()?
                .get(usize::try_from(id).ok()?)?
                .as_str()?,
        )
        .ok()
    }
    fn has_tag(&self, _: &Identifier, _: &Identifier) -> bool {
        true
    }
}
pub(crate) fn fixtures() -> (Value, Registries) {
    let value: Value = serde_json::from_str(include_str!("fixtures.json")).unwrap();
    let registries = Registries(value["registries"].clone());
    (value, registries)
}
pub(crate) fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
fn roundtrip<T: Wire>(hex: &str, registries: &Registries) -> T {
    let bytes = unhex(hex);
    let mut r = Reader::with_context(&bytes, registries);
    let value: T = r.read().unwrap();
    r.finish().unwrap();
    let mut w = Writer::with_context(registries);
    w.write(&value).unwrap();
    assert_eq!(w.as_slice(), bytes);
    value
}
#[test]
fn vanilla_component_streams() {
    let (fixtures, registries) = fixtures();
    for fixture in fixtures["components"].as_array().unwrap() {
        let key = Identifier::parse(fixture["type"].as_str().unwrap()).unwrap();
        let bytes = unhex(fixture["bytes"].as_str().unwrap());
        let mut r = Reader::with_context(&bytes, &registries);
        let value =
            DataComponent::decode_value(&key, &mut r).unwrap_or_else(|e| panic!("{key}: {e}"));
        r.finish().unwrap_or_else(|e| panic!("{key}: {e}"));
        let mut w = Writer::with_context(&registries);
        value.encode_value(&mut w).unwrap();
        assert_eq!(w.as_slice(), bytes, "{key}");
    }
}
#[test]
fn vanilla_stacks_removals_nesting_and_untrusted_delimiters() {
    let (f, registries) = fixtures();
    for name in ["empty", "stone", "sword", "nested"] {
        let stack: ItemStack = roundtrip(f[name].as_str().unwrap(), &registries);
        let bytes = unhex(f[format!("{name}_untrusted")].as_str().unwrap());
        let mut r = Reader::with_context(&bytes, &registries);
        let untrusted = OPTIONAL_UNTRUSTED_ITEM_STACK.decode(&mut r).unwrap();
        r.finish().unwrap();
        assert_eq!(stack, untrusted);
        let mut w = Writer::with_context(&registries);
        OPTIONAL_UNTRUSTED_ITEM_STACK
            .encode(&mut w, &stack)
            .unwrap();
        assert_eq!(w.as_slice(), bytes);
        let _: HashedStack = roundtrip(f[format!("{name}_hashed")].as_str().unwrap(), &registries);
    }
}
#[test]
fn vanilla_entity_data_list() {
    let (f, registries) = fixtures();
    let bytes = unhex(f["entity_data"].as_str().unwrap());
    let mut r = Reader::with_context(&bytes, &registries);
    assert_eq!(r.var_int().unwrap(), 123);
    let items: EntityDataItems = r.read().unwrap();
    r.finish().unwrap();
    let mut w = Writer::with_context(&registries);
    w.var_int(123);
    w.write(&items).unwrap();
    assert_eq!(w.as_slice(), bytes);
    assert_eq!(items.0.len(), 9);
}
#[test]
fn empty_stack_and_template_rules() {
    assert!(crate::codec::StreamCodec::decode(&ITEM_STACK, &mut Reader::new(&[0])).is_err());
    assert!(
        ItemStackTemplate::new(ferraloom_data::Item::AIR, 1, DataComponentPatch::default())
            .is_err()
    );
    assert!(
        ItemStackTemplate::new(
            ferraloom_data::Item::STONE,
            -1,
            DataComponentPatch::default()
        )
        .is_ok()
    );
}
#[test]
fn vanilla_persistent_component_hashes() {
    let (fixtures, registries) = fixtures();
    let mut mismatches = Vec::new();
    for fixture in fixtures["components"].as_array().unwrap() {
        let key = Identifier::parse(fixture["type"].as_str().unwrap()).unwrap();
        let bytes = unhex(fixture["bytes"].as_str().unwrap());
        let mut r = Reader::with_context(&bytes, &registries);
        let value = DataComponent::decode_value(&key, &mut r).unwrap();
        if let Some(expected) = fixture.get("hash") {
            match value.hash() {
                Ok(actual) if i64::from(actual) == expected.as_i64().unwrap() => {}
                other => mismatches.push(format!(
                    "{key} hash: {other:?}, expected {expected}, persistent {:?}",
                    value.to_json()
                )),
            }
            match DataComponent::from_json(&key, &fixture["value"]) {
                Ok(restored)
                    if matches!(value, DataComponent::CustomData(_)) || value == restored => {}
                other => mismatches.push(format!(
                    "{key} persistent decode: {other:?}, expected {value:?}"
                )),
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
#[test]
fn all_extracted_item_defaults_are_typed() {
    for &item in ferraloom_data::registries::item::ALL {
        let defaults = DataComponentMap::from_item_defaults(item)
            .unwrap_or_else(|e| panic!("{}: {e}", item.name()));
        assert!(!defaults.entries.is_empty(), "{}", item.name());
    }
}
#[test]
fn vanilla_hashed_stack_validation() {
    let (f, registries) = fixtures();
    for name in ["empty", "stone", "sword", "nested"] {
        let stack: ItemStack = roundtrip(f[name].as_str().unwrap(), &registries);
        let hashed: HashedStack =
            roundtrip(f[format!("{name}_hashed")].as_str().unwrap(), &registries);
        assert!(hashed.matches(&stack).unwrap(), "{name}");
        assert!(
            HashedStack::create(&stack)
                .unwrap()
                .matches(&stack)
                .unwrap()
        );
    }
}

#[test]
fn every_vanilla_entity_data_serializer() {
    let (f, registries) = fixtures();
    let bytes = unhex(f["all_entity_data"].as_str().unwrap());
    let mut r = Reader::with_context(&bytes, &registries);
    assert_eq!(r.var_int().unwrap(), 42);
    let values: EntityDataItems = r.read().unwrap();
    r.finish().unwrap();
    assert_eq!(values.0.len(), 44);
    for (i, v) in values.0.iter().enumerate() {
        assert_eq!(v.id as usize, i);
        assert_eq!(v.value.serializer() as usize, i);
    }
    let mut w = Writer::with_context(&registries);
    w.var_int(42);
    w.write(&values).unwrap();
    assert_eq!(w.as_slice(), bytes);
}
#[test]
fn vanilla_hash_ops_scalar_kinds_arrays_and_map_order() {
    use crate::ops::{EncodeOps, HashOps};
    let (f, _) = fixtures();
    let ops = HashOps;
    let hashes = vec![
        ops.empty(),
        ops.create_boolean(false),
        ops.create_boolean(true),
        ops.create_byte(-5),
        ops.create_short(1000),
        ops.create_int(-300),
        ops.create_long(12345678901234),
        ops.create_float(1.25),
        ops.create_double(-1.75),
        ops.create_string("hello 🌍"),
        ops.create_byte_list(&[-1, 2, 3]),
        ops.create_int_list(&[-1, 2, 300]),
        ops.create_long_list(&[-1, 2, 300]),
        ops.create_list(vec![ops.create_int(1), ops.create_int(2)]),
        ops.create_map(vec![
            (ops.create_string("x"), ops.create_float(1.5)),
            (ops.create_string("y"), ops.create_boolean(true)),
        ])
        .unwrap(),
    ];
    assert_eq!(
        hashes.iter().map(|h| i64::from(h.0)).collect::<Vec<_>>(),
        f["hash_ops"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect::<Vec<_>>()
    );
}
