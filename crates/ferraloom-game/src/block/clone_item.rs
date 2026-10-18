//! Required pick-block query shared by live simulation and worldgen readers.
use super::{BlockMethod, LevelReaderRef};
use crate::level::{Level, LevelError};
use ferraloom_data::BlockState;
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::BlockPos;

impl LevelReaderRef<'_> {
    pub fn get_clone_item_stack(
        &self,
        state: BlockState,
        pos: BlockPos,
        include_data: bool,
    ) -> Result<ItemStack, LevelError> {
        let behavior = match self {
            Self::Server(level) => {
                level.required_block_behavior(state.block(), BlockMethod::CloneItemStack)?
            }
        };
        behavior.get_clone_item_stack(state, pos, include_data, self)
    }
}
impl Level {
    pub fn get_clone_item_stack(
        &self,
        state: BlockState,
        pos: BlockPos,
        include_data: bool,
    ) -> Result<ItemStack, LevelError> {
        LevelReaderRef::Server(self).get_clone_item_stack(state, pos, include_data)
    }
}
