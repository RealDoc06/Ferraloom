#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{borrow::Cow, io::Cursor};

use pumpkin_command::{
    argument_types::{argument_type::ArgumentType, item::ItemStackArgumentType},
    snbt::SnbtParser,
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
            NbtTag::String("item.minecraft.stick".into()),
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
                "extra": NbtTag::List(vec![NbtTag::String("!".into())]),
            },
        ),
        (
            "{score:{name:'Doc',objective:'points'},extra:[{text:'!'}]}",
            nbt_compound_tag! {
                "score": nbt_compound_tag! { "name": NbtTag::String("Doc".into()), "objective": NbtTag::String("points".into()) },
                "extra": NbtTag::List(vec![NbtTag::String("!".into())]),
            },
        ),
        (
            "['Ferraloom',{text:' key',bold:true}]",
            nbt_compound_tag! {
                "text": NbtTag::String("Ferraloom".into()),
                "extra": NbtTag::List(vec![nbt_compound_tag! {
                    "text": NbtTag::String(" key".into()), "bold": NbtTag::Byte(1)
                }]),
            },
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
    for input in [
        "'Ferraloom key'",
        "{text:'Ferraloom key'}",
        "{type:'text',text:'Ferraloom key'}",
        "['Ferraloom key']",
    ] {
        let stack = command_stack(input);
        for length_prefixed in [false, true] {
            let mut outgoing = Vec::new();
            let serializer = ItemStackSerializer(Cow::Borrowed(&stack));
            if length_prefixed {
                serializer
                    .write_untrusted_with_version(&mut outgoing, &JavaMinecraftVersion::V_26_3)
                    .unwrap();
            } else {
                serializer.write(&mut outgoing).unwrap();
            }
            assert_eq!(outgoing, slot_bytes(expected, length_prefixed));
        }
    }
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
fn component_lists_append_after_existing_extras_and_normalize_nested_components() {
    let nested = nbt_compound_tag! {
        "text": NbtTag::String("next".into()),
        "extra": NbtTag::List(vec![nbt_compound_tag! {
            "text": NbtTag::String("last".into()), "italic": NbtTag::Byte(1)
        }]),
    };
    let expected = nbt_compound_tag! {
        "text": NbtTag::String("root".into()),
        "bold": NbtTag::Byte(1),
        "extra": NbtTag::List(vec![NbtTag::String("old".into()), nested]),
    };
    let stack =
        command_stack("[{text:'root',bold:true,extra:['old']},['next',{text:'last',italic:true}]]");
    assert_persistent_and_wire(stack.clone(), &expected);
    let equivalent = command_stack(
        "{text:'root',bold:true,extra:['old',{text:'next',extra:[{text:'last',italic:true}]}]}",
    );
    assert!(stack.are_items_and_components_equal(&equivalent));
    assert_eq!(
        stack
            .get_data_component::<ItemNameImpl>()
            .unwrap()
            .get_hash(),
        equivalent
            .get_data_component::<ItemNameImpl>()
            .unwrap()
            .get_hash()
    );
    assert_persistent_and_wire(
        command_stack("{translate:'key',with:[['arg']],extra:[['tail']]}"),
        &nbt_compound_tag! {
            "translate": NbtTag::String("key".into()),
            "with": NbtTag::List(vec![NbtTag::String("arg".into())]),
            "extra": NbtTag::List(vec![NbtTag::String("tail".into())]),
        },
    );
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
        nbt_compound_tag! { "text": NbtTag::String("styled".into()), "extra": NbtTag::List(vec![NbtTag::String("!".into())]) },
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

fn snbt(input: &str) -> NbtTag {
    let mut reader = StringReader::new(input);
    let tag = SnbtParser::parse_for_commands(&mut reader).expect("test input must be valid SNBT");
    assert_eq!(reader.cursor(), input.len());
    tag
}

fn saved_stack(tag: NbtTag) -> NbtCompound {
    let mut saved = NbtCompound::new();
    saved.put_string("id", "minecraft:stick".into());
    saved.put_int("count", 1);
    saved.put(
        "components",
        nbt_compound_tag! { "minecraft:item_name": tag },
    );
    saved
}

/// Exercise real boundaries, including direct native construction that skips `read_data`.
fn assert_refused_at_boundaries(input: &str) {
    use pumpkin_command::argument_types::item::ERROR_INVALID_COMPONENT;

    let tag = snbt(input);
    assert!(ItemNameImpl::parse(&tag).is_err(), "accepted {input}");
    let command = format!("stick[item_name={input}]");
    let mut reader = StringReader::new(&command);
    let error =
        <ItemStackArgumentType as ArgumentType<()>>::parse(&ItemStackArgumentType, &mut reader)
            .err()
            .expect("refused component must fail the command, never omit its name");
    assert!(
        error.is(&ERROR_INVALID_COMPONENT),
        "wrong error for {input}"
    );
    assert_eq!(error.context.unwrap().cursor, "stick[item_name=".len());
    assert!(
        ItemStack::read_item_stack(&saved_stack(tag.clone())).is_none(),
        "loaded {input}"
    );

    let payload = nbt_bytes(tag.clone());
    for length_prefixed in [false, true] {
        let bytes = slot_bytes(&payload, length_prefixed);
        let mut reader = Cursor::new(bytes);
        let result = if length_prefixed {
            ItemStackSerializer::read_length_prefixed_optional(&mut reader)
        } else {
            ItemStackSerializer::read(&mut reader)
        };
        assert!(
            result.is_err(),
            "accepted incoming {input}, length_prefixed={length_prefixed}"
        );
    }

    let unchecked = ItemNameImpl::Component(tag);
    let mut bytes = Vec::new();
    assert!(data_component::serialize(DataComponent::ItemName, &unchecked, &mut bytes).is_err());
    assert!(
        bytes.is_empty(),
        "component writer emitted unchecked data for {input}"
    );
    let mut stack = ItemStack::new(1, &Item::STICK);
    stack.set_data_component(unchecked);
    let serializer = ItemStackSerializer(Cow::Borrowed(&stack));
    assert!(serializer.write(&mut Vec::new()).is_err());
    assert!(
        serializer
            .write_untrusted_with_version(&mut Vec::new(), &JavaMinecraftVersion::V_26_3)
            .is_err()
    );
}

fn assert_accepted_at_boundaries(input: &str) {
    let stack = command_stack(input);
    let normalized = name(&stack);
    let raw = snbt(input);
    let loaded =
        ItemStack::read_item_stack(&saved_stack(raw.clone())).expect("supported disk name");
    assert_eq!(name(&loaded), normalized, "disk normalization for {input}");
    let payload = nbt_bytes(raw);
    for length_prefixed in [false, true] {
        let mut reader = Cursor::new(slot_bytes(&payload, length_prefixed));
        let decoded = if length_prefixed {
            ItemStackSerializer::read_length_prefixed_optional(&mut reader)
        } else {
            ItemStackSerializer::read(&mut reader)
        }
        .expect("supported incoming name")
        .to_stack();
        assert_eq!(
            name(&decoded),
            normalized,
            "incoming normalization for {input}"
        );
        assert!(stack.are_items_and_components_equal(&decoded));
    }
    assert_persistent_and_wire(stack, &normalized);
}

#[test]
fn retained_official_57_case_codec_contract_at_real_item_boundaries() {
    // Transcribed from the actual 26.3 ComponentSerialization probe retained in
    // sign-usable-component-contract.txt, not inferred from this implementation.
    // STREAM_CODEC uses getOrThrow: both NONE and PARTIAL must be refused.
    // These are acceptance observations, not full canonical/render/hash parity.
    #[derive(Clone, Copy)]
    enum Primary {
        None,
        Partial,
        Success,
        SuccessUnsupported,
    }
    use Primary::{None, Partial, Success, SuccessUnsupported};
    let cases = [
        ("{}", None),
        ("{color:'red'}", None),
        ("{extra:['usable']}", None),
        ("[]", None),
        ("[17]", None),
        ("[17,'usable']", Partial),
        ("['usable',17]", Partial),
        ("[{},'usable']", Partial),
        ("[[]]", None),
        ("[[17,'usable']]", Partial),
        ("['one','two']", Success),
        ("{text:'usable'}", Success),
        ("{text:17}", None),
        ("{text:['usable']}", None),
        ("{text:{}}", None),
        ("{text:'usable',bold:'wrong'}", Partial),
        ("{text:'usable',color:'wrong'}", Partial),
        ("{text:'usable',extra:[]}", Partial),
        ("{text:'usable',extra:[17,'extra']}", Partial),
        ("{text:'usable',extra:{}}", Partial),
        ("{translate:'some.key'}", Success),
        ("{translate:17}", None),
        ("{translatable:'some.key'}", None),
        ("{translate:'some.key',with:[17]}", Success),
        ("{score:{name:'Doc',objective:'obj'}}", Success),
        ("{score:{}}", None),
        ("{score:'Doc'}", None),
        ("{score:{name:'Doc'}}", None),
        ("{score:{name:17,objective:'obj'}}", None),
        ("{score:{name:'Doc',objective:17}}", None),
        ("{selector:'@a'}", Success),
        ("{selector:'Doc'}", Success),
        ("{selector:'@bad'}", None),
        ("{selector:17}", None),
        ("{selector:'@a',separator:17}", None),
        ("{keybind:'key.jump'}", Success),
        ("{keybind:17}", None),
        // Valid Java inputs, explicitly unsupported here until NBT path/source
        // grammars are validated. Never relabel these as Java-invalid cases.
        ("{nbt:'foo',storage:'minecraft:test'}", SuccessUnsupported),
        ("{nbt:'foo',entity:'@a'}", SuccessUnsupported),
        ("{nbt:'foo',block:'~ ~ ~'}", SuccessUnsupported),
        ("{nbt:'foo'}", None),
        ("{nbt:17,storage:'minecraft:test'}", None),
        ("{object:'atlas',sprite:'minecraft:foo'}", Success),
        ("{sprite:'minecraft:foo'}", Success),
        ("{object:'atlas'}", None),
        ("{object:'player',player:'Notch'}", Success),
        ("{object:'unknown',sprite:'minecraft:foo'}", None),
        ("{object:{}}", None),
        ("{type:'text',text:'usable'}", Success),
        ("{type:'text',translate:'key'}", None),
        ("{type:'unknown',text:'usable'}", None),
        ("{type:17,text:'usable'}", None),
        ("{type:'translatable',translate:'key'}", Success),
        ("{type:'selector',text:'usable'}", None),
        ("{text:17,translate:'key'}", Success),
        ("{text:'usable',translate:17}", Success),
        ("{object:'unknown',text:'usable'}", Success),
    ];
    assert_eq!(cases.len(), 57);
    for (input, primary) in cases {
        match primary {
            Success => assert_accepted_at_boundaries(input),
            None | Partial => assert_refused_at_boundaries(input),
            SuccessUnsupported => {
                assert!(
                    ItemNameImpl::parse(&snbt(input))
                        .unwrap_err()
                        .starts_with("Unsupported")
                );
                assert_refused_at_boundaries(input);
            }
        }
    }
}

#[test]
fn plain_text_canonicalizes_identity_disk_and_both_wire_inputs() {
    let literal = command_stack("'Ferraloom key'");
    let literal_name = literal.get_data_component::<ItemNameImpl>().unwrap();
    for input in [
        "{text:'Ferraloom key'}",
        "{text:'Ferraloom key',type:'text'}",
        "{text:'Ferraloom key',translate:17}",
    ] {
        let stack = command_stack(input);
        assert!(literal.are_items_and_components_equal(&stack));
        assert_eq!(
            stack
                .get_data_component::<ItemNameImpl>()
                .unwrap()
                .get_hash(),
            literal_name.get_hash()
        );
        assert_accepted_at_boundaries(input);
        assert_persistent_and_wire(stack, &NbtTag::String("Ferraloom key".into()));

        // Even a native unchecked constructor must go through normalization.
        let mut payload = Vec::new();
        data_component::serialize(
            DataComponent::ItemName,
            &ItemNameImpl::Component(snbt(input)),
            &mut payload,
        )
        .unwrap();
        assert_eq!(payload, b"\x08\x00\x0dFerraloom key");
    }
    assert_eq!(
        name(&command_stack("{text:17,translate:'key'}")),
        snbt("{translate:'key'}")
    );
}

#[test]
fn supported_style_events_and_bytecode_shapes_are_checked_recursively() {
    // Fields/codecs confirmed in 26.3 Style.Serializer, AtlasSprite, PlayerSprite,
    // ClickEvent.CopyToClipboard and HoverEvent.ShowText disassemblies.
    let input = "{text:'styled',color:'#12abef',bold:1b,italic:0b,underlined:1b,strikethrough:0b,obfuscated:0b,shadow_color:-1,font:'minecraft:default',insertion:'Doc',click_event:{action:'copy_to_clipboard',value:'hello'},hover_event:{action:'show_text',value:{text:'hover',color:'red'}},extra:['!']}";
    assert_accepted_at_boundaries(input);
    assert_persistent_and_wire(command_stack(input), &snbt(input));
    for input in [
        "{text:'x',italic:'true'}",
        "{text:'x',underlined:{}}",
        "{text:'x',strikethrough:[]}",
        "{text:'x',obfuscated:'false'}",
        "{text:'x',color:'#1000000'}",
        "{text:'x',insertion:17}",
        "{text:'x',font:'Bad Identifier'}",
        "{text:'x',click_event:{action:'copy_to_clipboard',value:17}}",
        "{text:'x',hover_event:{action:'show_text',value:{color:'red'}}}",
        "{translate:'key',with:[{text:'x',bold:'wrong'}]}",
        "{selector:'@a',separator:{text:'x',extra:[]}}",
        "{sprite:'minecraft:foo',atlas:17}",
        "{object:'player',player:'Notch',hat:'wrong'}",
    ] {
        assert_refused_at_boundaries(input);
    }
}

#[test]
fn unsupported_opaque_forms_cannot_bypass_outgoing_validation() {
    // Unsupported is deliberately separate from Java-invalid. Expanding support
    // requires the corresponding codec/grammar, not accepting these as opaque NBT.
    for input in [
        "{selector:'@a[tag=example]'}",
        "{text:'x',hover_event:{action:'show_item',id:'minecraft:stick'}}",
        "{text:'x',click_event:{action:'run_command',command:'/help'}}",
        "{text:'x',shadow_color:[1.0f,1.0f,1.0f,1.0f]}",
        "{object:'player',player:{name:'Notch',properties:[]}}",
        "{text:'x',future_component_field:{value:17}}",
    ] {
        assert!(
            ItemNameImpl::parse(&snbt(input))
                .unwrap_err()
                .starts_with("Unsupported")
        );
        assert_refused_at_boundaries(input);
    }
}

#[test]
fn literal_hash_uses_the_primary_utf16_preimage() {
    use pumpkin_data::data_component_impl::Digest;

    // Bytecode-derived preimages, not a paired Java execution. HashOps uses tag12,
    // String.length and Guava putUnencodedChars: UTF-16LE, including surrogate pairs.
    for (input, preimage) in [
        (
            "Schlüssel",
            b"\x0c\x09\0\0\0S\0c\0h\0l\0\xfc\0s\0s\0e\0l\0".as_slice(),
        ),
        ("🔑", b"\x0c\x02\0\0\0\x3d\xd8\x11\xdd".as_slice()),
    ] {
        let mut expected = Digest::new();
        expected.update(preimage);
        let literal = ItemNameImpl::Literal {
            name: Cow::Owned(input.into()),
        };
        assert_eq!(literal.get_hash(), expected.finalize() as i32);
    }
}
