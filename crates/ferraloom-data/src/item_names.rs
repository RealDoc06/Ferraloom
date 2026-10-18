//! Immutable Item#getDescriptionId and Potion#name facts from vanilla registries.
impl crate::Item {
    pub const fn description_id(self) -> &'static str {
        crate::ITEM_DESCRIPTION_IDS[self.protocol_id() as usize]
    }
}
impl crate::registries::potion::Id {
    pub const fn effect_name(self) -> &'static str {
        crate::POTION_EFFECT_NAMES[self.protocol_id() as usize]
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_registry_names_keep_block_descriptions_and_shared_potion_suffixes() {
        use crate::{Item, registries::potion};
        assert_eq!(Item::STONE.description_id(), "block.minecraft.stone");
        assert_eq!(
            Item::PLAYER_HEAD.description_id(),
            "block.minecraft.player_head"
        );
        assert_eq!(Item::SHIELD.description_id(), "item.minecraft.shield");
        assert_eq!(potion::HEALING.effect_name(), "healing");
        assert_eq!(potion::STRONG_HEALING.effect_name(), "healing");
        assert_eq!(potion::LONG_SWIFTNESS.effect_name(), "swiftness");
    }
}
