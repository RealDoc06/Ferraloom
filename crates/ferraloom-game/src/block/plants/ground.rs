//! Ports of SnowyBlock updates and FarmlandBlock/PathBlock support checks.
use super::super::{BlockBehavior, LevelAccessRef, LevelReaderRef};
use crate::level::LevelError;
use ferraloom_data::{BlockClass, BlockState, properties as p, tags::BlockTags};
use ferraloom_util::{BlockPos, Direction};
pub struct Ground;
pub static GROUND: Ground = Ground;
impl BlockBehavior for Ground {
    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let above = level.get_block_state(pos.above(1))?;
        Ok(!above.is_solid()
            || if state.block().class() == BlockClass::FarmlandBlock {
                level
                    .block_tags()?
                    .contains_key(BlockTags::MAINTAINS_FARMLAND, above.block())
            } else {
                above.block().is_instance_of(BlockClass::FenceGateBlock)
            })
    }
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        if state.get(p::SNOWY).is_some() {
            return Ok(if direction == Direction::Up {
                state
                    .with(
                        p::SNOWY,
                        level
                            .as_reader()
                            .block_tags()?
                            .contains_key(BlockTags::SNOW, neighbor.block()),
                    )
                    .unwrap()
            } else {
                state
            });
        }
        if direction == Direction::Up && !self.can_survive(state, pos, &level.as_reader())? {
            level.schedule_block_tick(pos, state.block(), 1)?;
        }
        Ok(state)
    }
}
