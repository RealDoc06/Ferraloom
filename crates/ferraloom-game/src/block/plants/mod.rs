//! Shared vanilla vegetation, foliage and ground behavior, on live and generation levels.
mod access;
mod leaves;
mod moss;
pub mod multiface;
pub mod multiface_spreader;
mod survival;
mod vines;
pub use moss::place_at as place_mossy_carpet;
pub use survival::place_double_plant;
pub use vines::acceptable_neighbor as vine_acceptable_neighbor;

use super::{BlockBehavior, BlockMethod};
use ferraloom_data::{Block, BlockClass};

/// Exact per-method family registration; incomplete virtuals stay Unsupported.
pub fn ported_behavior(block: Block, method: BlockMethod) -> Option<&'static dyn BlockBehavior> {
    use BlockClass as C;
    use BlockMethod as M;
    let behavior: &'static dyn BlockBehavior = match block.class() {
        C::LeavesBlock
        | C::TintedParticleLeavesBlock
        | C::UntintedParticleLeavesBlock
        | C::MangroveLeavesBlock
            if matches!(
                method,
                M::UpdateShape
                    | M::UpdateFromNeighbourShapes
                    | M::Tick
                    | M::RandomTick
                    | M::IsFaceSturdy
                    | M::CanPlaceLiquid
                    | M::PlaceLiquid
                    | M::PickupBlock
            ) =>
        {
            &leaves::LEAVES
        }
        C::VineBlock
            if matches!(
                method,
                M::CanSurvive | M::UpdateShape | M::UpdateFromNeighbourShapes
            ) =>
        {
            &vines::VINES
        }
        C::MultifaceBlock
        | C::GlowLichenBlock
        | C::SculkVeinBlock
        | C::MultifaceSpreadeableBlock
            if matches!(
                method,
                M::CanSurvive
                    | M::UpdateShape
                    | M::UpdateFromNeighbourShapes
                    | M::CanPlaceLiquid
                    | M::PlaceLiquid
                    | M::PickupBlock
            ) =>
        {
            &multiface::MULTIFACE
        }
        C::MossyCarpetBlock
            if matches!(
                method,
                M::CanSurvive | M::UpdateShape | M::UpdateFromNeighbourShapes
            ) =>
        {
            &moss::MOSSY_CARPET
        }
        _ if survival::supports(block, method) => &survival::PLANT,
        _ => return None,
    };
    Some(behavior)
}

pub(super) fn source_water(fluid: ferraloom_data::FluidState) -> bool {
    fluid.source && fluid.fluid_type == ferraloom_data::registries::fluid::Id::WATER
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod fixtures;
