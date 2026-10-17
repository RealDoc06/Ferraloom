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
