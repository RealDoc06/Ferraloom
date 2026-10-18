//! Temporary runtime policy while block families are ported. Tests and parity
//! remain strict; Warn records a skipped call, never a fabricated return value.
use super::BlockBehavior;
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, EntityType};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
};
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UnportedBehavior {
    #[default]
    Strict,
    Warn,
}
/// Approved whole-effect skips, distinct from queries that require a value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UnportedOperation {
    EntityMethod(EntityType, crate::entity::EntityMethod),
    ItemUseOn(ferraloom_data::Item),
    ItemCraftedPostProcess(ferraloom_data::Item),
    PortalUse(Block),
    AdvancementCriterion(crate::advancements::criteria::Trigger),
    WeatherStrike,
    PackReload,
    SnowAccumulation,
    PushEntitiesUp,
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BlockMethod {
    GetMenuProvider,
    GetDrops,
    SpawnAfterBreak,
    OnPlace,
    UseWithoutItem,
    UseItemOn,
    HandlePrecipitation,
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
    IsPathfindable,
    CanPlaceLiquid,
    PlaceLiquid,
    PickupBlock,
    CollisionShape,
    OutlineShape,
    IsFaceSturdy,
    EntityInsideCollisionShape,
    UpdateFromNeighbourShapes,
    TriggerEvent,
    IsCollisionShapeFullBlock,
    CloneItemStack,
    CanBeReplaced,
    GetStateForPlacement,
    Signal,
    DirectSignal,
    IsSignalSource,
    OwnSignal,
    ShouldRedstoneWireConnectTo,
    HasAnalogOutputSignal,
    AnalogOutputSignal,
    SetPlacedBy,
    PlayerWillDestroy,
    SpawnDestroyByEntityParticles,
    SpawnDestroyParticles,
}
impl BlockMethod {
    pub(crate) const fn vanilla_method(self) -> Option<ferraloom_data::BlockMethod> {
        use ferraloom_data::BlockMethod as Vanilla;
        Some(match self {
            Self::GetMenuProvider => Vanilla::GetMenuProvider,
            Self::GetDrops => Vanilla::GetDrops,
            Self::SpawnAfterBreak => Vanilla::SpawnAfterBreak,
            Self::OnPlace => Vanilla::OnPlace,
            Self::UseWithoutItem => Vanilla::UseWithoutItem,
            Self::UseItemOn => Vanilla::UseItemOn,
            Self::HandlePrecipitation => Vanilla::HandlePrecipitation,
            Self::AffectNeighborsAfterRemoval => Vanilla::AffectNeighborsAfterRemoval,
            Self::NeighborChanged => Vanilla::NeighborChanged,
            Self::EntityInside => Vanilla::EntityInside,
            Self::FallOn => Vanilla::FallOn,
            Self::StepOn => Vanilla::StepOn,
            Self::BounceOn => Vanilla::BounceOn,
            Self::UpdateShape | Self::UpdateFromNeighbourShapes => Vanilla::UpdateShape,
            Self::UpdateIndirectNeighborShapes => Vanilla::UpdateIndirectNeighbourShapes,
            Self::Tick => Vanilla::Tick,
            Self::RandomTick => Vanilla::RandomTick,
            Self::CanSurvive => Vanilla::CanSurvive,
            Self::IsPathfindable => Vanilla::IsPathfindable,
            Self::CollisionShape => Vanilla::GetCollisionShape,
            Self::OutlineShape => Vanilla::GetShape,
            Self::IsFaceSturdy => Vanilla::GetBlockSupportShape,
            Self::IsCollisionShapeFullBlock => Vanilla::IsCollisionShapeFullBlock,
            Self::EntityInsideCollisionShape => Vanilla::GetEntityInsideCollisionShape,
            Self::TriggerEvent => Vanilla::TriggerEvent,
            Self::CloneItemStack => Vanilla::GetCloneItemStack,
            Self::CanBeReplaced => Vanilla::CanBeReplaced,
            Self::GetStateForPlacement => Vanilla::GetStateForPlacement,
            Self::Signal => Vanilla::GetSignal,
            Self::DirectSignal => Vanilla::GetDirectSignal,
            Self::IsSignalSource => Vanilla::IsSignalSource,
            Self::OwnSignal => Vanilla::OwnSignal,
            Self::ShouldRedstoneWireConnectTo => Vanilla::ShouldRedstoneWireConnectTo,
            Self::HasAnalogOutputSignal => Vanilla::HasAnalogOutputSignal,
            Self::AnalogOutputSignal => Vanilla::GetAnalogOutputSignal,
            Self::SetPlacedBy => Vanilla::SetPlacedBy,
            Self::PlayerWillDestroy => Vanilla::PlayerWillDestroy,
            Self::SpawnDestroyByEntityParticles => Vanilla::SpawnDestroyByEntityParticles,
            Self::SpawnDestroyParticles => Vanilla::SpawnDestroyParticles,
            Self::CanPlaceLiquid => Vanilla::CanPlaceLiquid,
            Self::PlaceLiquid => Vanilla::PlaceLiquid,
            Self::PickupBlock => Vanilla::PickupBlock,
            Self::FluidTick | Self::FluidRandomTick | Self::BubbleColumnUpdate => return None,
        })
    }
    const fn can_skip(self) -> bool {
        !matches!(
            self,
            Self::GetMenuProvider
                | Self::GetDrops
                | Self::UseWithoutItem
                | Self::UseItemOn
                | Self::PlayerWillDestroy
                | Self::CanSurvive
                | Self::IsPathfindable
                | Self::CanPlaceLiquid
                | Self::PlaceLiquid
                | Self::PickupBlock
                | Self::CollisionShape
                | Self::OutlineShape
                | Self::IsFaceSturdy
                | Self::EntityInsideCollisionShape
                | Self::UpdateFromNeighbourShapes
                | Self::TriggerEvent
                | Self::IsCollisionShapeFullBlock
                | Self::CloneItemStack
                | Self::GetStateForPlacement
                | Self::CanBeReplaced
                | Self::Signal
                | Self::DirectSignal
                | Self::IsSignalSource
                | Self::OwnSignal
                | Self::ShouldRedstoneWireConnectTo
                | Self::HasAnalogOutputSignal
                | Self::AnalogOutputSignal
        )
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::GetMenuProvider => "getMenuProvider",
            Self::GetDrops => "getDrops",
            Self::SpawnAfterBreak => "spawnAfterBreak",
            Self::OnPlace => "onPlace",
            Self::UseWithoutItem => "useWithoutItem",
            Self::UseItemOn => "useItemOn",
            Self::HandlePrecipitation => "handlePrecipitation",
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
            Self::IsPathfindable => "isPathfindable",
            Self::CanPlaceLiquid => "canPlaceLiquid",
            Self::PlaceLiquid => "placeLiquid",
            Self::PickupBlock => "pickupBlock",
            Self::CollisionShape => "getCollisionShape",
            Self::OutlineShape => "getShape",
            Self::IsFaceSturdy => "isFaceSturdy",
            Self::EntityInsideCollisionShape => "getEntityInsideCollisionShape",
            Self::UpdateFromNeighbourShapes => "Block.updateFromNeighbourShapes",
            Self::TriggerEvent => "triggerEvent",
            Self::IsCollisionShapeFullBlock => "isCollisionShapeFullBlock",
            Self::CloneItemStack => "getCloneItemStack",
            Self::CanBeReplaced => "canBeReplaced(BlockPlaceContext)",
            Self::GetStateForPlacement => "getStateForPlacement",
            Self::Signal => "getSignal",
            Self::DirectSignal => "getDirectSignal",
            Self::IsSignalSource => "isSignalSource",
            Self::OwnSignal => "ownSignal",
            Self::ShouldRedstoneWireConnectTo => "shouldRedstoneWireConnectTo",
            Self::HasAnalogOutputSignal => "hasAnalogOutputSignal",
            Self::AnalogOutputSignal => "getAnalogOutputSignal",
            Self::SetPlacedBy => "setPlacedBy",
            Self::PlayerWillDestroy => "playerWillDestroy",
            Self::SpawnDestroyByEntityParticles => "spawnDestroyByEntityParticles",
            Self::SpawnDestroyParticles => "spawnDestroyParticles",
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
    seen: RefCell<HashSet<UnportedCall>>,
    stats: Cell<UnportedBehaviorStats>,
}
/// Named public form of the canonical worldgen unit/behavior deduplication key.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct WorldgenSkip {
    pub unit: String,
    pub behavior: String,
}
impl From<(String, String)> for WorldgenSkip {
    fn from((unit, behavior): (String, String)) -> Self {
        Self { unit, behavior }
    }
}
#[derive(Clone, Eq, Hash, PartialEq)]
enum UnportedCall {
    Block(Block, BlockMethod),
    Operation(UnportedOperation),
    Worldgen(WorldgenSkip),
    BlockEntity(Block, ferraloom_nbt::NbtString),
    PostProcessing(Block, &'static str),
}
impl UnportedCalls {
    fn record(&self, call: UnportedCall) {
        let mut stats = self.stats.get();
        stats.skipped_calls = stats.skipped_calls.saturating_add(1);
        let fresh = !self.seen.borrow().contains(&call);
        if fresh {
            let mut seen = self.seen.borrow_mut();
            seen.insert(call.clone());
            stats.distinct_calls = seen.len();
        }
        self.stats.set(stats);
        // No private metadata guard reaches logging or any world callback.
        if fresh {
            match &call {
                UnportedCall::Operation(UnportedOperation::EntityMethod(kind, method)) => {
                    eprintln!(
                        "Unported entity effect skipped: {}#{} (skipped calls: {})",
                        kind.name(),
                        method.name(),
                        stats.skipped_calls
                    )
                }
                UnportedCall::Operation(UnportedOperation::ItemUseOn(item)) => eprintln!(
                    "Unported item use effect skipped: {}#useOn (skipped calls: {})",
                    item.name(),
                    stats.skipped_calls
                ),
                UnportedCall::Operation(UnportedOperation::ItemCraftedPostProcess(item)) => {
                    eprintln!(
                        "Unported crafted item effect skipped: {}#onCraftedPostProcess (skipped calls: {})",
                        item.name(),
                        stats.skipped_calls
                    )
                }
                UnportedCall::Operation(UnportedOperation::PortalUse(block)) => eprintln!(
                    "Unported portal use effect skipped: {} (skipped calls: {})",
                    block.name(),
                    stats.skipped_calls
                ),
                UnportedCall::Operation(UnportedOperation::AdvancementCriterion(trigger)) => {
                    eprintln!(
                        "Unported advancement criterion skipped: {} (skipped calls: {})",
                        trigger.name(),
                        stats.skipped_calls
                    )
                }
                UnportedCall::Block(block, method) => eprintln!(
                    "Unported behavior skipped: {}#{} (skipped calls: {})",
                    block.name(),
                    method.name(),
                    stats.skipped_calls
                ),
                UnportedCall::Operation(UnportedOperation::WeatherStrike) => eprintln!(
                    "Unported behavior skipped: ServerLevel#tickThunder strike (skipped calls: {}; RNG parity pending)",
                    stats.skipped_calls
                ),
                UnportedCall::Operation(UnportedOperation::SnowAccumulation) => eprintln!(
                    "Unported behavior skipped: ServerLevel#tickPrecipitation snow accumulation (skipped calls: {})",
                    stats.skipped_calls
                ),
                UnportedCall::Operation(UnportedOperation::PushEntitiesUp) => eprintln!(
                    "Unported behavior skipped: Block#pushEntitiesUp (skipped calls: {})",
                    stats.skipped_calls
                ),
                UnportedCall::Operation(UnportedOperation::PackReload) => eprintln!(
                    "Pack reload not yet supported (skipped calls: {})",
                    stats.skipped_calls
                ),
                UnportedCall::Worldgen(WorldgenSkip { unit, behavior }) => eprintln!(
                    "Unported worldgen unit skipped: {unit}#{behavior} (skipped calls: {})",
                    stats.skipped_calls
                ),
                UnportedCall::BlockEntity(block, factory) => eprintln!(
                    "Pending block entity activation skipped: {}#{factory} (skipped calls: {}; raw NBT retained)",
                    block.name(),
                    stats.skipped_calls
                ),
                UnportedCall::PostProcessing(block, behavior) => eprintln!(
                    "Post-processing unit skipped: {}#{behavior} (skipped calls: {})",
                    block.name(),
                    stats.skipped_calls
                ),
            }
        }
    }
}
impl Level {
    pub(crate) fn skip_post_processing(&mut self, error: LevelError) -> Result<(), LevelError> {
        let (block, behavior) = match error {
            LevelError::UnsupportedBlockMethod(block, method) => (block, method.name()),
            LevelError::UnsupportedShape(state) => (state.block(), "getCollisionShape"),
            LevelError::UnsupportedBlock(block) => (block, "Block.updateFromNeighbourShapes"),
            LevelError::UnsupportedBlockEntity(block) => (block, "setBlockState block entity"),
            _ => return Err(error),
        };
        let calls = &mut self.region_mut().unported;
        if calls.policy == UnportedBehavior::Strict {
            return Err(error);
        }
        calls.record(UnportedCall::PostProcessing(block, behavior));
        Ok(())
    }
    /// Integrate owned generation-unit skips on the game thread. Strict rejects
    /// any skipped unit; Warn shares the live counters and logs each pair once.
    pub fn record_worldgen_skips(&self, skips: Vec<WorldgenSkip>) -> Result<(), LevelError> {
        let calls = &self.region().unported;
        if !skips.is_empty() && calls.policy == UnportedBehavior::Strict {
            return Err(LevelError::UnsupportedWorldgenBehavior);
        }
        for skip in skips {
            calls.record(UnportedCall::Worldgen(skip));
        }
        Ok(())
    }
    pub(crate) fn skip_block_entity_activation(
        &mut self,
        block: Block,
        factory: ferraloom_nbt::NbtString,
    ) -> Result<(), LevelError> {
        let calls = &mut self.region_mut().unported;
        if calls.policy == UnportedBehavior::Strict {
            return Err(LevelError::UnsupportedBlockEntity(block));
        }
        calls.record(UnportedCall::BlockEntity(block, factory));
        Ok(())
    }
    /// Record one approved unit-effect skip. This shares the block-method
    /// policy, deduplication and visible counters; it never supplies query values.
    pub fn unported_operation(&mut self, operation: UnportedOperation) -> Result<(), LevelError> {
        let calls = &mut self.region_mut().unported;
        if calls.policy == UnportedBehavior::Strict {
            return Err(match operation {
                UnportedOperation::EntityMethod(kind, method) => {
                    LevelError::UnsupportedEntityMethod(kind, method)
                }
                UnportedOperation::ItemUseOn(item) => LevelError::UnsupportedItemUseOn(item),
                UnportedOperation::ItemCraftedPostProcess(item) => {
                    LevelError::UnsupportedItemCraftedPostProcess(item)
                }
                UnportedOperation::PortalUse(block) => LevelError::UnsupportedPortalUse(block),
                UnportedOperation::AdvancementCriterion(trigger) => {
                    LevelError::UnsupportedAdvancementCriterion(trigger)
                }
                UnportedOperation::WeatherStrike => LevelError::UnsupportedWeatherQuery,
                UnportedOperation::PackReload => LevelError::UnsupportedPackReload,
                UnportedOperation::SnowAccumulation => {
                    LevelError::UnsupportedBlockMethod(Block::SNOW, BlockMethod::CanSurvive)
                }
                UnportedOperation::PushEntitiesUp => LevelError::UnsupportedPushEntitiesUp,
            });
        }
        calls.record(UnportedCall::Operation(operation));
        Ok(())
    }
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
        self.region().unported.stats.get()
    }
    #[inline]
    pub(crate) fn optional_block_call<T>(
        &mut self,
        block: Block,
        method: BlockMethod,
        call: impl FnOnce(&'static dyn BlockBehavior, &mut Level) -> Result<T, LevelError>,
    ) -> Result<Option<T>, LevelError> {
        self.optional_call(block, method, |level| {
            call(level.behavior_for_method(block, method)?, level)
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
        calls.record(UnportedCall::Block(block, method));
        Ok(None)
    }
}
