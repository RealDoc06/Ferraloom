//! Vanilla redstone block family. Each virtual method is registered separately;
//! unimplemented overrides continue through the required-behavior error path.
use super::{BlockBehavior, BlockMethod};
use ferraloom_data::{Block, BlockClass};

pub mod comparator;
pub mod diode;
pub mod lamp;
pub mod moving_piston;
pub mod observer;
pub mod piston;
pub mod signal;
mod sources;
mod state;
pub mod storage;
pub mod torch;
pub mod wire;

/// Region-local counterpart of mutable redstone block state in the Java
/// registry. Dispatchers remain stateless across reentrant level callbacks.
#[derive(Default)]
pub struct RedstoneState {
    pub(crate) wire_signals_suppressed: bool,
    pub(crate) piston_noclip: Option<ferraloom_util::Direction>,
    pub(crate) torch_toggles: std::collections::VecDeque<(ferraloom_util::BlockPos, i64)>,
}

pub(crate) fn ported_behavior(
    block: Block,
    method: BlockMethod,
) -> Option<&'static dyn BlockBehavior> {
    use BlockMethod as M;
    match block.class() {
        BlockClass::PoweredBlock if matches!(method, M::IsSignalSource | M::OwnSignal) => {
            Some(&signal::PoweredBlock)
        }
        BlockClass::RedstoneWireBlock
            if matches!(
                method,
                M::IsSignalSource
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
        BlockClass::RepeaterBlock
            if matches!(
                method,
                M::IsSignalSource
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
        BlockClass::ComparatorBlock
            if matches!(
                method,
                M::IsSignalSource
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
        BlockClass::MovingPistonBlock
            if matches!(
                method,
                M::OutlineShape | M::CollisionShape | M::CloneItemStack | M::UseWithoutItem
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
