//! Shared vanilla vegetation, foliage and ground behavior, on live and generation levels.
mod access;
mod bamboo;
pub mod bonemeal;
mod clone_item;
mod column;
mod coral;
mod crops;
mod dripleaf;
mod spread;
pub use bonemeal::{BonemealSource, BonemealType, BonemealableBlock, bonemealable_behavior};
mod geometry;
mod ground;
mod growing;
mod hanging_moss;
mod leaves;
mod moss;
pub mod multiface;
pub mod multiface_spreader;
mod nylium;
mod pitcher;
mod placement;
mod powder_snow;
mod sea_pickle;
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
    if placement::supports(block, method) {
        return Some(&placement::PLANT_PLACEMENT);
    }
    if method == M::CloneItemStack && clone_item::item(block).is_some() {
        return Some(&clone_item::PLANT_CLONE);
    }
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
    if matches!(method, M::OutlineShape | M::CollisionShape)
        && (geometry::supports(block.class()) || block == Block::MOSS_CARPET)
    {
        return Some(&geometry::PLANT_SHAPES);
    }
    let behavior: &'static dyn BlockBehavior = match block.class() {
        C::HangingMossBlock
            if matches!(
                method,
                M::CanSurvive | M::UpdateShape | M::UpdateFromNeighbourShapes | M::Tick
            ) =>
        {
            &hanging_moss::HANGING_MOSS
        }

        C::BambooStalkBlock | C::BambooSaplingBlock if method == M::RandomTick => &bamboo::BAMBOO,

        C::MushroomBlock if method == M::RandomTick => &spread::MUSHROOM,
        C::StemBlock if method == M::RandomTick => &spread::STEM,
        C::CactusBlock | C::SugarCaneBlock if method == M::RandomTick => &column::COLUMN_PLANT,

        C::PowderSnowBlock
            if matches!(
                method,
                M::CollisionShape | M::EntityInsideCollisionShape | M::PickupBlock
            ) =>
        {
            &powder_snow::POWDER_SNOW
        }

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
                M::CanSurvive | M::UpdateShape | M::UpdateFromNeighbourShapes | M::Tick
            ) =>
        {
            &ground::GROUND
        }
        C::FarmlandBlock if method == M::RandomTick => &ground::GROUND,
        C::SnowyBlock | C::SpreadingSnowyBlock | C::GrassBlock | C::MyceliumBlock
            if matches!(method, M::UpdateShape | M::UpdateFromNeighbourShapes) =>
        {
            &ground::GROUND
        }
        C::SnowLayerBlock if matches!(method, M::RandomTick | M::IsFaceSturdy) => &snow::SNOW,

        C::CropBlock
        | C::CarrotBlock
        | C::PotatoBlock
        | C::BeetrootBlock
        | C::TorchflowerCropBlock
            if method == M::RandomTick =>
        {
            &crops::CROP
        }
        C::NetherWartBlock if method == M::RandomTick => &crops::NETHER_WART,
        C::NyliumBlock if method == M::RandomTick => &nylium::NYLIUM,
        C::SweetBerryBushBlock if method == M::RandomTick => &crops::SWEET_BERRY,
        C::PitcherCropBlock if method == M::RandomTick => &pitcher::PITCHER,
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
                M::CanSurvive
                    | M::UpdateShape
                    | M::UpdateFromNeighbourShapes
                    | M::RandomTick
                    | M::CanBeReplaced
                    | M::GetStateForPlacement
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
