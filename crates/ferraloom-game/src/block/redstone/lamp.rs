//! Port of RedstoneLampBlock: immediate lighting and four-tick delayed darkness.
use crate::{
    block::{BlockBehavior, UPDATE_LIMIT, UpdateFlags},
    level::{Level, LevelError, redstone::Orientation},
};
use ferraloom_data::{Block, BlockState, properties as p};
use ferraloom_util::BlockPos;
pub struct RedstoneLampBlock;
pub static LAMP: RedstoneLampBlock = RedstoneLampBlock;
impl BlockBehavior for RedstoneLampBlock {
    fn neighbor_changed(
        &self,
        state: BlockState,
        pos: BlockPos,
        _source: Block,
        _orientation: Option<Orientation>,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let lit = state.get(p::LIT).unwrap();
        if lit != level.has_neighbor_signal(pos)? {
            if lit {
                level.schedule_block_tick_after(pos, state.block(), 4)?;
            } else {
                level.set_block(
                    pos,
                    state.with(p::LIT, true).unwrap(),
                    UpdateFlags::CLIENTS,
                    UPDATE_LIMIT,
                )?;
            }
        }
        Ok(())
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        if state.get(p::LIT).unwrap() && !level.has_neighbor_signal(pos)? {
            level.set_block(
                pos,
                state.with(p::LIT, false).unwrap(),
                UpdateFlags::CLIENTS,
                UPDATE_LIMIT,
            )?;
        }
        Ok(())
    }
}
