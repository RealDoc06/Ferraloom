//! Shared vanilla vegetation, foliage and ground behavior, on live and generation levels.
mod access;
mod bamboo;
pub mod bonemeal;
mod chorus;
mod clone_item;
mod column;
mod coral;
mod crops;
mod dripleaf;
mod eyeblossom;
mod feature_bonemeal;
mod spread;
mod spreading;
pub use bonemeal::{BonemealSource, BonemealType, BonemealableBlock, bonemealable_behavior};
mod geometry;
mod grass_bonemeal;
mod ground;
mod growing;
mod hanging_moss;
mod harvest;
mod huge_mushroom;
mod ice;
mod leaves;
mod lifecycle;
mod moss;
pub mod multiface;
pub mod multiface_spreader;
mod nylium;
mod pathfinding;
mod pitcher;
mod placement;
#[cfg(test)]
mod placement_tests;
mod powder_snow;
mod sapling;
pub mod sculk;
mod sea_pickle;
mod snow;
mod survival;
pub mod tree_grower;
mod vines;
mod wall_plants;
pub use moss::place_at as place_mossy_carpet;
pub(crate) use powder_snow::before_extinguish as powder_snow_before_extinguish;
pub use powder_snow::can_walk_on as can_entity_walk_on_powder_snow;
pub use survival::place_double_plant;
pub use vines::acceptable_neighbor as vine_acceptable_neighbor;

use super::{BlockBehavior, BlockMethod};
use ferraloom_data::{Block, BlockClass};

/// Exact per-method family registration; incomplete virtuals stay Unsupported.
pub fn ported_behavior(block: Block, method: BlockMethod) -> Option<&'static dyn BlockBehavior> {
    use BlockClass as C;
    use BlockMethod as M;
    if matches!(block.class(), C::CocoaBlock | C::ShelfMushroomBlock)
        && matches!(method, M::CanSurvive | M::GetStateForPlacement)
    {
        return Some(&wall_plants::WALL_PLANTS);
    }
    if method == M::IsPathfindable {
        return pathfinding::behavior(block);
    }
    if matches!(
        block.class(),
        C::SculkSensorBlock | C::CalibratedSculkSensorBlock
    ) && matches!(
        method,
        M::OnPlace
            | M::AffectNeighborsAfterRemoval
            | M::Tick
            | M::StepOn
            | M::IsSignalSource
            | M::OwnSignal
            | M::Signal
            | M::DirectSignal
            | M::HasAnalogOutputSignal
            | M::AnalogOutputSignal
    ) {
        return Some(&sculk::SENSOR);
    }
    if matches!(
        block.class(),
        C::SculkSensorBlock | C::CalibratedSculkSensorBlock | C::SculkShriekerBlock
    ) && matches!(method, M::UpdateShape | M::UpdateFromNeighbourShapes)
    {
        return Some(&sculk::WATERLOGGED_SCULK);
    }
    if block.class() == C::HugeMushroomBlock
        && matches!(
            method,
            M::UpdateShape | M::UpdateFromNeighbourShapes | M::GetStateForPlacement
        )
    {
        return Some(&huge_mushroom::HUGE_MUSHROOM);
    }
    if block == Block::FROSTED_ICE
        && matches!(
            method,
            M::OnPlace | M::CloneItemStack | M::Tick | M::NeighborChanged
        )
    {
        return Some(&ice::FROSTED_ICE);
    }
    if matches!(block, Block::ICE | Block::FROSTED_ICE) && method == M::RandomTick {
        return Some(&ice::ICE);
    }
    if matches!(block, Block::GRASS_BLOCK | Block::MYCELIUM) && method == M::RandomTick {
        return Some(&spreading::SPREADING_SNOWY);
    }
    if block.is_instance_of(C::DoublePlantBlock)
        && matches!(method, M::SetPlacedBy | M::PlayerWillDestroy)
    {
        return Some(&lifecycle::PLANT_LIFECYCLE);
    }
    if block.class() == C::MossyCarpetBlock && method == M::SetPlacedBy {
        return Some(&moss::MOSSY_CARPET);
    }
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
    if block == Block::BAMBOO && method == M::IsCollisionShapeFullBlock {
        return Some(&geometry::PLANT_SHAPES);
    }
    let behavior: &'static dyn BlockBehavior = match block.class() {
        C::ChorusPlantBlock | C::ChorusFlowerBlock
            if matches!(
                method,
                M::CanSurvive | M::UpdateShape | M::UpdateFromNeighbourShapes | M::Tick
            ) || (block == Block::CHORUS_FLOWER
                && matches!(method, M::RandomTick | M::IsFaceSturdy))
                || (block == Block::CHORUS_PLANT && method == M::GetStateForPlacement) =>
        {
            &chorus::CHORUS
        }

        C::SweetBerryBushBlock if matches!(method, M::UseItemOn | M::UseWithoutItem) => {
            &harvest::HARVEST
        }
        C::CaveVinesBlock | C::CaveVinesPlantBlock if method == M::UseWithoutItem => {
            &harvest::HARVEST
        }
        C::BigDripleafBlock if matches!(method, M::Tick | M::NeighborChanged | M::EntityInside) => {
            &dripleaf::DRIPLEAF
        }
        C::EyeblossomBlock if matches!(method, M::Tick | M::RandomTick) => &eyeblossom::EYEBLOSSOM,
        C::HangingMossBlock
            if matches!(
                method,
                M::CanSurvive | M::UpdateShape | M::UpdateFromNeighbourShapes | M::Tick
            ) =>
        {
            &hanging_moss::HANGING_MOSS
        }

        C::BambooStalkBlock | C::BambooSaplingBlock if method == M::RandomTick => &bamboo::BAMBOO,

        C::SaplingBlock | C::MangrovePropaguleBlock if method == M::RandomTick => &sapling::SAPLING,
        C::MushroomBlock if method == M::RandomTick => &spread::MUSHROOM,
        C::StemBlock if method == M::RandomTick => &spread::STEM,
        C::CactusBlock | C::SugarCaneBlock if method == M::RandomTick => &column::COLUMN_PLANT,

        C::PowderSnowBlock
            if matches!(
                method,
                M::CollisionShape
                    | M::EntityInsideCollisionShape
                    | M::PickupBlock
                    | M::FallOn
                    | M::EntityInside
            ) =>
        {
            &powder_snow::POWDER_SNOW
        }

        C::CoralBlock if method == M::GetStateForPlacement => &coral::CORAL,
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
        C::FarmlandBlock if matches!(method, M::RandomTick | M::FallOn) => &ground::GROUND,
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

#[cfg(test)]
mod tree_worldgen_tests;

#[cfg(test)]
mod cave_worldgen_tests;

#[cfg(test)]
mod runtime_tests;

#[cfg(test)]
mod fixtures;
