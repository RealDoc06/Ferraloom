//! Ferraloom: the item components Horizon does not model, on the wire.
//!
//! `guest::stack_ref_to_network` writes the components Horizon models; a stack
//! keeps the others as their saved NBT (`StackData::other`). Vanilla sends
//! every networked component, so lore, custom data, a styled name and the
//! rest that `/give`, loot tables, data packs and plugins put on items reach
//! the client from here, in each component's stream codec. A stack with
//! nothing unmodelled costs one emptiness check.
use crate::items::ItemStack;
use horizon_protocol::item::{Component, Profile, Stack, component};
use horizon_world::nbt::Tag;

/// `minecraft:data_component_type` ids the protocol crate does not name.
const GLIDER: i32 = 34;
const ENCHANTABLE: i32 = 31;
const TOOLTIP_STYLE: i32 = 35;
const DAMAGE_RESISTANT: i32 = 27;

/// Adds `s`'s unmodelled networked components to `stack`, and its name's
/// component in place of the plain text when the stack keeps one.
pub fn add(stack: &mut Stack, s: &ItemStack) {
    let Some(d) = s.data() else { return };
    if d.other.is_empty() {
        return;
    }
    // A styled name replaces the plain text `stack_ref_to_network` wrote.
    if let Some(name) = d.custom_name_tag() {
        stack.added.retain(|(k, _)| *k != component::CUSTOM_NAME);
        stack
            .added
            .push((component::CUSTOM_NAME, Component::Text(name.clone())));
    }
    for (id, tag) in &d.other {
        let Some((kind, value)) = network(id, tag) else {
            continue;
        };
        if stack.added.iter().all(|(k, _)| *k != kind) {
            stack.added.push((kind, value));
        }
    }
}

/// One saved component in its stream codec's form; none for one Horizon
/// does not send.
fn network(id: &str, tag: &Tag) -> Option<(i32, Component)> {
    let value = match id {
        "minecraft:custom_data" => (component::CUSTOM_DATA, Component::Nbt(tag.clone())),
        "minecraft:lore" => match tag {
            // ItemLore.MAX_LINES: the client's codec refuses a longer list.
            Tag::List(lines) => {
                let lines = lines.iter().take(256).cloned().collect();
                (component::LORE, Component::Lore(lines))
            }
            _ => return None,
        },
        "minecraft:max_stack_size" => (component::MAX_STACK_SIZE, Component::Int(int(tag)?)),
        "minecraft:max_damage" => (component::MAX_DAMAGE, Component::Int(int(tag)?)),
        "minecraft:unbreakable" => (component::UNBREAKABLE, Component::Unit),
        "minecraft:glider" => (GLIDER, Component::Unit),
        "minecraft:item_model" => (component::ITEM_MODEL, Component::Ident(text(tag)?)),
        "minecraft:tooltip_style" => (TOOLTIP_STYLE, Component::Ident(text(tag)?)),
        "minecraft:rarity" => {
            let rarity = ["common", "uncommon", "rare", "epic"];
            let i = rarity
                .iter()
                .position(|r| text(tag).as_deref() == Some(r))?;
            (component::RARITY, Component::Int(i as i32))
        }
        "minecraft:enchantment_glint_override" => (
            component::ENCHANTMENT_GLINT_OVERRIDE,
            Component::Bool(int(tag)? != 0),
        ),
        // Enchantable's codec rejects a value below 1: the client would disconnect.
        "minecraft:enchantable" => (
            ENCHANTABLE,
            Component::Int(int(tag.get("value")?).filter(|v| *v >= 1)?),
        ),
        // DamageResistant: a holder set of damage types (ByteBufCodecs.holderSet).
        // Only a tag crosses: 0, then the tag's id. Vanilla's items name
        // tags; a list of types would need the registry's ids.
        "minecraft:damage_resistant" => {
            let types = text(tag.get("types")?)?;
            let mut w = horizon_protocol::buf::Writer::new();
            w.varint(0);
            w.string(types.strip_prefix('#')?);
            (DAMAGE_RESISTANT, Component::Raw(w.data))
        }
        "minecraft:custom_model_data" => (
            component::CUSTOM_MODEL_DATA,
            Component::CustomModelData {
                floats: list(tag.get("floats"), |t| match t {
                    Tag::Float(bits) => Some(f32::from_bits(*bits)),
                    _ => int(t).map(|v| v as f32),
                }),
                flags: list(tag.get("flags"), |t| int(t).map(|v| v != 0)),
                strings: list(tag.get("strings"), text),
                colors: list(tag.get("colors"), int),
            },
        ),
        "minecraft:tooltip_display" => (
            component::TOOLTIP_DISPLAY,
            Component::TooltipDisplay {
                hide: tag
                    .get("hide_tooltip")
                    .and_then(int)
                    .is_some_and(|v| v != 0),
                hidden: list(tag.get("hidden_components"), |t| {
                    let name = text(t)?;
                    let name = name.strip_prefix("minecraft:").unwrap_or(&name);
                    COMPONENTS.iter().position(|c| *c == name).map(|i| i as i32)
                }),
            },
        ),
        "minecraft:profile" => (component::PROFILE, Component::Profile(profile(tag)?)),
        _ => return None,
    };
    Some(value)
}

