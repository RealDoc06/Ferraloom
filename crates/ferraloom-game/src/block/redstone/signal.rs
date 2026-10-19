//! Port of SignalGetter. Direction arguments point from the receiver to the
//! queried source; direct power is relayed through conductors exactly once.
use crate::{
    block::{BlockBehavior, BlockMethod, LevelReaderRef},
    level::{Level, LevelError},
};
use ferraloom_data::{Block, BlockClass, BlockState, properties};
use ferraloom_util::{BlockPos, Direction};

impl LevelReaderRef<'_, '_> {
    pub fn is_redstone_conductor(
        &self,
        state: BlockState,
        pos: BlockPos,
    ) -> Result<bool, LevelError> {
        if let Some(conductor) = state.cached_redstone_conductor() {
            return Ok(conductor);
        }
        // Blocks registers these dynamic-shape blocks with Blocks::never.
        // Shulker boxes retain the default live collision-full predicate.
        match state.block().class() {
            BlockClass::MovingPistonBlock
            | BlockClass::BambooStalkBlock
            | BlockClass::ScaffoldingBlock
            | BlockClass::PowderSnowBlock
            | BlockClass::PointedDripstoneBlock
            | BlockClass::SulfurSpikeBlock => Ok(false),
            BlockClass::ShulkerBoxBlock => self.is_collision_shape_full_block(state, pos),
            _ => Err(LevelError::UnsupportedShape(state)),
        }
    }
    pub fn is_signal_source(&self, state: BlockState) -> Result<bool, LevelError> {
        if state.block() == Block::REDSTONE_WIRE && !self.wire_signals_enabled() {
            return Ok(false);
        }
        Ok(self
            .required_behavior(state.block(), BlockMethod::IsSignalSource)?
            .is_signal_source(state))
    }
    pub fn own_signal(&self, state: BlockState, pos: BlockPos) -> Result<i32, LevelError> {
        self.required_behavior(state.block(), BlockMethod::OwnSignal)?
            .own_signal(state, pos, self)
    }
    pub fn block_signal(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
    ) -> Result<i32, LevelError> {
        self.required_behavior(state.block(), BlockMethod::Signal)?
            .signal(state, pos, direction, self)
    }
    pub fn get_direct_signal(
        &self,
        pos: BlockPos,
        direction: Direction,
    ) -> Result<i32, LevelError> {
        let state = self.get_block_state(pos)?;
        self.required_behavior(state.block(), BlockMethod::DirectSignal)?
            .direct_signal(state, pos, direction, self)
    }
    pub fn get_direct_signal_to(&self, pos: BlockPos) -> Result<i32, LevelError> {
        let mut best = 0;
        for direction in Direction::VALUES {
            best = best.max(self.get_direct_signal(pos.relative(direction, 1), direction)?);
            if best >= 15 {
                return Ok(best);
            }
        }
        Ok(best)
    }
    pub fn get_signal(&self, pos: BlockPos, direction: Direction) -> Result<i32, LevelError> {
        let state = self.get_block_state(pos)?;
        let signal = self.block_signal(state, pos, direction)?;
        Ok(if self.is_redstone_conductor(state, pos)? {
            signal.max(self.get_direct_signal_to(pos)?)
        } else {
            signal
        })
    }
    pub fn get_control_input_signal(
        &self,
        pos: BlockPos,
        direction: Direction,
        only_diodes: bool,
    ) -> Result<i32, LevelError> {
        let state = self.get_block_state(pos)?;
        if only_diodes {
            return if state.block().is_instance_of(BlockClass::DiodeBlock) {
                self.get_direct_signal(pos, direction)
            } else {
                Ok(0)
            };
        }
        match state.block() {
            Block::REDSTONE_BLOCK => Ok(15),
            Block::REDSTONE_WIRE => Ok(i32::from(state.get(properties::POWER).unwrap())),
            _ if self.is_signal_source(state)? => self.get_direct_signal(pos, direction),
            _ => Ok(0),
        }
    }
    pub fn has_signal(&self, pos: BlockPos, direction: Direction) -> Result<bool, LevelError> {
        Ok(self.get_signal(pos, direction)? > 0)
    }
    pub fn has_neighbor_signal(&self, pos: BlockPos) -> Result<bool, LevelError> {
        for direction in Direction::VALUES {
            if self.has_signal(pos.relative(direction, 1), direction)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub fn get_best_neighbor_signal(&self, pos: BlockPos) -> Result<i32, LevelError> {
        let mut best = 0;
        for direction in Direction::VALUES {
            let signal = self.get_signal(pos.relative(direction, 1), direction)?;
            if signal >= 15 {
                return Ok(15);
            }
            best = best.max(signal);
        }
        Ok(best)
    }
    pub fn get_best_own_or_neighbor_signal(&self, pos: BlockPos) -> Result<i32, LevelError> {
        let state = self.get_block_state(pos)?;
        // Vanilla evaluates the neighbor query before the own-signal query.
        let neighbors = self.get_best_neighbor_signal(pos)?;
        Ok(neighbors.max(if self.is_signal_source(state)? {
            self.own_signal(state, pos)?
        } else {
            0
        }))
    }
    pub fn should_redstone_wire_connect_to(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Option<Direction>,
    ) -> Result<bool, LevelError> {
        self.required_behavior(state.block(), BlockMethod::ShouldRedstoneWireConnectTo)?
            .should_redstone_wire_connect_to(state, pos, direction, self)
    }
    pub(super) fn wire_signals_enabled(&self) -> bool {
        match self {
            Self::Server(level) => !level.region().redstone.wire_signals_suppressed,
            Self::PathNavigation(reader) => reader.wire_signals_enabled(),
            Self::Worldgen(_) => true,
        }
    }
}

impl Level {
    pub fn is_redstone_conductor(
        &self,
        state: BlockState,
        pos: BlockPos,
    ) -> Result<bool, LevelError> {
        LevelReaderRef::Server(self).is_redstone_conductor(state, pos)
    }
    pub fn get_signal(&self, pos: BlockPos, direction: Direction) -> Result<i32, LevelError> {
        LevelReaderRef::Server(self).get_signal(pos, direction)
    }
    pub fn get_direct_signal(
        &self,
        pos: BlockPos,
        direction: Direction,
    ) -> Result<i32, LevelError> {
        LevelReaderRef::Server(self).get_direct_signal(pos, direction)
    }
    pub fn get_direct_signal_to(&self, pos: BlockPos) -> Result<i32, LevelError> {
        LevelReaderRef::Server(self).get_direct_signal_to(pos)
    }
    pub fn get_control_input_signal(
        &self,
        pos: BlockPos,
        direction: Direction,
        only_diodes: bool,
    ) -> Result<i32, LevelError> {
        LevelReaderRef::Server(self).get_control_input_signal(pos, direction, only_diodes)
    }
    pub fn has_signal(&self, pos: BlockPos, direction: Direction) -> Result<bool, LevelError> {
        LevelReaderRef::Server(self).has_signal(pos, direction)
    }
    pub fn has_neighbor_signal(&self, pos: BlockPos) -> Result<bool, LevelError> {
        LevelReaderRef::Server(self).has_neighbor_signal(pos)
    }
    pub fn get_best_neighbor_signal(&self, pos: BlockPos) -> Result<i32, LevelError> {
        LevelReaderRef::Server(self).get_best_neighbor_signal(pos)
    }
    pub fn get_best_own_or_neighbor_signal(&self, pos: BlockPos) -> Result<i32, LevelError> {
        LevelReaderRef::Server(self).get_best_own_or_neighbor_signal(pos)
    }
    pub fn get_analog_output_signal(
        &mut self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
    ) -> Result<i32, LevelError> {
        self.required_block_behavior(state.block(), BlockMethod::AnalogOutputSignal)?
            .analog_output_signal(state, pos, direction, self)
    }
}

/// Port of PoweredBlock, the redstone block's weak-power-only source.
pub(super) struct PoweredBlock;
impl BlockBehavior for PoweredBlock {
    fn is_signal_source(&self, _state: BlockState) -> bool {
        true
    }
    fn own_signal(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        Ok(15)
    }
}
