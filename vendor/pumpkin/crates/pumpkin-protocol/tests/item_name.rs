#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{borrow::Cow, io::Cursor};

use pumpkin_command::{
    argument_types::{argument_type::ArgumentType, item::ItemStackArgumentType},
    string_reader::StringReader,
};
use pumpkin_data::{
    data_component::DataComponent,
    data_component_impl::{DamageImpl, DataComponentImpl, ItemNameImpl},
    item::Item,
    item_stack::ItemStack,
};
use pumpkin_nbt::{
    Nbt, compound::NbtCompound, deserializer::NbtReadHelperJava, nbt_compound_tag,
    serializer::NbtWriteHelperJava, tag::NbtTag,
};
use pumpkin_protocol::{
    VarInt,
    codec::{data_component, item_stack_seralizer::ItemStackSerializer},
    ser::{NetworkReadExt, NetworkWriteExt},
};
use pumpkin_util::version::JavaMinecraftVersion;

fn command_stack(value: &str) -> ItemStack {
    let input = format!("stick[minecraft:item_name={value}]");
    let mut reader = StringReader::new(input.as_str());
    let stack =
        <ItemStackArgumentType as ArgumentType<()>>::parse(&ItemStackArgumentType, &mut reader)
            .expect("real command argument should parse");
    assert_eq!(reader.cursor(), input.len());
    assert!(
        stack
            .patch
            .iter()
            .any(|(id, value)| *id == DataComponent::ItemName && value.is_some())
    );
    stack
}

fn name(stack: &ItemStack) -> NbtTag {
    stack
        .get_data_component::<ItemNameImpl>()
        .expect("item_name must be present")
        .write_data()
}

fn nbt_bytes(tag: NbtTag) -> Vec<u8> {
    let mut bytes = Vec::new();
    tag.serialize(&mut NbtWriteHelperJava::new(&mut bytes))
        .unwrap();
    bytes
}

/// Writes actual slot headers around an independently supplied component payload.
fn slot_bytes(payload: &[u8], length_prefixed: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    for value in [
        1,
        i32::from(Item::STICK.id),
        1,
        0,
        i32::from(DataComponent::ItemName.to_id()),
    ] {
        bytes.put_var_int(&VarInt(value)).unwrap();
    }
    if length_prefixed {
        bytes
            .put_var_int(&VarInt(i32::try_from(payload.len()).unwrap()))
            .unwrap();
    }
    bytes.extend_from_slice(payload);
    bytes
}