/// `ResolvableProfile`'s saved form: a name alone, or `{name, id, properties}`.
fn profile(tag: &Tag) -> Option<Profile> {
    if let Tag::String(name) = tag {
        return Some(Profile {
            name: Some(name.to_string_lossy()),
            id: None,
            properties: Vec::new(),
        });
    }
    let id = match tag.get("id") {
        Some(Tag::IntArray(a)) if a.len() == 4 => Some(
            a.iter()
                .fold(0u128, |id, part| (id << 32) | u128::from(*part as u32)),
        ),
        _ => None,
    };
    let properties = list(tag.get("properties"), |p| {
        Some((
            text(p.get("name")?)?,
            text(p.get("value")?)?,
            p.get("signature").and_then(text),
        ))
    });
    Some(Profile {
        name: tag.get("name").and_then(text),
        id,
        properties,
    })
}

fn int(tag: &Tag) -> Option<i32> {
    match tag {
        Tag::Byte(v) => Some(i32::from(*v)),
        Tag::Short(v) => Some(i32::from(*v)),
        Tag::Int(v) => Some(*v),
        _ => None,
    }
}

fn text(tag: &Tag) -> Option<String> {
    match tag {
        Tag::String(s) => Some(s.to_string_lossy()),
        _ => None,
    }
}

fn list<T>(tag: Option<&Tag>, f: impl Fn(&Tag) -> Option<T>) -> Vec<T> {
    match tag {
        Some(Tag::List(items)) => items.iter().filter_map(f).collect(),
        _ => Vec::new(),
    }
}

