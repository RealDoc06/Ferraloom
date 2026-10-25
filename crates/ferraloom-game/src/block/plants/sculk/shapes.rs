//! Shape updates shared by sculk sensor variants and shriekers.
use crate::{
    block::{BlockBehavior, LevelAccessRef, plants::survival::schedule_water},
    level::LevelError,
};
use ferraloom_data::BlockState;
use ferraloom_util::{BlockPos, Direction};

pub(in crate::block::plants) struct WaterloggedSculk;
pub(in crate::block::plants) static WATERLOGGED_SCULK: WaterloggedSculk = WaterloggedSculk;
impl BlockBehavior for WaterloggedSculk {
    /// Port of SculkSensorBlock/SculkShriekerBlock#updateShape. Calibrated
    /// sensors inherit the sensor body; scheduling precedes the base virtual.
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        neighbor_pos: BlockPos,
        neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        schedule_water(state, pos, level)?;
        crate::block::BASE_BLOCK.update_shape(state, pos, direction, neighbor_pos, neighbor, level)
    }
}

#[cfg(test)]
mod tests;
