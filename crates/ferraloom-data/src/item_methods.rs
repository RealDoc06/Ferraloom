use crate::Item;

include!(concat!(env!("OUT_DIR"), "/item_methods.rs"));

impl Item {
    pub const fn class(self) -> ItemClass {
        ITEM_CLASSES[self.protocol_id() as usize]
    }
    pub const fn is_instance_of(self, class: ItemClass) -> bool {
        ITEM_CLASS_INSTANCES[self.class() as usize][class as usize / 64]
            & (1 << (class as usize % 64))
            != 0
    }
    /// Whether the actual implementation differs from this signature's base.
    /// An absent signature has no owner; callers must check applicability first.
    pub const fn overrides(self, method: ItemMethod) -> bool {
        match method.owner(self) {
            Some(owner) => owner as u16 != method.base() as u16,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_item_classes_and_exact_virtual_owners_are_distinct() {
        assert_eq!(Item::STONE.class(), ItemClass::BlockItem);
        assert_eq!(Item::OAK_SIGN.class(), ItemClass::StandingAndWallBlockItem);
        assert_eq!(Item::OAK_HANGING_SIGN.class(), ItemClass::HangingSignItem);
        assert_eq!(Item::SCAFFOLDING.class(), ItemClass::ScaffoldingBlockItem);
        assert_eq!(Item::SUNFLOWER.class(), ItemClass::DoubleHighBlockItem);
        assert!(Item::OAK_SIGN.is_instance_of(ItemClass::StandingAndWallBlockItem));
        assert!(Item::OAK_SIGN.is_instance_of(ItemClass::BlockItem));
        assert!(!Item::APPLE.is_instance_of(ItemClass::BlockItem));
        assert_eq!(
            ItemMethod::UseOn.owner(Item::STONE),
            Some(ItemClass::BlockItem)
        );
        assert_eq!(
            ItemMethod::UpdatePlacementContext.owner(Item::SCAFFOLDING),
            Some(ItemClass::ScaffoldingBlockItem)
        );
        assert_eq!(
            ItemMethod::PlaceBlock.owner(Item::SUNFLOWER),
            Some(ItemClass::DoubleHighBlockItem)
        );
        assert!(!Item::STONE.overrides(ItemMethod::PlaceBlock));
        assert!(Item::SUNFLOWER.overrides(ItemMethod::PlaceBlock));
        assert_eq!(ItemMethod::PlaceBlock.owner(Item::APPLE), None);
        assert!(!Item::APPLE.overrides(ItemMethod::UseOn));
    }
}
