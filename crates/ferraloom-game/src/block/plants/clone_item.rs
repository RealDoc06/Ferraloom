//! Ports of plant getCloneItemStack overrides; generated item registries retain
//! vanilla's built-in seed and head items for these blocks.
use super::super::{BlockBehavior, LevelReaderRef};
use crate::level::LevelError;
use ferraloom_data::{Block, BlockState, Item};
use ferraloom_protocol::component::{DataComponentPatch, ItemStack};
use ferraloom_util::BlockPos;

pub struct PlantClone;
pub static PLANT_CLONE: PlantClone = PlantClone;

pub(super) fn item(block: Block) -> Option<Item> {
    Some(match block {
        Block::WHEAT => Item::WHEAT_SEEDS,
        Block::CARROTS => Item::CARROT,
        Block::POTATOES => Item::POTATO,
        Block::BEETROOTS => Item::BEETROOT_SEEDS,
        Block::TORCHFLOWER_CROP => Item::TORCHFLOWER_SEEDS,
        Block::MELON_STEM | Block::ATTACHED_MELON_STEM => Item::MELON_SEEDS,
        Block::PUMPKIN_STEM | Block::ATTACHED_PUMPKIN_STEM => Item::PUMPKIN_SEEDS,
        Block::SWEET_BERRY_BUSH => Item::SWEET_BERRIES,
        Block::NETHER_WART => Item::NETHER_WART,
        Block::BAMBOO_SAPLING => Item::BAMBOO,
        Block::CAVE_VINES | Block::CAVE_VINES_PLANT => Item::GLOW_BERRIES,
        Block::BIG_DRIPLEAF_STEM => Item::BIG_DRIPLEAF,
        Block::TALL_SEAGRASS => Item::SEAGRASS,
        Block::KELP_PLANT => Item::KELP,
        Block::TWISTING_VINES_PLANT => Item::TWISTING_VINES,
        Block::WEEPING_VINES_PLANT => Item::WEEPING_VINES,
        _ => return None,
    })
}

impl BlockBehavior for PlantClone {
    fn get_clone_item_stack(
        &self,
        state: BlockState,
        _pos: BlockPos,
        _include_data: bool,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<ItemStack, LevelError> {
        Ok(ItemStack::new(
            item(state.block()).unwrap(),
            1,
            DataComponentPatch::default(),
        ))
    }
}
