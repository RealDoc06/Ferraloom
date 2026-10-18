//! Shared vanilla vegetation, foliage and ground behavior, on live and generation levels.
mod access;
pub mod bonemeal;
mod coral;
mod crops;
pub use bonemeal::{BonemealSource, BonemealType, BonemealableBlock, bonemealable_behavior};
mod ground;
mod growing;
mod leaves;
mod moss;
pub mod multiface;
pub mod multiface_spreader;
mod snow;
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
    if matches!(
        block,
        Block::KELP
            | Block::KELP_PLANT
            | Block::TWISTING_VINES
            | Block::TWISTING_VINES_PLANT
            | Block::WEEPING_VINES
            | Block::WEEPING_VINES_PLANT
            | Block::CAVE_VINES
            | Block::CAVE_VINES_PLANT
    ) && (matches!(
        method,
        M::CanSurvive
            | M::UpdateShape
            | M::UpdateFromNeighbourShapes
            | M::Tick
            | M::CanPlaceLiquid
            | M::PlaceLiquid
    ) || (method == M::RandomTick
        && block
            .default_state()
            .get(ferraloom_data::properties::AGE)
            .is_some()))
    {
        return Some(&growing::GROWING_PLANT);
    }
    if block.is_instance_of(C::SimpleWaterloggedBlock)
        && survival::supports(block, M::CanSurvive)
        && matches!(method, M::CanPlaceLiquid | M::PlaceLiquid | M::PickupBlock)
    {
        return Some(&super::fluid::SIMPLE_WATERLOGGED_BLOCK);
    }
    if matches!(block.class(), C::SeagrassBlock | C::TallSeagrassBlock)
        && matches!(method, M::CanPlaceLiquid | M::PlaceLiquid)
    {
        return Some(&super::BASE_BLOCK);
    }
    let behavior: &'static dyn BlockBehavior = match block.class() {
        C::CoralBlock | C::CoralPlantBlock | C::CoralFanBlock | C::CoralWallFanBlock
            if matches!(
                method,
                M::UpdateShape | M::UpdateFromNeighbourShapes | M::Tick | M::OnPlace
            ) =>
        {
            &coral::CORAL
        }
        C::CoralPlantBlock | C::CoralFanBlock | C::CoralWallFanBlock if method == M::CanSurvive => {
            &survival::PLANT
        }

        C::FarmlandBlock | C::PathBlock
            if matches!(
                method,
                M::CanSurvive | M::UpdateShape | M::UpdateFromNeighbourShapes
            ) =>
        {
            &ground::GROUND
        }
        C::SnowyBlock | C::SpreadingSnowyBlock | C::GrassBlock
            if matches!(method, M::UpdateShape | M::UpdateFromNeighbourShapes) =>
        {
            &ground::GROUND
        }
        C::SnowLayerBlock if matches!(method, M::RandomTick | M::IsFaceSturdy) => &snow::SNOW,

        C::CropBlock | C::BeetrootBlock | C::TorchflowerCropBlock if method == M::RandomTick => {
            &crops::CROP
        }
        C::NetherWartBlock if method == M::RandomTick => &crops::NETHER_WART,
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
mod worldgen_tests;
