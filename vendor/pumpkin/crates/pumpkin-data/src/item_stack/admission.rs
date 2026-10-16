//! Preservation gates before a modeled inventory consumes an item.
//! This does not implement unsupported component gameplay or network codecs.

use crate::{
    data_component::DataComponent,
    data_component_impl::{BundleContentsImpl, ContainerImpl},
};
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemStackAdmissionError {
    EnvelopeExtensions,
    Unreadable,
    Unwritable,
    Lossy,
}

impl ItemStackAdmissionError {
    /// Static diagnostics keep item names and authored component data private.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::EnvelopeExtensions => {
                "item envelope extensions are not modeled; components use the central item codec"
            }
            Self::Unreadable => "item cannot be read by the current item codec",
            Self::Unwritable => "item contains data the current persistence writer cannot encode",
            Self::Lossy => "item cannot round-trip losslessly through the current item codec",
        }
    }
}

/// Check the persistence projection of an already decoded stack before publishing
/// it. This cannot recover data discarded by an upstream wire/command decoder and
/// does not certify component gameplay or wire fidelity. Transient components are
/// intentionally omitted by the writer; component removals remain valid patches.
pub fn validate_runtime_item_stack(
    stack: &super::ItemStack,
) -> Result<(), ItemStackAdmissionError> {
    if stack.is_empty() && stack.patch.is_empty() {
        return Ok(());
    }
    // Check every actual typed stack before any writer can overwrite duplicate
    // patch keys. Only these components hold ItemStacks; arbitrary authored NBT
    // (including custom_data) must not be interpreted as another inventory.
    let mut pending_stacks = vec![stack];
    while let Some(current) = pending_stacks.pop() {
        let mut seen = std::collections::HashSet::new();
        for (id, value) in &current.patch {
            if !seen.insert(*id)
                || value
                    .as_ref()
                    .is_some_and(|value| value.get_self_enum() != *id)
            {
                return Err(ItemStackAdmissionError::Lossy);
            }
            let Some(value) = value else { continue };
            match id {
                DataComponent::BundleContents => {
                    let bundle = value
                        .as_any()
                        .downcast_ref::<BundleContentsImpl>()
                        .ok_or(ItemStackAdmissionError::Lossy)?;
                    pending_stacks.extend(&bundle.items);
                }
                DataComponent::Container => {
                    let container = value
                        .as_any()
                        .downcast_ref::<ContainerImpl>()
                        .ok_or(ItemStackAdmissionError::Lossy)?;
                    pending_stacks.extend(container.items.iter().map(|(_, nested)| nested));
                }
                _ => {}
            }
        }
    }
    let mut written = NbtCompound::new();
    stack.write_item_stack(&mut written);
    // Inspect before decoding: a placeholder's End can survive the in-memory
    // codec round trip while terminating the enclosing compound on disk.
    let mut pending: Vec<_> = written.child_tags.values().collect();
    while let Some(tag) = pending.pop() {
        match tag {
            NbtTag::End => return Err(ItemStackAdmissionError::Unwritable),
            NbtTag::Compound(compound) => pending.extend(compound.child_tags.values()),
            NbtTag::List(items) => pending.extend(items),
            _ => {}
        }
    }
    written
        .clone()
        .serialize_content(&mut pumpkin_nbt::serializer::NbtWriteHelperJava::new(
            &mut std::io::sink(),
        ))
        .map_err(|_| ItemStackAdmissionError::Unwritable)?;
    if stack.is_empty() {
        // Empty stacks clear inventory slots, but some entity writers still
        // serialize their patches. They must pass the End/encoding checks too.
        Ok(())
    } else {
        validate_item_stack(&written)
    }
}

/// Compare the actual decoder/writer against the saved tree, allowing only
/// documented defaults. The caller validates and removes its inventory Slot.
pub fn validate_item_stack(item: &NbtCompound) -> Result<(), ItemStackAdmissionError> {
    if item
        .child_tags
        .keys()
        .any(|key| !matches!(key.as_ref(), "id" | "count" | "components"))
    {
        return Err(ItemStackAdmissionError::EnvelopeExtensions);
    }
    if !match item.get("count") {
        None => true,
        Some(NbtTag::Int(count)) => (1..=255).contains(count),
        _ => false,
    } || item
        .get("components")
        .is_some_and(|tag| !matches!(tag, NbtTag::Compound(_)))
    {
        return Err(ItemStackAdmissionError::Unreadable);
    }
    // Charged projectiles retain raw NBT today, but gameplay reads each through
    // ItemStack. Check those boundaries too, so opaque retention cannot mask a
    // nested stack that the shared codec would lose when it is used.
    if let Some(components) = item.get_compound("components") {
        for (name, data) in &components.child_tags {
            let Some(list) = data.extract_list() else {
                continue;
            };
            for value in list {
                let nested = match name.as_ref() {
                    "minecraft:container" => value
                        .extract_compound()
                        .and_then(|entry| entry.get_compound("item")),
                    "minecraft:bundle_contents" | "minecraft:charged_projectiles" => {
                        value.extract_compound()
                    }
                    _ => None,
                };
                if let Some(nested) = nested {
                    validate_item_stack(nested)?;
                }
            }
        }
    }
    let stack = super::ItemStack::read_item_stack(item)
        .filter(|stack| !stack.is_empty())
        .ok_or(ItemStackAdmissionError::Unreadable)?;
    let mut written = NbtCompound::new();
    stack.write_item_stack(&mut written);
    let mut expected = item.clone();
    normalize_item_defaults(&mut expected);
    normalize_item_defaults(&mut written);
    if written != expected {
        return Err(ItemStackAdmissionError::Lossy);
    }
    Ok(())
}