fn assert_persistent_and_wire(mut stack: ItemStack, expected: &NbtTag) {
    assert_eq!(&name(&stack), expected);
    // Include another component and a removal to expose over/underreads.
    stack.set_data_component(DamageImpl { damage: 7 });
    stack.remove_data_component(DataComponent::CustomName);
    let mut saved = NbtCompound::new();
    stack.write_item_stack(&mut saved);
    assert_eq!(
        saved
            .get_compound("components")
            .unwrap()
            .get("minecraft:item_name"),
        Some(expected)
    );
    let mut bytes = Vec::new();
    Nbt::new(String::new(), saved.clone())
        .write_to_writer(&mut bytes)
        .unwrap();
    let mut cursor = Cursor::new(bytes.as_slice());
    let disk = Nbt::read(&mut NbtReadHelperJava::new(&mut cursor)).unwrap();
    assert_eq!(cursor.position(), bytes.len() as u64);
    let loaded = ItemStack::read_item_stack(&disk.root_tag).unwrap();
    assert_eq!(&name(&loaded), expected);
    let mut resaved = NbtCompound::new();
    loaded.write_item_stack(&mut resaved);
    assert_eq!(resaved, saved);

    for length_prefixed in [false, true] {
        let mut bytes = Vec::new();
        let serializer = ItemStackSerializer(Cow::Borrowed(&loaded));
        if length_prefixed {
            serializer
                .write_untrusted_with_version(&mut bytes, &JavaMinecraftVersion::V_26_3)
                .unwrap();
        } else {
            serializer.write(&mut bytes).unwrap();
        }

        // Decode the outgoing name as native NBT independently of ItemNameImpl.
        let mut outgoing = Cursor::new(bytes.as_slice());
        for expected_header in [1, i32::from(stack.item.id), 2, 1] {
            assert_eq!(outgoing.get_var_int().unwrap().0, expected_header);
        }
        let mut seen = Vec::new();
        for _ in 0..2 {
            let id = outgoing.get_var_int().unwrap().0;
            let declared = length_prefixed.then(|| outgoing.get_var_int().unwrap().0);
            let start = outgoing.position();
            if id == i32::from(DataComponent::ItemName.to_id()) {
                let tag = NbtTag::deserialize(&mut NbtReadHelperJava::new(&mut outgoing)).unwrap();
                assert_eq!(&tag, expected);
            } else {
                assert_eq!(id, i32::from(DataComponent::Damage.to_id()));
                assert_eq!(outgoing.get_var_int().unwrap().0, 7);
            }
            assert!(!seen.contains(&id));
            seen.push(id);
            if let Some(declared) = declared {
                assert_eq!(
                    outgoing.position() - start,
                    u64::try_from(declared).unwrap()
                );
            }
        }
        assert_eq!(
            outgoing.get_var_int().unwrap().0,
            i32::from(DataComponent::CustomName.to_id())
        );
        assert_eq!(outgoing.position(), bytes.len() as u64);

        let mut incoming = Cursor::new(bytes.as_slice());
        let decoded = if length_prefixed {
            ItemStackSerializer::read_untrusted_with_version(
                &mut incoming,
                &JavaMinecraftVersion::V_26_3,
            )
        } else {
            ItemStackSerializer::read(&mut incoming)
        }
        .unwrap()
        .to_stack();
        assert_eq!(incoming.position(), bytes.len() as u64);
        assert_eq!(&name(&decoded), expected);
        assert_eq!(
            decoded.get_data_component::<DamageImpl>().unwrap().damage,
            7
        );
        assert!(
            decoded
                .patch
                .iter()
                .any(|(id, value)| *id == DataComponent::CustomName && value.is_none())
        );
        assert!(loaded.are_items_and_components_equal(&decoded));
    }
}

#[test]
fn command_item_names_keep_literal_translation_and_rich_nbt() {
    let cases = [
        ("''", NbtTag::String("".into())),
        ("'Ferraloom key'", NbtTag::String("Ferraloom key".into())),
        (
            "'item.minecraft.stick'",
            NbtTag::String("item.minecraft.stick".into()),
        ),
        (
            "{text:'item.minecraft.stick'}",
            nbt_compound_tag! { "text": NbtTag::String("item.minecraft.stick".into()) },
        ),
        (
            "{translate:'item.minecraft.stick'}",
            nbt_compound_tag! { "translate": NbtTag::String("item.minecraft.stick".into()) },
        ),
        (
            "{text:'Ferraloom key',color:'red',bold:true,extra:[{text:'!',italic:true}]}",
            nbt_compound_tag! {
                "text": NbtTag::String("Ferraloom key".into()),
                "color": NbtTag::String("red".into()),
                "bold": NbtTag::Byte(1),
                "extra": NbtTag::List(vec![nbt_compound_tag! { "text": NbtTag::String("!".into()), "italic": NbtTag::Byte(1) }]),
            },
        ),
        (
            "{translate:'chat.type.text',fallback:'%s: %s',with:['Doc',{text:'hello',bold:true}],extra:[{text:'!'}]}",
            nbt_compound_tag! {
                "translate": NbtTag::String("chat.type.text".into()),
                "fallback": NbtTag::String("%s: %s".into()),
                "with": NbtTag::List(vec![NbtTag::String("Doc".into()), nbt_compound_tag! { "text": NbtTag::String("hello".into()), "bold": NbtTag::Byte(1) }]),
                "extra": NbtTag::List(vec![nbt_compound_tag! { "text": NbtTag::String("!".into()) }]),
            },
        ),
        (
            "{score:{name:'Doc',objective:'points'},extra:[{text:'!'}]}",
            nbt_compound_tag! {
                "score": nbt_compound_tag! { "name": NbtTag::String("Doc".into()), "objective": NbtTag::String("points".into()) },
                "extra": NbtTag::List(vec![nbt_compound_tag! { "text": NbtTag::String("!".into()) }]),
            },
        ),
        (
            "['Ferraloom',{text:' key',bold:true}]",
            NbtTag::List(vec![
                NbtTag::String("Ferraloom".into()),
                nbt_compound_tag! { "text": NbtTag::String(" key".into()), "bold": NbtTag::Byte(1) },
            ]),
        ),
    ];
    for (input, expected) in cases {
        assert_persistent_and_wire(command_stack(input), &expected);
    }
}

