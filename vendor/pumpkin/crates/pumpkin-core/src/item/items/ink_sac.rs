use std::sync::Arc;

use crate::block::entities::{BlockEntity, sign::SignEntityRef};
use pumpkin_data::item::Item;

use crate::{
    block::{UseWithItemArgs, registry::BlockActionResult},
    item::{ItemBehaviour, ItemMetadata},
};

pub struct InkSacItem;

impl ItemMetadata for InkSacItem {
    fn ids() -> Box<[u16]> {
        [Item::INK_SAC.id].into()
    }
}

impl ItemBehaviour for InkSacItem {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl InkSacItem {
    pub fn apply_to_sign(
        &self,
        args: &UseWithItemArgs<'_>,
        block_entity: &Arc<dyn BlockEntity>,
        sign: &SignEntityRef<'_>,
        is_front: bool,
    ) -> BlockActionResult {
        let changed = sign.apply_glowing(args.player.gameprofile.id, is_front, false);

        if !changed {
            return BlockActionResult::PassToDefaultBlockAction;
        }

        args.world.update_block_entity(block_entity);
        args.world.play_block_sound(
            pumpkin_data::sound::Sound::ItemInkSacUse,
            pumpkin_data::sound::SoundCategory::Blocks,
            *args.position,
        );
        BlockActionResult::Success
    }
}