// Compare only documented codec defaults at known item/component boundaries.
// Never walk arbitrary compounds: a field named "components" inside custom_data,
// block_entity_data, a book, or an extension is authored data, not an item patch.
fn normalize_item_defaults(item: &mut NbtCompound) {
    if item.get("count").is_none() {
        item.put_int("count", 1);
    }
    if item
        .get_compound("components")
        .is_some_and(|patch| patch.child_tags.is_empty())
    {
        item.child_tags.remove("components");
    }
    let Some(NbtTag::Compound(components)) = item.child_tags.get_mut("components") else {
        return;
    };
    for (name, data) in &mut components.child_tags {
        match name.as_ref() {
            "minecraft:instrument" => {
                if let NbtTag::String(name) = data
                    && let Some(instrument) = crate::instrument::Instrument::from_name(name)
                {
                    *name = instrument.asset_id().into();
                }
            }
            "minecraft:container" => {
                if let NbtTag::List(entries) = data {
                    for entry in entries {
                        if let NbtTag::Compound(entry) = entry
                            && let Some(NbtTag::Compound(nested)) = entry.child_tags.get_mut("item")
                        {
                            normalize_item_defaults(nested);
                        }
                    }
                }
            }
            "minecraft:bundle_contents" | "minecraft:charged_projectiles" => {
                if let NbtTag::List(items) = data {
                    for nested in items {
                        if let NbtTag::Compound(nested) = nested {
                            normalize_item_defaults(nested);
                        }
                    }
                }
            }
            "minecraft:potion_contents" => normalize_potion_defaults(data),
            "minecraft:fireworks" => {
                if let NbtTag::Compound(fireworks) = data {
                    normalize_unsigned_byte(fireworks, "flight_duration");
                    if fireworks
                        .get_list("explosions")
                        .is_some_and(<[NbtTag]>::is_empty)
                    {
                        fireworks.child_tags.remove("explosions");
                    }
                }
            }
            _ => {}
        }
    }
}

fn normalize_unsigned_byte(compound: &mut NbtCompound, key: &str) {
    if let Some(value) = compound.get_int(key)
        && let Ok(value) = u8::try_from(value)
    {
        compound.put_byte(key, value as i8);
    }
    if compound.get_byte(key) == Some(0) {
        compound.child_tags.remove(key);
    }
}

