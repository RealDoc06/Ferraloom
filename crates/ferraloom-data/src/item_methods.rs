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
    /// Immutable StandingAndWallBlockItem constructor's alternate block.
    pub const fn wall_block(self) -> Option<crate::Block> {
        ITEM_WALL_BLOCK[self.protocol_id() as usize]
    }
    /// Immutable StandingAndWallBlockItem constructor's attachment direction.
    pub const fn attachment_direction(self) -> Option<ferraloom_util::Direction> {
        ITEM_ATTACHMENT_DIRECTION[self.protocol_id() as usize]
    }
    /// Immutable SolidBucketItem constructor's placement sound.
    pub const fn place_sound(self) -> Option<crate::registries::sound_event::Id> {
        ITEM_PLACE_SOUND[self.protocol_id() as usize]
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
        assert_eq!(
            Item::OAK_SIGN.wall_block(),
            Some(crate::Block::OAK_WALL_SIGN)
        );
        assert_eq!(
            Item::OAK_SIGN.attachment_direction(),
            Some(ferraloom_util::Direction::Down)
        );
        assert_eq!(
            Item::OAK_HANGING_SIGN.attachment_direction(),
            Some(ferraloom_util::Direction::Up)
        );
        assert_eq!(
            Item::POWDER_SNOW_BUCKET.place_sound(),
            Some(crate::registries::sound_event::Id::BUCKET_EMPTY_POWDER_SNOW)
        );
        assert_eq!(Item::STONE.wall_block(), None);
        assert_eq!(Item::STONE.place_sound(), None);
        assert_eq!(
            ItemMethod::CanPlaceStandingOrWall.owner(Item::OAK_SIGN),
            Some(ItemClass::StandingAndWallBlockItem)
        );
        assert_eq!(
            ItemMethod::CanPlaceStandingOrWall.owner(Item::OAK_HANGING_SIGN),
            Some(ItemClass::HangingSignItem)
        );
        assert_eq!(ItemMethod::CanPlaceStandingOrWall.owner(Item::STONE), None);
    }
}