#[test]
fn literal_witness_has_string_nbt_on_wire_and_both_inputs() {
    // TAG_String bytes for the literal shape in the retained vanilla hand witness.
    // This expected payload is deliberately independent of our NBT writer.
    let expected = b"\x08\x00\x0dFerraloom key";
    let stack = command_stack("'Ferraloom key'");
    let mut outgoing = Vec::new();
    ItemStackSerializer(Cow::Borrowed(&stack))
        .write(&mut outgoing)
        .unwrap();
    assert_eq!(outgoing, slot_bytes(expected, false));
    for length_prefixed in [false, true] {
        let bytes = slot_bytes(expected, length_prefixed);
        let mut cursor = Cursor::new(bytes.as_slice());
        let decoded = if length_prefixed {
            ItemStackSerializer::read_length_prefixed_optional(&mut cursor)
        } else {
            ItemStackSerializer::read(&mut cursor)
        }
        .unwrap()
        .to_stack();
        assert_eq!(name(&decoded), NbtTag::String("Ferraloom key".into()));
        assert_eq!(cursor.position(), bytes.len() as u64);
    }
}

#[test]
fn generated_and_intentional_map_names_remain_translated() {
    assert_eq!(
        Item::STICK
            .translated_name()
            .0
            .to_nbt_compound()
            .get_string("translate"),
        Some("item.minecraft.stick")
    );
    let mut stick = ItemStack::new(1, &Item::STICK);
    let default_name = stick.get_data_component::<ItemNameImpl>().unwrap().clone();
    assert!(matches!(
        default_name,
        ItemNameImpl::Translated {
            name: Cow::Borrowed(_)
        }
    ));
    stick.set_data_component(default_name);
    assert_persistent_and_wire(
        stick,
        &nbt_compound_tag! { "translate": NbtTag::String("item.minecraft.stick".into()) },
    );

    // The old filled-map factory intentionally names its result with a translation.
    let mut map_name = ItemStack::new(1, &Item::FILLED_MAP);
    map_name.set_data_component(ItemNameImpl::Translated {
        name: Cow::Borrowed("filled_map.mansion"),
    });
    assert_persistent_and_wire(
        map_name,
        &nbt_compound_tag! { "translate": NbtTag::String("filled_map.mansion".into()) },
    );
}

#[test]
fn item_name_removal_survives_persistence_and_both_wire_routes() {
    let mut stack = ItemStack::new(1, &Item::STICK);
    stack.remove_data_component(DataComponent::ItemName);
    let mut saved = NbtCompound::new();
    stack.write_item_stack(&mut saved);
    let loaded = ItemStack::read_item_stack(&saved).unwrap();
    assert!(loaded.get_data_component::<ItemNameImpl>().is_none());
    assert!(
        saved
            .get_compound("components")
            .unwrap()
            .get("!minecraft:item_name")
            .is_some()
    );
    for length_prefixed in [false, true] {
        let mut bytes = Vec::new();
        let serializer = ItemStackSerializer(Cow::Borrowed(&loaded));
        if length_prefixed {
            serializer
                .write_untrusted_with_version(&mut bytes, &JavaMinecraftVersion::V_26_3)
                .unwrap();
        } else {
            serializer.write(&mut bytes).unwrap();
        }
        let mut cursor = Cursor::new(bytes.as_slice());
        let decoded = if length_prefixed {
            ItemStackSerializer::read_length_prefixed_optional(&mut cursor)
        } else {
            ItemStackSerializer::read(&mut cursor)
        }
        .unwrap()
        .to_stack();
        assert!(decoded.get_data_component::<ItemNameImpl>().is_none());
        assert_eq!(cursor.position(), bytes.len() as u64);
    }
}

