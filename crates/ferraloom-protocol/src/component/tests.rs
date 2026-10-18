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
    fn encoded_values(float: Option<f32>, double: Option<f64>) -> Value {
        serde_json::json!({
            "float_nbt": float.map(|n| match n.to_nbt().unwrap() {
                Tag::Float(value) => format!("{:x}",value.to_bits()),
                _ => panic!("float codec must emit FloatTag"),
            }),
            "float_hash": float.map(|n| n.hash().unwrap().to_string()),
            "double_nbt": double.map(|n| match n.to_nbt().unwrap() {
                Tag::Double(value) => format!("{:x}",value.to_bits()),
                _ => panic!("double codec must emit DoubleTag"),
            }),
            "double_hash": double.map(|n| n.hash().unwrap().to_string()),
        })
    }
    for row in fixture["json"].as_array().unwrap() {
        let input = row["input"].as_str().unwrap();
        let value = serde_json::from_str(input).unwrap();
        let label = format!(
            "JSON {} ({} chars)",
            input.chars().take(80).collect::<String>(),
            input.len()
        );
        assert_eq!(json_values(&value), row["values"], "{label}");
        assert_eq!(
            encoded_values(f32::from_json(&value).ok(), f64::from_json(&value).ok()),
            row["encoded"],
            "{label} encoding"
        );
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
            "negativeNan" => Tag::Double(f64::from_bits(0xfff8_0000_0000_0001)),
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
        assert_eq!(
            encoded_values(f32::from_nbt(&value).ok(), f64::from_nbt(&value).ok()),
            row["encoded"],
            "NBT {name} encoding"
        );
    }
    for row in fixture["nested"].as_array().unwrap() {
        let input = ferraloom_nbt::parse(row["nbt"].as_str().unwrap()).unwrap();
        let actual = match row["codec"].as_str().unwrap() {
            "fireworks" => Fireworks::from_nbt(&input).unwrap().to_nbt().unwrap(),
            "details" => MobEffectDetails::from_nbt(&input)
                .unwrap()
                .to_nbt()
                .unwrap(),
            "mob_effects" => MobEffectsPredicate::from_nbt(&input)
                .unwrap()
                .to_nbt()
                .unwrap(),
            "int_bounds" => IntBounds::from_nbt(&input).unwrap().to_nbt().unwrap(),
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
            codec => panic!("unknown nested codec {codec}"),
        };
        let expected = ferraloom_nbt::parse(row["expected"].as_str().unwrap()).unwrap();
        assert_eq!(actual, expected, "nested {}", row["codec"]);
    }
    for row in fixture["collection_shapes"].as_array().unwrap() {
        let text = row["input"].as_str().unwrap();
        let json: Value = serde_json::from_str(text).unwrap();
        let nbt = if json.is_null() {
            Tag::End
        } else {
            ferraloom_nbt::parse(text).unwrap()
        };
        assert_eq!(
            CollectionPredicate::<MobEffectsPredicate>::from_json(&json).is_ok(),
            row["json_accepted"].as_bool().unwrap(),
            "collection JSON {text}"
        );
        assert_eq!(
            CollectionPredicate::<MobEffectsPredicate>::from_nbt(&nbt).is_ok(),
            row["nbt_accepted"].as_bool().unwrap(),
            "collection NBT {text}"
        );
    }
    for row in fixture["double_bounds"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let input = match name {
            "negative_nan" => Tag::Double(f64::from_bits(0xfff8_0000_0000_0001)),
            "infinity" => Tag::Double(f64::INFINITY),
            name => {
                let mut tag = CompoundTag::new();
                tag.insert(
                    "min",
                    Tag::Double(match name {
                        "signed_zero" => -0.0,
                        "nan_min" => f64::NAN,
                        "swapped" => 2.0,
                        _ => 0.0,
                    }),
                );
                tag.insert(
                    "max",
                    Tag::Double(if name == "nan_max" { f64::NAN } else { 0.0 }),
                );
                Tag::Compound(tag)
            }
        };
        let decoded = DoubleBounds::from_nbt(&input);
        assert_eq!(
            decoded.is_ok(),
            row["accepted"].as_bool().unwrap(),
            "{name}"
        );
        if let Ok(value) = decoded {
            let bits = |n: JavaF64| {
                format!(
                    "{:x}",
                    if n.0.is_nan() {
                        f64::NAN.to_bits()
                    } else {
                        n.to_bits()
                    }
                )
            };
            assert_eq!(
                value.min.map(bits).as_deref(),
                row["min"].as_str(),
                "{name} min"
            );
            assert_eq!(
                value.max.map(bits).as_deref(),
                row["max"].as_str(),
                "{name} max"
            );
            assert_eq!(
                u64::from(value.to_nbt().unwrap().id()),
                row["tag_id"].as_u64().unwrap(),
                "{name} encoding shape"
            );
        }
    }
    assert!(IntBounds::from_nbt(&ferraloom_nbt::parse("{min:2,max:1}").unwrap()).is_err());
    assert!(IntBounds::from_json(&serde_json::json!({"min":2,"max":1})).is_err());
    for input in [
        Tag::String("#minecraft:test".into()),
        Tag::List(ListTag(vec![Tag::String("minecraft:pig".into())])),
        Tag::IntArray(vec![]),
    ] {
        assert!(crate::codec::HolderSet::from_nbt(&input).is_ok());
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
            assert_eq!(
                NbtPredicate::from_nbt(&expected).unwrap().0.0,
                expected,
                "native predicate numeric tag kinds"
            );
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

/// Native record equality/hashCode and ItemStack#isSameItemSameComponents,
/// independent of the encoder cache's separate value-equality behavior.
#[test]
fn native_component_float_equality_and_hashes() {
    use ferraloom_util::collections::JavaHash;

    let fixture: Value = serde_json::from_str(include_str!("float-fixtures.json")).unwrap();
    let (_, registries) = fixtures();
    for group in fixture["groups"].as_array().unwrap() {
        let name = group["type"].as_str().unwrap();
        let key = Identifier::parse(group["component"].as_str().unwrap()).unwrap();
        let rows = group["rows"].as_array().unwrap();
        let values: Vec<_> = rows
            .iter()
            .map(|row| {
                let bytes = unhex(row["wire"].as_str().unwrap());
                let mut reader = Reader::with_context(&bytes, &registries);
                let value = DataComponent::decode_value(&key, &mut reader).unwrap();
                reader.finish().unwrap();
                let mut writer = Writer::with_context(&registries);
                value.encode_value(&mut writer).unwrap();
                assert_eq!(writer.as_slice(), bytes, "{name} {} wire", row["bits"]);
                let record_hash = match &value {
                    DataComponent::Food(value) => Some(value.java_hash_code()),
                    DataComponent::UseEffects(value) => Some(value.java_hash_code()),
                    DataComponent::AttackRange(value) => Some(value.java_hash_code()),
                    _ => None,
                };
                assert_eq!(
                    record_hash.map(i64::from),
                    row["record_hash"].as_i64(),
                    "{name} {} record hash",
                    row["bits"]
                );
                assert_eq!(
                    value.hash().ok().map(i64::from),
                    row["hash"].as_i64(),
                    "{name} {} HashOps",
                    row["bits"]
                );
                if let Some(expected) = row["nbt"].as_str() {
                    let mut writer = Writer::new();
                    writer.nbt(&value.to_nbt().unwrap()).unwrap();
                    assert_eq!(
                        writer.as_slice(),
                        unhex(expected),
                        "{name} {} NBT",
                        row["bits"]
                    );
                } else {
                    assert!(value.to_nbt().is_err(), "{name} {} NBT", row["bits"]);
                }
                value
            })
            .collect();
        let stacks = |count| {
            values
                .iter()
                .map(|value| {
                    ItemStack::new(
                        ferraloom_data::Item::STONE,
                        count,
                        DataComponentPatch {
                            added: vec![value.clone()],
                            removed: vec![],
                        },
                    )
                })
                .collect::<Vec<_>>()
        };
        let left = stacks(1);
        let right = stacks(7);
        for (i, row) in rows.iter().enumerate() {
            let equality = row["equal"].as_str().unwrap().as_bytes();
            let stacks = row["stacks"].as_str().unwrap().as_bytes();
            assert_eq!(equality.len(), values.len());
            assert_eq!(stacks.len(), values.len());
            for (j, other) in values.iter().enumerate() {
                assert_eq!(
                    &values[i] == other,
                    equality[j] == b'1',
                    "{name} {i}/{j} equality"
                );
                assert_eq!(
                    left[i].is_same_item_same_components(&right[j]).unwrap(),
                    stacks[j] == b'1',
                    "{name} {i}/{j} stacks"
                );
            }
        }
    }
    let rows = fixture["bounds"].as_array().unwrap();
    let values: Vec<_> = rows
        .iter()
        .map(|row| {
            let value =
                JavaF64::from_bits(u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap());
            DoubleBounds {
                min: Some(value),
                max: Some(value),
            }
        })
        .collect();
    for (i, row) in rows.iter().enumerate() {
        for (j, expected) in row["equal"].as_str().unwrap().bytes().enumerate() {
            assert_eq!(values[i] == values[j], expected == b'1', "bounds {i}/{j}");
        }
    }
}

#[test]
fn native_component_map_and_patch_insertion_order() {
    let rows: Value = serde_json::from_str(include_str!("map-order-fixtures.json")).unwrap();
    let rows = rows.as_array().unwrap();
    let (_, registries) = fixtures();
    let values: Vec<_> = rows
        .iter()
        .map(|row| {
            let bytes = unhex(row["wire"].as_str().unwrap());
            let mut reader = Reader::with_context(&bytes, &registries);
            let patch: DataComponentPatch = reader.read().unwrap();
            reader.finish().unwrap();
            let mut writer = Writer::with_context(&registries);
            writer.write(&patch).unwrap();
            assert_eq!(writer.as_slice(), bytes, "patch wire order {}", row["bits"]);
            let map = DataComponentMap {
                entries: patch.added.clone(),
            };
            let stack = ItemStack::new(ferraloom_data::Item::STONE, 1, patch.clone());
            let other_count = ItemStack::new(ferraloom_data::Item::STONE, 7, patch.clone());
            let template =
                ItemStackTemplate::new(ferraloom_data::Item::STONE, 1, patch.clone()).unwrap();
            (map, patch, stack, other_count, template)
        })
        .collect();
    for (i, row) in rows.iter().enumerate() {
        let left = &values[i];
        for (j, right) in values.iter().enumerate() {
            let expected = |key: &str| row[key].as_str().unwrap().as_bytes()[j] == b'1';
            assert_eq!(left.0 == right.0, expected("map"), "map {i}/{j}");
            assert_eq!(left.1 == right.1, expected("patch"), "patch {i}/{j}");
            assert_eq!(left.2 == right.2, expected("stack"), "stack {i}/{j}");
            assert_eq!(
                left.2.is_same_item_same_components(&right.3).unwrap(),
                expected("same_components"),
                "stack components {i}/{j}"
            );
            assert_eq!(left.4 == right.4, expected("template"), "template {i}/{j}");
        }
    }
}

#[test]
fn native_effect_details_optional_nbt_fields_keep_defaults_and_recursive_partials() {
    let mut checked = 0;
    for row in include_str!("../../tests/java/effects/vectors.tsv").lines() {
        let fields: Vec<_> = row.split('\t').collect();
        let input = ferraloom_nbt::parse(fields[2]).unwrap();
        let actual = MobEffectDetails::from_nbt(&input).and_then(|value| value.to_nbt());
        if fields[3] == "ERROR" {
            assert!(actual.is_err(), "{row}");
        } else {
            assert_eq!(
                actual.unwrap(),
                ferraloom_nbt::parse(fields[3]).unwrap(),
                "{row}"
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 27);
}
