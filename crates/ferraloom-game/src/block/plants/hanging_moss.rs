//! Port of HangingMossBlock attachment, tip state and support-loss ticks.
use super::super::{BlockBehavior, LevelAccessRef, LevelReaderRef};
use crate::level::{Level, LevelError};
use ferraloom_data::{BlockState, properties as p};
use ferraloom_util::{BlockPos, Direction};
pub struct HangingMoss;
pub static HANGING_MOSS: HangingMoss = HangingMoss;
impl BlockBehavior for HangingMoss {
    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let above = level.get_block_state(pos.above(1))?;
        Ok(
            super::multiface::can_attach_to_neighbor(level, above, pos.above(1), Direction::Up)?
                || above.block() == state.block(),
        )
    }
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        if !self.can_survive(state, pos, &level.as_reader())? {
            level.schedule_block_tick(pos, state.block(), 1)?;
        }
        Ok(state
            .with(
                p::TIP,
                level.get_block_state(pos.below(1))?.block() != state.block(),
            )
            .unwrap())
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        if !self.can_survive(state, pos, &LevelReaderRef::Server(level))? {
            level.destroy_block(pos, true, super::super::UPDATE_LIMIT)?;
        }
        Ok(())
    }
}
pub fn growth_position(
    state: BlockState,
    mut pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
) -> Result<BlockPos, LevelError> {
    loop {
        pos = pos.below(1);
        if level.get_block_state(pos)?.block() != state.block() {
            return Ok(pos);
        }
    }
}