#[test]
fn malformed_truncated_and_trailing_names_do_not_become_empty() {
    let valid = [
        NbtTag::String("Ferraloom key".into()),
        nbt_compound_tag! { "translate": NbtTag::String("item.minecraft.stick".into()) },
        nbt_compound_tag! { "text": NbtTag::String("styled".into()), "extra": NbtTag::List(vec![nbt_compound_tag! { "text": NbtTag::String("!".into()) }]) },
    ];
    for tag in valid {
        let payload = nbt_bytes(tag);
        for end in 0..payload.len() {
            for length_prefixed in [false, true] {
                let bytes = slot_bytes(&payload[..end], length_prefixed);
                let mut cursor = Cursor::new(bytes.as_slice());
                let result = if length_prefixed {
                    ItemStackSerializer::read_length_prefixed_optional(&mut cursor)
                } else {
                    ItemStackSerializer::read(&mut cursor)
                };
                assert!(
                    result.is_err(),
                    "accepted truncation at {end}, length_prefixed={length_prefixed}"
                );
            }
        }
        let mut trailing = payload.clone();
        trailing.push(0x7f);
        let bytes = slot_bytes(&trailing, true);
        assert!(
            ItemStackSerializer::read_length_prefixed_optional(&mut Cursor::new(bytes)).is_err()
        );

        // Generic component decoding consumes exactly its NBT, leaving the next field.
        let mut cursor = Cursor::new(trailing);
        data_component::deserialize(DataComponent::ItemName, &mut cursor).unwrap();
        assert_eq!(cursor.position(), payload.len() as u64);
        assert_eq!(cursor.get_u8().unwrap(), 0x7f);
    }
    for invalid in [
        NbtTag::End,
        NbtTag::Int(7),
        nbt_compound_tag! {},
        NbtTag::List(vec![]),
        nbt_compound_tag! { "text": NbtTag::Int(1) },
        nbt_compound_tag! { "translate": NbtTag::Int(1) },
        nbt_compound_tag! { "text": NbtTag::String("x".into()), "extra": NbtTag::Int(1) },
    ] {
        let mut saved = NbtCompound::new();
        saved.put_string("id", "minecraft:stick".into());
        saved.put_int("count", 1);
        saved.put(
            "components",
            nbt_compound_tag! { "minecraft:item_name": invalid.clone() },
        );
        assert!(ItemStack::read_item_stack(&saved).is_none());
        let payload = nbt_bytes(invalid);
        for length_prefixed in [false, true] {
            let bytes = slot_bytes(&payload, length_prefixed);
            let mut cursor = Cursor::new(bytes.as_slice());
            let result = if length_prefixed {
                ItemStackSerializer::read_length_prefixed_optional(&mut cursor)
            } else {
                ItemStackSerializer::read(&mut cursor)
            };
            assert!(result.is_err());
        }
    }
}

#[test]
fn identical_display_projections_do_not_merge_component_identity() {
    let literal = command_stack("'item.minecraft.stick'");
    let translated = command_stack("{translate:'item.minecraft.stick'}");
    assert_eq!(literal.get_hover_name(), translated.get_hover_name());
    assert!(!literal.are_items_and_components_equal(&translated));
    let literal = literal.get_data_component::<ItemNameImpl>().unwrap();
    let translated = translated.get_data_component::<ItemNameImpl>().unwrap();
    assert!(!literal.equal(translated));
    assert_ne!(literal.get_hash(), translated.get_hash());

    let first = command_stack("{text:'same',bold:true,color:'red'}");
    let reordered = command_stack("{color:'red',bold:true,text:'same'}");
    assert!(first.are_items_and_components_equal(&reordered));
    assert_eq!(
        first
            .get_data_component::<ItemNameImpl>()
            .unwrap()
            .get_hash(),
        reordered
            .get_data_component::<ItemNameImpl>()
            .unwrap()
            .get_hash()
    );
}