fn normalize_potion_defaults(data: &mut NbtTag) {
    // PotionContents accepts a potion name as shorthand for its full record.
    if let NbtTag::String(name) = data {
        let mut contents = NbtCompound::new();
        contents.put_string("potion", name.to_string());
        *data = NbtTag::Compound(contents);
    }
    let NbtTag::Compound(contents) = data else {
        return;
    };
    let potion = match contents.get("potion") {
        Some(NbtTag::String(name)) => {
            crate::potion::Potion::from_name(name.strip_prefix("minecraft:").unwrap_or(name))
        }
        // Older Rust saves used the same pinned registry's numeric ID. Resolve
        // only known IDs; this must never truncate or erase an unknown value.
        Some(NbtTag::Int(id)) => u8::try_from(*id)
            .ok()
            .and_then(crate::potion::Potion::from_id),
        _ => None,
    };
    if let Some(potion) = potion {
        contents.put_string("potion", format!("minecraft:{}", potion.name));
    }
    if contents
        .get_list("custom_effects")
        .is_some_and(<[NbtTag]>::is_empty)
    {
        contents.child_tags.remove("custom_effects");
    }
    if let Some(NbtTag::List(effects)) = contents.child_tags.get_mut("custom_effects") {
        for effect in effects {
            let NbtTag::Compound(effect) = effect else {
                continue;
            };
            normalize_unsigned_byte(effect, "amplifier");
            if effect.get_int("duration") == Some(0) {
                effect.child_tags.remove("duration");
            }
            if effect.get_byte("ambient") == Some(0) {
                effect.child_tags.remove("ambient");
            }
            if effect.get("show_icon").is_none() {
                effect.put_byte("show_icon", effect.get_byte("show_particles").unwrap_or(1));
            }
            if effect.get_byte("show_particles") == Some(1) {
                effect.child_tags.remove("show_particles");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_component::DataComponent,
        data_component_impl::{
            BundleContentsImpl, ContainerImpl, CustomDataImpl, DataComponentImpl,
            TooltipDisplayImpl,
        },
        item::Item,
        item_stack::ItemStack,
    };

    #[test]
    fn instrument_raw_admission_normalizes_only_known_references() {
        fn horn(value: NbtTag) -> NbtCompound {
            let mut patch = NbtCompound::new();
            patch.put("minecraft:instrument", value);
            let mut item = NbtCompound::new();
            item.put_string("id", "minecraft:goat_horn".into());
            item.put_int("count", 1);
            item.put_compound("components", patch);
            item
        }
        for instrument in crate::instrument::Instrument::all() {
            for name in [instrument.to_name(), instrument.asset_id()] {
                let saved = horn(NbtTag::String(name.into()));
                assert_eq!(validate_item_stack(&saved), Ok(()));
                let stack = ItemStack::read_item_stack(&saved).unwrap();
                assert_eq!(validate_runtime_item_stack(&stack), Ok(()));
                let mut written = NbtCompound::new();
                stack.write_item_stack(&mut written);
                assert_eq!(
                    written
                        .get_compound("components")
                        .unwrap()
                        .get_string("minecraft:instrument"),
                    Some(instrument.asset_id())
                );
                // Raw admission must not mutate its caller's saved tree.
                assert_eq!(
                    saved
                        .get_compound("components")
                        .unwrap()
                        .get_string("minecraft:instrument"),
                    Some(name)
                );
            }
        }
        let mut inline = NbtCompound::new();
        inline.put_string("sound_event", "minecraft:item.goat_horn.sound.1".into());
        inline.put_float("use_duration", 7.0);
        inline.put_float("range", 256.0);
        inline.put_string("description", "Sing".into());
        for value in [
            NbtTag::String("custom:sing_goat_horn".into()),
            NbtTag::String("minecraft:missing_horn".into()),
            NbtTag::String("Minecraft:sing_goat_horn".into()),
            NbtTag::Int(6),
            NbtTag::Compound(NbtCompound::new()),
            NbtTag::Compound(inline),
        ] {
            let saved = horn(value);
            assert!(ItemStack::read_item_stack(&saved).is_none());
            assert_eq!(
                validate_item_stack(&saved),
                Err(ItemStackAdmissionError::Unreadable)
            );
        }
        let mut opaque = NbtCompound::new();
        opaque.put_string("minecraft:instrument", "sing_goat_horn".into());
        let stack = ItemStack::new_with_component(
            1,
            &Item::GOAT_HORN,
            vec![
                (DataComponent::Instrument, None),
                (
                    DataComponent::CustomData,
                    Some(CustomDataImpl::new(opaque.clone()).to_dyn()),
                ),
            ],
        );
        let mut saved = NbtCompound::new();
        stack.write_item_stack(&mut saved);
        assert_eq!(validate_item_stack(&saved), Ok(()));
        assert_eq!(
            saved
                .get_compound("components")
                .unwrap()
                .get_compound("minecraft:custom_data"),
            Some(&opaque)
        );
        assert!(
            ItemStack::read_item_stack(&saved)
                .unwrap()
                .get_data_component::<crate::data_component_impl::InstrumentImpl>()
                .is_none()
        );
    }

    #[test]
    fn runtime_admission_rejects_writerless_components_including_nested_stacks() {
        let bad = ItemStack::new_with_component(
            1,
            &Item::STICK,
            vec![(
                DataComponent::TooltipDisplay,
                Some(TooltipDisplayImpl.to_dyn()),
            )],
        );
        let nested = ItemStack::new_with_component(
            1,
            &Item::SHULKER_BOX,
            vec![(
                DataComponent::Container,
                Some(
                    ContainerImpl {
                        items: vec![(0, bad.clone())],
                    }
                    .to_dyn(),
                ),
            )],
        );
        let bundle = ItemStack::new_with_component(
            1,
            &Item::BUNDLE,
            vec![(
                DataComponent::BundleContents,
                Some(
                    BundleContentsImpl {
                        items: vec![bad.clone()],
                    }
                    .to_dyn(),
                ),
            )],
        );
        for stack in [bad.copy_with_count(0), bad, nested, bundle] {
            assert_eq!(
                validate_runtime_item_stack(&stack),
                Err(ItemStackAdmissionError::Unwritable)
            );
        }
    }

    #[test]
    fn runtime_admission_preserves_removals_nested_items_and_opaque_custom_data() {
        let healthy = ItemStack::new_with_component(
            37,
            &Item::GOLD_INGOT,
            vec![(DataComponent::TooltipDisplay, None)],
        );
        let nested = ItemStack::new_with_component(
            1,
            &Item::SHULKER_BOX,
            vec![(
                DataComponent::Container,
                Some(
                    ContainerImpl {
                        items: vec![(0, healthy.clone())],
                    }
                    .to_dyn(),
                ),
            )],
        );
        let bundle = ItemStack::new_with_component(
            1,
            &Item::BUNDLE,
            vec![(
                DataComponent::BundleContents,
                Some(
                    BundleContentsImpl {
                        items: vec![healthy.clone()],
                    }
                    .to_dyn(),
                ),
            )],
        );
        let mut authored = NbtCompound::new();
        let mut components = NbtCompound::new();
        components.put_string("minecraft:instrument", "opaque authored value".into());
        authored.put_compound("components", components);
        let custom = ItemStack::new_with_component(
            1,
            &Item::STICK,
            vec![(
                DataComponent::CustomData,
                Some(CustomDataImpl::new(authored).to_dyn()),
            )],
        );
        for stack in [ItemStack::EMPTY.clone(), healthy, nested, bundle, custom] {
            assert_eq!(validate_runtime_item_stack(&stack), Ok(()));
        }
    }

    #[test]
    fn runtime_admission_refuses_duplicate_patch_keys_before_the_writer_overwrites_them() {
        let stack = ItemStack::new_with_component(
            1,
            &Item::STICK,
            vec![
                (DataComponent::TooltipDisplay, None),
                (
                    DataComponent::TooltipDisplay,
                    Some(TooltipDisplayImpl.to_dyn()),
                ),
            ],
        );
        assert_eq!(
            validate_runtime_item_stack(&stack),
            Err(ItemStackAdmissionError::Lossy)
        );
    }

    #[test]
    fn nested_runtime_type_checks_preserve_transients_removals_and_opaque_data() {
        use crate::data_component_impl::CreativeSlotLockImpl;

        fn nested(stack: ItemStack) -> ItemStack {
            let bundle = ItemStack::new_with_component(
                1,
                &Item::BUNDLE,
                vec![(
                    DataComponent::BundleContents,
                    Some(BundleContentsImpl { items: vec![stack] }.to_dyn()),
                )],
            );
            ItemStack::new_with_component(
                1,
                &Item::SHULKER_BOX,
                vec![(
                    DataComponent::Container,
                    Some(
                        ContainerImpl {
                            items: vec![(0, bundle)],
                        }
                        .to_dyn(),
                    ),
                )],
            )
        }

        // Wire decoding chooses matching types; this protects runtime producers
        // too, without claiming a wrong-type wire exploit.
        let wrong_type = ItemStack::new_with_component(
            1,
            &Item::STICK,
            vec![(
                DataComponent::CustomName,
                Some(CustomDataImpl::new(NbtCompound::new()).to_dyn()),
            )],
        );
        assert_eq!(
            validate_runtime_item_stack(&nested(wrong_type)),
            Err(ItemStackAdmissionError::Lossy)
        );

        let mut aliases = NbtCompound::new();
        aliases.put_string("custom_name", "A".into());
        aliases.put_string("minecraft:custom_name", "B".into());
        let mut authored_item = NbtCompound::new();
        authored_item.put_string("id", "minecraft:stick".into());
        authored_item.put_compound("components", aliases);
        let mut authored = NbtCompound::new();
        authored.put_list(
            "minecraft:bundle_contents",
            vec![NbtTag::Compound(authored_item)],
        );
        let healthy = ItemStack::new_with_component(
            1,
            &Item::STICK,
            vec![
                (
                    DataComponent::CustomData,
                    Some(CustomDataImpl::new(authored.clone()).to_dyn()),
                ),
                (DataComponent::TooltipDisplay, None),
                (
                    DataComponent::CreativeSlotLock,
                    Some(CreativeSlotLockImpl.to_dyn()),
                ),
            ],
        );
        assert_eq!(
            validate_runtime_item_stack(&nested(healthy.clone())),
            Ok(())
        );
        assert_eq!(
            healthy.get_data_component::<CustomDataImpl>().unwrap().data,
            authored
        );
        assert!(
            healthy
                .get_data_component::<CreativeSlotLockImpl>()
                .is_some()
        );
        assert!(
            healthy
                .patch
                .iter()
                .any(|(id, data)| *id == DataComponent::TooltipDisplay && data.is_none())
        );
    }
}
