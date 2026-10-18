use super::*;
use crate::{
    Identifier, Reader, Wire, Writer,
    codec::{RegistryContext, StreamCodec, VarInt},
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
fn vanilla_validated_creative_stacks() {
    let (fixtures, registries) = fixtures();
    for fixture in fixtures["validated_stacks"].as_array().unwrap() {
        let name = fixture["name"].as_str().unwrap();
        let bytes = unhex(fixture["bytes"].as_str().unwrap());
        let accepted = fixture["accepted"].as_bool().unwrap();
        let mut reader = Reader::with_context(&bytes, &registries);
        let stack = OPTIONAL_UNTRUSTED_ITEM_STACK.decode(&mut reader).unwrap();
        reader.finish().unwrap();
        assert_eq!(stack.validate_persistent().is_ok(), accepted, "{name}");
        if !stack.is_empty() {
            // The same persistent encoders constrain storage, JSON and hashes.
            assert_eq!(stack.to_nbt().is_ok(), accepted, "NBT: {name}");
            assert_eq!(stack.to_json().is_ok(), accepted, "JSON: {name}");
            assert_eq!(stack.hash().is_ok(), accepted, "hash: {name}");
        }
        let mut reader = Reader::with_context(&bytes, &registries);
        let result = VALIDATED_OPTIONAL_UNTRUSTED_ITEM_STACK.decode(&mut reader);
        assert_eq!(result.is_ok(), accepted, "{name}: {result:?}");
        reader.finish().unwrap();
        // Vanilla does not validate on the encode path, even for rejected stacks.
        let mut writer = Writer::with_context(&registries);
        VALIDATED_OPTIONAL_UNTRUSTED_ITEM_STACK
            .encode(&mut writer, &stack)
            .unwrap();
        assert_eq!(writer.as_slice(), bytes, "{name}");
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
            if let Some(expected) = fixture.get("nbt").and_then(Value::as_str) {
                let expected = unhex(expected);
                let mut nbt_reader = Reader::new(&expected);
                let tag = nbt_reader.nbt(false).unwrap();
                match DataComponent::from_nbt(&key, &tag) {
                    Ok(restored) if restored == value => {}
                    other => mismatches.push(format!(
                        "{key} persistent NBT decode: {other:?}, expected {value:?}"
                    )),
                }
                let mut writer = Writer::new();
                match value.to_nbt().and_then(|v| writer.nbt(&v)) {
                    Ok(()) if writer.as_slice() == expected => {}
                    other => mismatches.push(format!(
                        "{key} persistent NBT: {other:?}, actual {:x?}, expected {:x?}",
                        writer.as_slice(),
                        expected
                    )),
                }
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
#[test]
fn item_defaults_merge_with_additions_and_explicit_removals() {
    let (f, registries) = fixtures();
    let stack: ItemStack = roundtrip(f["sword"].as_str().unwrap(), &registries);
    let components = stack.components().unwrap();
    assert_eq!(
        components.get(ComponentTypeId::MAX_DAMAGE),
        Some(&DataComponent::MaxDamage(crate::codec::VarInt(1561)))
    );
    assert_eq!(
        components.get(ComponentTypeId::DAMAGE),
        Some(&DataComponent::Damage(crate::codec::VarInt(7)))
    );
    assert!(components.get(ComponentTypeId::ITEM_NAME).is_none());
    assert!(matches!(
        components.get(ComponentTypeId::CUSTOM_DATA),
        Some(DataComponent::CustomData(_))
    ));
}
#[test]
fn vanilla_stack_patch_normalization() {
    let (f, registries) = fixtures();
    let input = unhex(f["redundant_stack_input"].as_str().unwrap());
    let mut r = Reader::with_context(&input, &registries);
    let stack: ItemStack = r.read().unwrap();
    r.finish().unwrap();
    assert!(stack.0.as_ref().unwrap().components.added.is_empty());
    assert!(stack.0.as_ref().unwrap().components.removed.is_empty());
    let mut w = Writer::with_context(&registries);
    w.write(&stack).unwrap();
    assert_eq!(
        w.as_slice(),
        unhex(f["redundant_stack_output"].as_str().unwrap())
    );
}
#[test]
fn vanilla_scalar_untrusted_patch_needs_no_registry_context() {
    let (f, _) = fixtures();
    let bytes = unhex(f["scalar_patch_untrusted"].as_str().unwrap());
    let mut reader = Reader::new(&bytes);
    let patch = DataComponentPatch::decode_delimited(&mut reader).unwrap();
    reader.finish().unwrap();
    assert_eq!(
        patch.added,
        vec![DataComponent::Damage(crate::codec::VarInt(7))]
    );
    let mut writer = Writer::new();
    patch.encode_delimited(&mut writer).unwrap();
    assert_eq!(writer.as_slice(), bytes);
}

/// DFU's real primitive codecs, including Gson NumberLimits and NbtOps conversions.
#[test]
fn vanilla_persistent_primitive_coercions() {
    use ferraloom_nbt::{CompoundTag, ListTag, Tag};
    let fixture: Value = serde_json::from_str(include_str!("primitive-fixtures.json")).unwrap();
    fn json_values(value: &Value) -> Value {
        serde_json::json!({
            "int": i32::from_json(value).ok().map(|n| n.to_string()),
            "long": i64::from_json(value).ok().map(|n| n.to_string()),
            "float": f32::from_json(value).ok().map(|n| format!("{:x}", n.to_bits())),
            "double": f64::from_json(value).ok().map(|n| format!("{:x}", n.to_bits())),
            "bool": bool::from_json(value).ok().map(|n| n.to_string()),
            "string": String::from_json(value).ok(),
        })
    }
    fn nbt_values(value: &Tag) -> Value {
        assert_eq!(
            VarInt::from_nbt(value).ok().map(|n| n.0),
            i32::from_nbt(value).ok()
        );
        serde_json::json!({
            "int": i32::from_nbt(value).ok().map(|n| n.to_string()),
            "long": i64::from_nbt(value).ok().map(|n| n.to_string()),
            "float": f32::from_nbt(value).ok().map(|n| format!("{:x}", n.to_bits())),
            "double": f64::from_nbt(value).ok().map(|n| format!("{:x}", n.to_bits())),
            "bool": bool::from_nbt(value).ok().map(|n| n.to_string()),
            "string": String::from_nbt(value).ok(),
        })
    }
    for row in fixture["json"].as_array().unwrap() {
        let input = row["input"].as_str().unwrap();
        let value = serde_json::from_str(input).unwrap();
        assert_eq!(json_values(&value), row["values"], "JSON {input}");
        assert_eq!(
            VarInt::from_json(&value).ok().map(|n| n.0),
            i32::from_json(&value).ok()
        );
    }
    for row in fixture["nbt"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let value = match name {
            "byte" => Tag::Byte(-1),
            "short" => Tag::Short(257),
            "int" => Tag::Int(i32::MIN),
            "long" => Tag::Long(4_294_967_297),
            "longMax" => Tag::Long(i64::MAX),
            "float" => Tag::Float(-2.9),
            "double" => Tag::Double(-2.9),
            "doubleMax" => Tag::Double(f64::MAX),
            "negativeMax" => Tag::Double(-f64::MAX),
            "nan" => Tag::Double(f64::NAN),
            "floatNan" => Tag::Float(f32::NAN),
            "positiveInfinity" => Tag::Float(f32::INFINITY),
            "negativeInfinity" => Tag::Double(f64::NEG_INFINITY),
            "negativeZero" => Tag::Double(-0.0),
            "fraction" => Tag::Double(0.5),
            "string" => Tag::String("1".into()),
            "compound" => Tag::Compound(CompoundTag::new()),
            "list" => Tag::List(ListTag(vec![])),
            _ => panic!("unknown probe {name}"),
        };
        assert_eq!(nbt_values(&value), row["values"], "NBT {name}");
    }
    for row in fixture["nested"].as_array().unwrap() {
        let input = ferraloom_nbt::parse(row["nbt"].as_str().unwrap()).unwrap();
        let actual = match row["codec"].as_str().unwrap() {
            "fireworks" => Fireworks::from_nbt(&input).unwrap().to_nbt().unwrap(),
            "details" => MobEffectDetails::from_nbt(&input)
                .unwrap()
                .to_nbt()
                .unwrap(),
            "teleport" => {
                let mut input = input;
                input
                    .as_compound_mut()
                    .unwrap()
                    .insert("type", Tag::String("minecraft:teleport_randomly".into()));
                let mut output = ConsumeEffect::from_nbt(&input).unwrap().to_nbt().unwrap();
                output.as_compound_mut().unwrap().remove("type");
                output
            }
            _ => panic!("unknown nested codec"),
        };
        let expected = ferraloom_nbt::parse(row["expected"].as_str().unwrap()).unwrap();
        assert_eq!(actual, expected, "nested {}", row["codec"]);
    }
    // A component nested in a recipe result must use the same primitive codec.
    assert_eq!(
        DataComponent::from_json(
            &Identifier::parse("minecraft:damage").unwrap(),
            &serde_json::from_str("4294967297.9").unwrap()
        )
        .unwrap(),
        DataComponent::Damage(VarInt(1))
    );
    assert!(
        DataComponent::from_json(
            &Identifier::parse("minecraft:enchantment_glint_override").unwrap(),
            &Value::from(1)
        )
        .is_err()
    );
}

#[test]
fn vanilla_persistent_compound_or_snbt_alternatives() {
    use ferraloom_nbt::Tag;
    let fixture: Value = serde_json::from_str(include_str!("primitive-fixtures.json")).unwrap();
    for row in fixture["compounds"].as_array().unwrap() {
        let input: Value = serde_json::from_str(row["input"].as_str().unwrap()).unwrap();
        let actual = CustomData::from_json(&input);
        if let Some(expected) = row["compound"].as_str() {
            let expected = ferraloom_nbt::parse(expected).unwrap();
            assert_eq!(actual.unwrap().0, expected, "{input}");
            let nbt_input = if let Some(text) = input.as_str() {
                Tag::String(text.into())
            } else {
                expected
            };
            assert!(CustomData::from_nbt(&nbt_input).is_ok());
        } else {
            assert!(actual.is_err(), "{input}");
            if let Some(text) = input.as_str() {
                assert!(CustomData::from_nbt(&Tag::String(text.into())).is_err());
            }
        }
        let entity = TypedEntityData::from_json(&input);
        let block_entity = TypedBlockEntityData::from_json(&input);
        if let Some(expected) = row["typed"].as_str() {
            assert_eq!(entity.unwrap().entity_type.0.as_str(), expected);
            assert_eq!(block_entity.unwrap().entity_type.0.as_str(), expected);
            if let Some(text) = input.as_str() {
                assert!(TypedEntityData::from_nbt(&Tag::String(text.into())).is_ok());
                assert!(TypedBlockEntityData::from_nbt(&Tag::String(text.into())).is_ok());
            }
        } else {
            assert!(entity.is_err());
            assert!(block_entity.is_err());
        }
    }
}
