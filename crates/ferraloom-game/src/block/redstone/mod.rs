//! Vanilla redstone block family. Each virtual method is registered separately;
//! unimplemented overrides continue through the required-behavior error path.
use super::{BlockBehavior, BlockMethod};
use ferraloom_data::{Block, BlockClass};

pub mod comparator;
pub mod crafter;
pub mod device;
pub mod diode;
pub mod dispenser;
pub mod hopper;
pub mod lamp;
pub mod moving_piston;
pub mod note;
pub mod observer;
pub mod opening;
mod ore;
pub mod piston;
mod placement;
pub mod rail;
pub mod signal;
mod sources;
mod state;
pub mod storage;
pub mod switches;
pub mod target;
pub mod torch;
pub mod tripwire;
pub mod wire;

/// Region-local counterpart of mutable redstone block state in the Java
/// registry. Dispatchers remain stateless across reentrant level callbacks.
#[derive(Default)]
pub struct RedstoneState {
    pub(crate) wire_signals_suppressed: bool,
    pub(crate) dispense: dispenser::DispenseRegistry,
    pub(crate) piston_noclip: Option<ferraloom_util::Direction>,
    pub(crate) torch_toggles: std::collections::VecDeque<(ferraloom_util::BlockPos, i64)>,
}

fn stack_error(error: crate::inventory::MenuError) -> crate::level::LevelError {
    match error {
        crate::inventory::MenuError::Level(error) => error,
        _ => crate::level::LevelError::Inventory("redstone stack receiver"),
    }
}

