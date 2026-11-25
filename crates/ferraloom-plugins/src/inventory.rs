//! The inventory area: item stacks across JNI, inventories and their events
//! (`java/bridge/.../InventoryEvents.java`, `InventoryNative.java`).
//!
//! An item crosses as one `byte[]`: `ItemStack.CODEC`'s compound (`id`,
//! `count`, `components`) in network NBT, null for an empty slot. Java reads
//! it into a Bukkit `ItemStack` whose meta keeps the component patch as NBT,
//! so components neither side models come back as they went
//! (`Items.java`). Converting allocates on both sides: event calls pass
//! items only when the event needs them, and the Bukkit API fetches the rest
//! when a plugin asks.
use crate::natives::Fail;
use horizon_items::{item_nbt, items::ItemStack};
use horizon_world::nbt::{self, Limits};
use jni::{
    JNIEnv,
    objects::{JByteArray, JObject},
};

/// A stack as Java's `Items.read` takes it; empty for none.
pub fn encode(stack: Option<&ItemStack>) -> Vec<u8> {
    let Some(stack) = stack.filter(|s| s.count > 0) else {
        return Vec::new();
    };
    // An item's compound always encodes: the limits are the default's.
    nbt::write_network(&item_nbt::item_to_nbt(stack), Limits::default()).unwrap_or_default()
}

/// What Java's `Items.write` gave; none for empty bytes, air, a zero count
/// or bytes that are not an item.
pub fn decode(bytes: &[u8]) -> Option<ItemStack> {
    if bytes.is_empty() {
        return None;
    }
    let (tag, _) = nbt::read_network(bytes, Limits::default()).ok()?;
    let count = tag.get("count").and_then(|c| match c {
        nbt::Tag::Byte(v) => Some(i32::from(*v)),
        nbt::Tag::Short(v) => Some(i32::from(*v)),
        nbt::Tag::Int(v) => Some(*v),
        _ => None,
    });
    if count.is_some_and(|c| c <= 0) {
        return None;
    }
    item_nbt::item_from_nbt(&tag)
}

/// A stack as a Java `byte[]`, or null for none.
#[allow(dead_code)] // Until the areas' first event call passes an item.
pub(crate) fn to_java<'l>(
    env: &mut JNIEnv<'l>,
    stack: Option<&ItemStack>,
) -> jni::errors::Result<JObject<'l>> {
    let bytes = encode(stack);
    if bytes.is_empty() {
        return Ok(JObject::null());
    }
    Ok(env.byte_array_from_slice(&bytes)?.into())
}

/// A Java `byte[]` (or null) as a stack.
#[allow(dead_code)] // Until the first native takes an item.
pub(crate) fn from_java(env: &JNIEnv, bytes: &JByteArray) -> Result<Option<ItemStack>, Fail> {
    if bytes.is_null() {
        return Ok(None);
    }
    Ok(decode(&env.convert_byte_array(bytes)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_world::nbt::Tag;

    /// A sword as a plugin leaves it: enchanted, damaged and named (modelled
    /// by Horizon), with lore, plugin data and a removed component (not).
    fn plugin_sword() -> ItemStack {
        let components = Tag::compound([
            ("minecraft:damage", Tag::Int(12)),
            (
                "minecraft:enchantments",
                Tag::compound([("minecraft:sharpness", Tag::Int(5))]),
            ),
            ("minecraft:custom_name", Tag::String("Blade".into())),
            (
                "minecraft:lore",
                Tag::List(vec![Tag::compound([
                    ("text", Tag::String("Sharp".into())),
                    ("color", Tag::String("gold".into())),
                    ("italic", Tag::Byte(0)),
                ])]),
            ),
            (
                "minecraft:custom_data",
                Tag::compound([(
                    "PublicBukkitValues",
                    Tag::compound([("shop:price", Tag::Long(250))]),
                )]),
            ),
            ("!minecraft:tool", Tag::Compound(Default::default())),
        ]);
        let tag = Tag::compound([
            ("id", Tag::String("minecraft:diamond_sword".into())),
            ("count", Tag::Int(1)),
            ("components", components),
        ]);
        item_nbt::item_from_nbt(&tag).unwrap()
    }

    #[test]
    fn a_stack_survives_the_round_trip_with_the_components_it_does_not_model() {
        let sword = plugin_sword();
        let back = decode(&encode(Some(&sword))).unwrap();
        assert_eq!(back, sword);
        assert_eq!(back.damage, 12);
        assert_eq!(back.enchantment_level("minecraft:sharpness"), 5);
        let data = back.data().unwrap();
        assert!(data.other("minecraft:lore").is_some());
        assert!(data.other("!minecraft:tool").is_some());
        let price = data
            .other("minecraft:custom_data")
            .and_then(|d| d.get("PublicBukkitValues"))
            .and_then(|v| v.get("shop:price"));
        assert_eq!(price, Some(&Tag::Long(250)));
    }

    #[test]
    fn empty_stacks_cross_as_nothing() {
        assert!(encode(None).is_empty());
        assert!(encode(Some(&ItemStack::new("minecraft:stone", 0))).is_empty());
        assert_eq!(decode(&[]), None);
        assert_eq!(decode(&[1, 2, 3]), None);
        let air = Tag::compound([
            ("id", Tag::String("minecraft:air".into())),
            ("count", Tag::Int(1)),
        ]);
        assert_eq!(
            decode(&nbt::write_network(&air, Limits::default()).unwrap()),
            None
        );
        // Bukkit lets a plugin set an amount of zero: that slot is empty.
        let none = Tag::compound([
            ("id", Tag::String("minecraft:stone".into())),
            ("count", Tag::Int(0)),
        ]);
        assert_eq!(
            decode(&nbt::write_network(&none, Limits::default()).unwrap()),
            None
        );
        let stone = decode(&encode(Some(&ItemStack::new("minecraft:stone", 64)))).unwrap();
        assert_eq!(
            (&*stone.item, stone.count, stone.extra),
            ("minecraft:stone", 64, None)
        );
    }
}
