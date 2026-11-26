//! Item tooltip formatting: equipment modifiers with a player's base values,
//! and lines without player context for the recipe book's search tree.
use crate::items::ItemStack;

fn is_potion_item(item: &str) -> bool {
    item.ends_with("potion") || item == "minecraft:tipped_arrow"
}

/// The name line of `stack`, as the item's translation or, for potions and
/// smithing templates, its name component gives it.
pub fn name_line(stack: &ItemStack, translate: &dyn Fn(&str) -> Option<String>) -> String {
    let path = stack.item.trim_start_matches("minecraft:");
    if let Some(name) = stack.custom_name() {
        return name.to_owned();
    }
    if let Some(name) = stack.item_name() {
        return name;
    }
    if stack.data().is_some_and(|d| d.ominous_banner) {
        return translate("block.minecraft.ominous_banner")
            .unwrap_or_else(|| "Ominous Banner".to_owned());
    }
    let key = if is_potion_item(&stack.item) {
        Some(crate::brewing::potion_name_key(&stack.item, stack.potion()))
    } else if let Some(pattern) = path.strip_suffix("_armor_trim_smithing_template") {
        Some(format!("trim_pattern.minecraft.{pattern}"))
    } else if path == "netherite_upgrade_smithing_template" {
        Some("upgrade.minecraft.netherite_upgrade".to_owned())
    } else if path.ends_with("_banner_pattern") {
        // The item name component of the pattern items that have one.
        Some(format!("item.minecraft.{path}.new"))
    } else {
        None
    };
    key.and_then(|k| translate(&k))
        .or_else(|| translate(&format!("item.minecraft.{path}")))
        .or_else(|| translate(&format!("block.minecraft.{path}")))
        .unwrap_or_else(|| path.replace('_', " "))
}

/// ItemStack.getRarity: the ominous banner override, then the enchantment upgrade.
pub fn rarity<'a>(stack: &ItemStack, default: &'a str) -> &'a str {
    let rarity = if stack.data().is_some_and(|d| d.ominous_banner) {
        "uncommon"
    } else {
        default
    };
    if stack.enchantments().is_empty() {
        rarity
    } else {
        match rarity {
            "common" | "uncommon" => "rare",
            "rare" => "epic",
            r => r,
        }
    }
}
