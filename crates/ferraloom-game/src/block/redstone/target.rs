//! TargetBlock's placement reset and scheduled signal recovery. Projectile
//! impacts remain pending the canonical projectile dispatch and entity hooks.
use crate::{
    block::{BlockBehavior, UPDATE_LIMIT, UpdateFlags},
    level::{Level, LevelError},
};
use ferraloom_data::{BlockState, properties as p};
use ferraloom_util::BlockPos;

pub struct TargetBlock;
pub static TARGET: TargetBlock = TargetBlock;

impl BlockBehavior for TargetBlock {
    /// Port of TargetBlock#onPlace. An existing scheduled pulse survives a new
    /// powered state; otherwise a newly placed target loses its stored power.
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if old.block() != state.block()
            && state.get(p::POWER).unwrap() > 0
            && !level.has_scheduled_block_tick(pos, state.block())
        {
            level.set_block(
                pos,
                state.with(p::POWER, 0).unwrap(),
                UpdateFlags(18),
                UPDATE_LIMIT,
            )?;
        }
        Ok(())
    }

    /// Port of TargetBlock#tick; the impact hook owns the 8/20-tick delay.
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        if state.get(p::POWER).unwrap() != 0 {
            level.set_block_and_update(pos, state.with(p::POWER, 0).unwrap())?;
        }
        Ok(())
    }
}
