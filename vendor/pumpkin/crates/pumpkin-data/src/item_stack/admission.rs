//! Preservation gate for saved items before a modeled inventory consumes them.
//! This does not implement unsupported component gameplay or network codecs.

use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemStackAdmissionError {
    EnvelopeExtensions,
    Unreadable,
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
            Self::Lossy => "item cannot round-trip losslessly through the current item codec",
        }
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