pub(crate) fn ported_behavior(
    block: Block,
    method: BlockMethod,
) -> Option<&'static dyn BlockBehavior> {
    use BlockMethod as M;
    if method == M::SetPlacedBy {
        return match block.class() {
            BlockClass::RepeaterBlock => Some(&diode::REPEATER),
            BlockClass::ComparatorBlock => Some(&comparator::COMPARATOR),
            BlockClass::PistonBaseBlock => Some(&piston::PISTON),
            BlockClass::TripWireHookBlock => Some(&tripwire::HOOK),
            _ if block.is_instance_of(BlockClass::DoorBlock) => Some(&opening::OPENING),
            _ => None,
        };
    }
    if method == M::PlayerWillDestroy && block.class() == BlockClass::TripWireBlock {
        return Some(&tripwire::WIRE);
    }
    if method == M::GetStateForPlacement && placement::supports(block) {
        return Some(&placement::PLACEMENT);
    }
    if block.class() == BlockClass::DetectorRailBlock {
        if matches!(
            method,
            M::OnPlace | M::Tick | M::EntityInside | M::AnalogOutputSignal
        ) {
            return Some(&rail::DETECTOR);
        }
        if matches!(
            method,
            M::IsSignalSource | M::OwnSignal | M::Signal | M::DirectSignal
        ) {
            return Some(&sources::SOURCES);
        }
        if matches!(
            method,
            M::CanSurvive
                | M::UpdateShape
                | M::UpdateFromNeighbourShapes
                | M::NeighborChanged
                | M::AffectNeighborsAfterRemoval
        ) {
            return Some(&rail::RAIL);
        }
    }
    if block.class() == BlockClass::CrafterBlock && matches!(method, M::NeighborChanged | M::Tick) {
        return Some(&crafter::CRAFTER);
    }
    if device::supports(block, method) {
        return Some(&device::DEVICES);
    }
    if method == M::PlayerWillDestroy {
        if block.is_instance_of(BlockClass::DoorBlock) {
            return Some(&opening::OPENING);
        }
        if block.class() == BlockClass::PistonHeadBlock {
            return Some(&piston::PISTON_HEAD);
        }
    }
    if block.class() == BlockClass::RedStoneOreBlock
        && matches!(method, M::RandomTick | M::StepOn | M::UseItemOn)
    {
        return Some(&ore::ORE);
    }
    match block.class() {
        BlockClass::PoweredBlock if matches!(method, M::IsSignalSource | M::OwnSignal) => {
            Some(&signal::PoweredBlock)
        }
        BlockClass::RedstoneWireBlock
            if matches!(
                method,
                M::UseWithoutItem
                    | M::IsSignalSource
                    | M::OwnSignal
                    | M::Signal
                    | M::DirectSignal
                    | M::ShouldRedstoneWireConnectTo
                    | M::CanSurvive
                    | M::UpdateShape
                    | M::UpdateFromNeighbourShapes
                    | M::UpdateIndirectNeighborShapes
                    | M::OnPlace
                    | M::AffectNeighborsAfterRemoval
                    | M::NeighborChanged
            ) =>
        {
            Some(&wire::WIRE)
        }
        BlockClass::ObserverBlock
            if matches!(
                method,
                M::IsSignalSource
                    | M::OwnSignal
                    | M::Signal
                    | M::DirectSignal
                    | M::ShouldRedstoneWireConnectTo
                    | M::Tick
                    | M::UpdateShape
                    | M::UpdateFromNeighbourShapes
                    | M::OnPlace
                    | M::AffectNeighborsAfterRemoval
            ) =>
        {
            Some(&observer::OBSERVER)
        }
        BlockClass::RedstoneTorchBlock | BlockClass::RedstoneWallTorchBlock
            if matches!(
                method,
                M::IsSignalSource
                    | M::OwnSignal
                    | M::Signal
                    | M::DirectSignal
                    | M::Tick
                    | M::CanSurvive
                    | M::UpdateShape
                    | M::UpdateFromNeighbourShapes
                    | M::OnPlace
                    | M::AffectNeighborsAfterRemoval
                    | M::NeighborChanged
            ) =>
        {
            Some(&torch::TORCH)
        }
        BlockClass::RedstoneLampBlock if matches!(method, M::NeighborChanged | M::Tick) => {
            Some(&lamp::LAMP)
        }
        BlockClass::TargetBlock if matches!(method, M::OnPlace | M::Tick) => Some(&target::TARGET),
        BlockClass::TripWireBlock
            if matches!(
                method,
                M::UpdateShape
                    | M::UpdateFromNeighbourShapes
                    | M::OnPlace
                    | M::AffectNeighborsAfterRemoval
                    | M::EntityInside
                    | M::EntityInsideCollisionShape
                    | M::Tick
            ) =>
        {
            Some(&tripwire::WIRE)
        }
        BlockClass::TripWireHookBlock
            if matches!(
                method,
                M::CanSurvive
                    | M::UpdateShape
                    | M::UpdateFromNeighbourShapes
                    | M::AffectNeighborsAfterRemoval
                    | M::Tick
            ) =>
        {
            Some(&tripwire::HOOK)
        }
        BlockClass::NoteBlock
            if matches!(
                method,
                M::UseItemOn
                    | M::UseWithoutItem
                    | M::UpdateShape
                    | M::UpdateFromNeighbourShapes
                    | M::NeighborChanged
                    | M::TriggerEvent
            ) =>
        {
            Some(&note::NOTE)
        }
        BlockClass::RailBlock | BlockClass::PoweredRailBlock
            if matches!(
                method,
                M::OutlineShape
                    | M::CanSurvive
                    | M::UpdateShape
                    | M::UpdateFromNeighbourShapes
                    | M::OnPlace
                    | M::NeighborChanged
                    | M::AffectNeighborsAfterRemoval
            ) =>
        {
            Some(&rail::RAIL)
        }
        BlockClass::RepeaterBlock
            if matches!(
                method,
                M::UseWithoutItem
                    | M::IsSignalSource
                    | M::OwnSignal
                    | M::Signal
                    | M::DirectSignal
                    | M::ShouldRedstoneWireConnectTo
                    | M::CanSurvive
                    | M::OnPlace
                    | M::AffectNeighborsAfterRemoval
                    | M::UpdateShape
                    | M::UpdateFromNeighbourShapes
                    | M::NeighborChanged
                    | M::Tick
            ) =>
        {
            Some(&diode::REPEATER)
        }
        BlockClass::LeverBlock
        | BlockClass::ButtonBlock
        | BlockClass::PressurePlateBlock
        | BlockClass::WeightedPressurePlateBlock
        | BlockClass::LightningRodBlock
        | BlockClass::WeatheringLightningRodBlock
        | BlockClass::TripWireHookBlock
        | BlockClass::LecternBlock
        | BlockClass::DaylightDetectorBlock
        | BlockClass::SculkSensorBlock
        | BlockClass::CalibratedSculkSensorBlock
        | BlockClass::TargetBlock
            if matches!(
                method,
                M::IsSignalSource | M::OwnSignal | M::DirectSignal | M::Signal
            ) =>
        {
            Some(&sources::SOURCES)
        }
        BlockClass::DispenserBlock | BlockClass::DropperBlock
            if matches!(
                method,
                M::NeighborChanged
                    | M::Tick
                    | M::AnalogOutputSignal
                    | M::HasAnalogOutputSignal
                    | M::AffectNeighborsAfterRemoval
            ) =>
        {
            Some(&dispenser::DISPENSER)
        }
        BlockClass::HopperBlock
            if matches!(
                method,
                M::OnPlace
                    | M::NeighborChanged
                    | M::AnalogOutputSignal
                    | M::HasAnalogOutputSignal
                    | M::AffectNeighborsAfterRemoval
                    | M::EntityInside
            ) =>
        {
            Some(&hopper::HOPPER)
        }
        BlockClass::ComparatorBlock
            if matches!(
                method,
                M::UseWithoutItem
                    | M::IsSignalSource
                    | M::OwnSignal
                    | M::Signal
                    | M::DirectSignal
                    | M::CanSurvive
                    | M::OnPlace
                    | M::AffectNeighborsAfterRemoval
                    | M::UpdateShape
                    | M::UpdateFromNeighbourShapes
                    | M::NeighborChanged
                    | M::Tick
                    | M::TriggerEvent
            ) =>
        {
            Some(&comparator::COMPARATOR)
        }
        BlockClass::LeverBlock | BlockClass::ButtonBlock
            if matches!(
                method,
                M::UseWithoutItem
                    | M::CanSurvive
                    | M::UpdateShape
                    | M::UpdateFromNeighbourShapes
                    | M::AffectNeighborsAfterRemoval
                    | M::Tick
                    | M::EntityInside
            ) =>
        {
            Some(&switches::SWITCHES)
        }
        BlockClass::PressurePlateBlock | BlockClass::WeightedPressurePlateBlock
            if matches!(
                method,
                M::CanSurvive
                    | M::UpdateShape
                    | M::UpdateFromNeighbourShapes
                    | M::AffectNeighborsAfterRemoval
                    | M::Tick
                    | M::EntityInside
            ) =>
        {
            Some(&switches::SWITCHES)
        }
        _ if (block.is_instance_of(BlockClass::DoorBlock)
            || block.is_instance_of(BlockClass::TrapDoorBlock)
            || block.is_instance_of(BlockClass::FenceGateBlock))
            && matches!(
                method,
                M::UseWithoutItem
                    | M::CanSurvive
                    | M::UpdateShape
                    | M::UpdateFromNeighbourShapes
                    | M::NeighborChanged
                    | M::IsFaceSturdy
            ) =>
        {
            Some(&opening::OPENING)
        }
        _ if block.is_instance_of(BlockClass::CopperBulbBlock)
            && matches!(
                method,
                M::OnPlace | M::NeighborChanged | M::HasAnalogOutputSignal | M::AnalogOutputSignal
            ) =>
        {
            Some(&opening::BULB)
        }
        BlockClass::MovingPistonBlock
            if matches!(
                method,
                M::GetDrops
                    | M::OutlineShape
                    | M::CollisionShape
                    | M::CloneItemStack
                    | M::UseWithoutItem
            ) =>
        {
            Some(&moving_piston::MOVING_PISTON)
        }
        BlockClass::PistonBaseBlock
            if matches!(method, M::OnPlace | M::NeighborChanged | M::TriggerEvent) =>
        {
            Some(&piston::PISTON)
        }
        BlockClass::PistonHeadBlock
            if matches!(
                method,
                M::CanSurvive
                    | M::UpdateShape
                    | M::UpdateFromNeighbourShapes
                    | M::NeighborChanged
                    | M::AffectNeighborsAfterRemoval
                    | M::CloneItemStack
            ) =>
        {
            Some(&piston::PISTON_HEAD)
        }
        _ => None,
    }
}

/// Port of ExperimentalRedstoneUtils#initialOrientation. The up transform runs
/// before the front transform; there is no RNG draw with experiments disabled.
pub(super) fn initial_orientation(
    level: &mut crate::level::Level,
    front: Option<ferraloom_util::Direction>,
    up: Option<ferraloom_util::Direction>,
) -> Option<crate::level::redstone::Orientation> {
    use crate::level::redstone::{Orientation, SideBias};
    use ferraloom_util::random::RandomSource;
    if !level.rules().redstone_experiments {
        return None;
    }
    let mut orientation = Orientation::from_index(level.random().next_int_bound(48) as u8)
        .unwrap()
        .with_side_bias(SideBias::Left);
    if let Some(up) = up {
        orientation = orientation.with_up(up);
    }
    Some(front.map_or(orientation, |front| orientation.with_front(front)))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod identity_tests;

#[cfg(test)]
mod device_tests;

#[cfg(test)]
mod registry_tests;
