//! Temporary runtime policy while block families are ported. Tests and parity
//! remain strict; Warn records a skipped call, never a fabricated return value.
use super::BlockBehavior;
use crate::level::{Level, LevelError};
use ferraloom_data::Block;
use std::collections::HashSet;
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UnportedBehavior {
    #[default]
    Strict,
    Warn,
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BlockMethod {
    OnPlace,
    AffectNeighborsAfterRemoval,
    NeighborChanged,
    EntityInside,
    FallOn,
    StepOn,
    BounceOn,
    UpdateShape,
    UpdateIndirectNeighborShapes,
    Tick,
    RandomTick,
    FluidTick,
    FluidRandomTick,
    BubbleColumnUpdate,
    CanSurvive,
    CanPlaceLiquid,
    PlaceLiquid,
    PickupBlock,
}
impl BlockMethod {
    const fn can_skip(self) -> bool {
        !matches!(
            self,
            Self::CanSurvive | Self::CanPlaceLiquid | Self::PlaceLiquid | Self::PickupBlock
        )
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::OnPlace => "onPlace",
            Self::AffectNeighborsAfterRemoval => "affectNeighborsAfterRemoval",
            Self::NeighborChanged => "neighborChanged",
            Self::EntityInside => "entityInside",
            Self::FallOn => "fallOn",
            Self::StepOn => "stepOn",
            Self::BounceOn => "bounceOn",
            Self::UpdateShape => "updateShape",
            Self::UpdateIndirectNeighborShapes => "updateIndirectNeighbourShapes",
            Self::Tick => "tick",
            Self::RandomTick => "randomTick",
            Self::FluidTick => "fluid.tick",
            Self::FluidRandomTick => "fluid.randomTick",
            Self::BubbleColumnUpdate => "BubbleColumnBlock.updateColumn",
            Self::CanSurvive => "canSurvive",
            Self::CanPlaceLiquid => "canPlaceLiquid",
            Self::PlaceLiquid => "placeLiquid",
            Self::PickupBlock => "pickupBlock",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UnportedBehaviorStats {
    pub skipped_calls: u64,
    pub distinct_calls: usize,
}
#[derive(Default)]
pub(crate) struct UnportedCalls {
    pub policy: UnportedBehavior,
    seen: HashSet<(Block, BlockMethod)>,
    stats: UnportedBehaviorStats,
}
impl Level {
    /// Dispatch the captured inside-block state with the configured unsupported
    /// behavior policy. Movement must not call a silent superclass substitute.
    pub fn call_entity_inside(
        &mut self,
        state: ferraloom_data::BlockState,
        pos: ferraloom_util::BlockPos,
        entity: crate::entity::EntityRef,
    ) -> Result<(), LevelError> {
        self.optional_block_call(
            state.block(),
            BlockMethod::EntityInside,
            |behavior, level| behavior.entity_inside(state, pos, entity, level),
        )?;
        Ok(())
    }
    pub fn call_fall_on(
        &mut self,
        state: ferraloom_data::BlockState,
        pos: ferraloom_util::BlockPos,
        entity: crate::entity::EntityRef,
        fall_distance: f64,
    ) -> Result<(), LevelError> {
        self.optional_block_call(state.block(), BlockMethod::FallOn, |behavior, level| {
            behavior.fall_on(state, pos, entity, fall_distance, level)
        })?;
        Ok(())
    }
    pub fn call_step_on(
        &mut self,
        state: ferraloom_data::BlockState,
        pos: ferraloom_util::BlockPos,
        entity: crate::entity::EntityRef,
    ) -> Result<(), LevelError> {
        self.optional_block_call(state.block(), BlockMethod::StepOn, |behavior, level| {
            behavior.step_on(state, pos, entity, level)
        })?;
        Ok(())
    }
    pub fn call_bounce_on(
        &mut self,
        state: ferraloom_data::BlockState,
        pos: ferraloom_util::BlockPos,
        entity: crate::entity::EntityRef,
        fall_distance: f64,
    ) -> Result<(), LevelError> {
        self.optional_block_call(state.block(), BlockMethod::BounceOn, |behavior, level| {
            behavior.bounce_on(state, pos, entity, fall_distance, level)
        })?;
        Ok(())
    }
    pub fn set_unported_behavior(&mut self, policy: UnportedBehavior) {
        self.region_mut().unported.policy = policy;
    }
    pub fn unported_behavior(&self) -> UnportedBehavior {
        self.region().unported.policy
    }
    pub fn unported_behavior_stats(&self) -> UnportedBehaviorStats {
        self.region().unported.stats
    }
    #[inline]
    pub(crate) fn optional_block_call<T>(
        &mut self,
        block: Block,
        method: BlockMethod,
        call: impl FnOnce(&'static dyn BlockBehavior, &mut Level) -> Result<T, LevelError>,
    ) -> Result<Option<T>, LevelError> {
        self.optional_call(block, method, |level| {
            call(level.block_behavior(block)?, level)
        })
    }
    #[inline]
    pub(crate) fn optional_call<T>(
        &mut self,
        block: Block,
        method: BlockMethod,
        call: impl FnOnce(&mut Level) -> Result<T, LevelError>,
    ) -> Result<Option<T>, LevelError> {
        let error = match call(self) {
            Ok(value) => return Ok(Some(value)),
            Err(error) => error,
        };
        let (block, method) = match error {
            LevelError::UnsupportedBlock(block) => (block, method),
            LevelError::UnsupportedBlockMethod(block, method) if method.can_skip() => {
                (block, method)
            }
            LevelError::UnsupportedFluidRandomTick => (block, method),
            _ => return Err(error),
        };
        let calls = &mut self.region_mut().unported;
        if calls.policy == UnportedBehavior::Strict {
            return Err(error);
        }
        calls.stats.skipped_calls = calls.stats.skipped_calls.saturating_add(1);
        if calls.seen.insert((block, method)) {
            calls.stats.distinct_calls = calls.seen.len();
            eprintln!(
                "Unported behavior skipped: {}#{} (skipped calls: {})",
                block.name(),
                method.name(),
                calls.stats.skipped_calls
            );
        }
        Ok(None)
    }
}
