//! Frosted ice callbacks independent of the pending environment-attribute query.
use super::super::{BlockBehavior, LevelReaderRef};
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockState};
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::{BlockPos, random::RandomSource};

pub struct FrostedIce;
pub static FROSTED_ICE: FrostedIce = FrostedIce;
impl BlockBehavior for FrostedIce {
    /// Port of FrostedIceBlock#onPlace. Draw from the live level's canonical
    /// stream even when the pending-tick container already contains this tick.
    fn on_place(
        &self,
        _state: BlockState,
        pos: BlockPos,
        _old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let delay = 60 + level.random().next_int_bound(61);
        level.schedule_block_tick_after(pos, Block::FROSTED_ICE, delay)
    }
    fn get_clone_item_stack(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _include_data: bool,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<ItemStack, LevelError> {
        Ok(ItemStack::default())
    }
}