/// `minecraft:data_component_type` in registry order (26.3): an id is its index.
const COMPONENTS: [&str; 122] = [
    "custom_data",
    "max_stack_size",
    "max_damage",
    "damage",
    "unbreakable",
    "use_effects",
    "custom_name",
    "minimum_attack_charge",
    "damage_type",
    "item_name",
    "item_model",
    "lore",
    "rarity",
    "enchantments",
    "can_place_on",
    "can_break",
    "attribute_modifiers",
    "custom_model_data",
    "tooltip_display",
    "repair_cost",
    "creative_slot_lock",
    "enchantment_glint_override",
    "intangible_projectile",
    "food",
    "consumable",
    "use_remainder",
    "use_cooldown",
    "damage_resistant",
    "tool",
    "weapon",
    "attack_range",
    "enchantable",
    "equippable",
    "repairable",
    "glider",
    "tooltip_style",
    "death_protection",
    "blocks_attacks",
    "piercing_weapon",
    "kinetic_weapon",
    "attack_animation",
    "interact_animation",
    "additional_trade_cost",
    "block_transformer",
    "villager_food",
    "stored_enchantments",
    "dye",
    "dyed_color",
    "map_id",
    "map_decorations",
    "map_post_processing",
    "charged_projectiles",
    "bundle_contents",
    "potion_contents",
    "potion_duration_scale",
    "suspicious_stew_effects",
    "writable_book_content",
    "written_book_content",
    "trim",
    "debug_stick_state",
    "entity_data",
    "bucket_entity_data",
    "block_entity_data",
    "instrument",
    "provides_trim_material",
    "ominous_bottle_amplifier",
    "jukebox_playable",
    "provides_banner_patterns",
    "recipes",
    "lodestone_tracker",
    "firework_explosion",
    "fireworks",
    "profile",
    "note_block_sound",
    "banner_patterns",
    "base_color",
    "pot_decorations",
    "container",
    "block_state",
    "bees",
    "sulfur_cube_content",
    "lock",
    "container_loot",
    "break_sound",
    "compostable",
    "cooking_fuel",
    "brewing_fuel",
    "mob_visibility",
    "villager/variant",
    "wolf/variant",
    "wolf/sound_variant",
    "wolf/collar",
    "fox/variant",
    "salmon/size",
    "parrot/variant",
    "tropical_fish/pattern",
    "tropical_fish/base_color",
    "tropical_fish/pattern_color",
    "mooshroom/variant",
    "rabbit/variant",
    "pig/variant",
    "pig/sound_variant",
    "cow/variant",
    "cow/sound_variant",
    "chicken/variant",
    "chicken/sound_variant",
    "zombie_nautilus/variant",
    "frog/variant",
    "horse/variant",
    "painting/variant",
    "llama/variant",
    "axolotl/variant",
    "cat/variant",
    "cat/sound_variant",
    "cat/collar",
    "sheep/color",
    "shulker/color",
    "provides_pottery_pattern",
    "sign_text_front",
    "sign_text_back",
    "waxed",
    "cushion/color",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item_data::decode_components;

    fn stack_with(components: Tag) -> Stack {
        let (damage, extra) = decode_components(Some(&components));
        let item = ItemStack {
            item: "minecraft:stick".into(),
            count: 1,
            damage,
            extra,
        };
        super::super::guest::stack_ref_to_network(|_| 1, |_, _| None, &item).unwrap()
    }

    #[test]
    fn unmodelled_components_reach_the_wire() {
        let lore = Tag::List(vec![Tag::String("Sharp".into())]);
        let stack = stack_with(Tag::compound([
            ("minecraft:lore", lore.clone()),
            (
                "minecraft:custom_data",
                Tag::compound([("price", Tag::Int(5))]),
            ),
            ("minecraft:unbreakable", Tag::Compound(Default::default())),
            (
                "minecraft:tooltip_display",
                Tag::compound([(
                    "hidden_components",
                    Tag::List(vec![Tag::String("minecraft:enchantments".into())]),
                )]),
            ),
            ("minecraft:rarity", Tag::String("epic".into())),
            // Not networked by Horizon yet: left off, as before.
            ("minecraft:food", Tag::Compound(Default::default())),
        ]));
        let Tag::List(lines) = lore else {
            unreachable!()
        };
        assert_eq!(
            stack.component(component::LORE),
            Some(&Component::Lore(lines))
        );
        assert_eq!(
            stack.component(component::CUSTOM_DATA),
            Some(&Component::Nbt(Tag::compound([("price", Tag::Int(5))])))
        );
        assert_eq!(
            stack.component(component::UNBREAKABLE),
            Some(&Component::Unit)
        );
        assert_eq!(
            stack.component(component::TOOLTIP_DISPLAY),
            Some(&Component::TooltipDisplay {
                hide: false,
                hidden: vec![13],
            })
        );
        assert_eq!(stack.component(component::RARITY), Some(&Component::Int(3)));
        assert_eq!(stack.component(23), None);
    }

    #[test]
    fn a_stack_without_extras_is_sent_as_before() {
        let stack = stack_with(Tag::compound([("minecraft:damage", Tag::Int(3))]));
        assert_eq!(stack.added, vec![(component::DAMAGE, Component::Int(3))]);
    }

    /// `ByteBufCodecs.holderSet` for a tag: 0, then the id as a string.
    #[test]
    fn damage_resistant_is_a_holder_set_naming_its_tag() {
        let tag = Tag::compound([("types", Tag::String("#minecraft:is_fire".into()))]);
        let mut bytes = vec![0, 17];
        bytes.extend(b"minecraft:is_fire");
        assert_eq!(
            network("minecraft:damage_resistant", &tag),
            Some((DAMAGE_RESISTANT, Component::Raw(bytes)))
        );
    }

    #[test]
    fn the_component_list_is_the_registry() {
        assert_eq!(
            COMPONENTS[component::TOOLTIP_DISPLAY as usize],
            "tooltip_display"
        );
        assert_eq!(COMPONENTS[component::PROFILE as usize], "profile");
        assert_eq!(COMPONENTS[GLIDER as usize], "glider");
        assert_eq!(COMPONENTS[ENCHANTABLE as usize], "enchantable");
        assert_eq!(COMPONENTS[TOOLTIP_STYLE as usize], "tooltip_style");
        assert_eq!(COMPONENTS[DAMAGE_RESISTANT as usize], "damage_resistant");
    }
}
