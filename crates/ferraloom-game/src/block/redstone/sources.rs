//! State-only signal methods shared by source classes. Their interaction,
//! detection and scheduled-tick overrides register separately when ported.
use super::state::facing;
use crate::{
    block::{BlockBehavior, LevelReaderRef},
    level::LevelError,
};
use ferraloom_data::{BlockClass, BlockState, properties as p};
use ferraloom_util::{BlockPos, Direction};
pub struct Sources;
pub static SOURCES: Sources = Sources;
pub(super) fn connected_direction(state: BlockState) -> Direction {
    match state.get(p::FACE).unwrap() {
        p::Face::Floor => Direction::Up,
        p::Face::Ceiling => Direction::Down,
        p::Face::Wall => facing(state),
    }
}
impl BlockBehavior for Sources {
    fn is_signal_source(&self, _state: BlockState) -> bool {
        true
    }
    fn own_signal(
        &self,
        state: BlockState,
        _pos: BlockPos,
        _reader: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        Ok(
            if state
                .block()
                .is_instance_of(BlockClass::WeightedPressurePlateBlock)
                || state
                    .block()
                    .is_instance_of(BlockClass::DaylightDetectorBlock)
                || state.block().is_instance_of(BlockClass::SculkSensorBlock)
                || state.block().is_instance_of(BlockClass::TargetBlock)
            {
                i32::from(state.get(p::POWER).unwrap())
            } else if state.get(p::POWERED).unwrap() {
                15
            } else {
                0
            },
        )
    }
    fn direct_signal(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        let class = state.block().class();
        let connected = match class {
            BlockClass::LeverBlock | BlockClass::ButtonBlock => connected_direction(state),
            BlockClass::LightningRodBlock | BlockClass::TripWireHookBlock => facing(state),
            _ => Direction::Up,
        };
        if direction == connected {
            self.own_signal(state, pos, reader)
        } else {
            Ok(0)
        }
    }
    fn signal(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        if state.block().class() == BlockClass::CalibratedSculkSensorBlock
            && direction == facing(state)
        {
            Ok(0)
        } else {
            self.own_signal(state, pos, reader)
        }
    }
}
