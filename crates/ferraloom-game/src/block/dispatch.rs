/// Immutable virtual dispatch tables. Unsupported behavior is an explicit error.
pub struct Behaviors {
    entities: [Option<&'static dyn crate::entity::Entity>; ferraloom_data::ENTITY_TYPES.len()],
    blocks: [Option<&'static dyn super::BlockBehavior>; ferraloom_data::BLOCKS.len()],
}

impl Behaviors {
    pub const EMPTY: Self = Self {
        entities: [None; ferraloom_data::ENTITY_TYPES.len()],
        blocks: [None; ferraloom_data::BLOCKS.len()],
    };

    pub const fn with_entity(
        mut self,
        kind: ferraloom_data::EntityType,
        behavior: &'static dyn crate::entity::Entity,
    ) -> Self {
        self.entities[kind.protocol_id() as usize] = Some(behavior);
        self
    }

    pub const fn with_block(
        mut self,
        block: ferraloom_data::Block,
        behavior: &'static dyn super::BlockBehavior,
    ) -> Self {
        self.blocks[block.protocol_id() as usize] = Some(behavior);
        self
    }

    pub fn entity(
        &self,
        kind: ferraloom_data::EntityType,
    ) -> Option<&'static dyn crate::entity::Entity> {
        self.entities[kind.protocol_id() as usize]
    }

    pub fn block(&self, block: ferraloom_data::Block) -> Option<&'static dyn super::BlockBehavior> {
        self.blocks[block.protocol_id() as usize]
    }
}

/// Static class-to-dispatcher table. Exact class matching prevents an unported
/// subclass from accidentally inheriting silent base behavior.
static VANILLA_CLASSES: [Option<&'static dyn super::BlockBehavior>;
    ferraloom_data::BlockClass::ALL.len()] = {
    use ferraloom_data::BlockClass;
    let mut classes: [Option<&'static dyn super::BlockBehavior>; BlockClass::ALL.len()] =
        [None; BlockClass::ALL.len()];
    classes[BlockClass::Block as usize] = Some(&super::BASE_BLOCK);
    classes[BlockClass::AirBlock as usize] = Some(&super::foundational::AIR_BLOCK);
    classes[BlockClass::RotatedPillarBlock as usize] = Some(&super::BASE_BLOCK);
    classes[BlockClass::SandBlock as usize] = Some(&super::foundational::FALLING_BLOCK);
    classes[BlockClass::ColoredFallingBlock as usize] = Some(&super::foundational::FALLING_BLOCK);
    classes[BlockClass::FlowerBlock as usize] = Some(&super::foundational::VEGETATION_BLOCK);
    classes[BlockClass::TallGrassBlock as usize] = Some(&super::foundational::VEGETATION_BLOCK);
    classes[BlockClass::BushBlock as usize] = Some(&super::foundational::VEGETATION_BLOCK);
    classes[BlockClass::ChainBlock as usize] = Some(&super::foundational::CHAIN_BLOCK);
    classes[BlockClass::SlabBlock as usize] = Some(&super::foundational::SLAB_WATERLOGGING);
    classes[BlockClass::LiquidBlock as usize] = Some(&super::fluid::LIQUID_BLOCK);
    classes
};
pub fn vanilla_behavior(block: ferraloom_data::Block) -> Option<&'static dyn super::BlockBehavior> {
    VANILLA_CLASSES[block.class() as usize]
}

/// The class port must explicitly implement the method vanilla overrides.
/// Non-overridden methods select the reflected base owner before this table.
pub(crate) fn ported_behavior(
    block: ferraloom_data::Block,
    method: super::BlockMethod,
) -> Option<&'static dyn super::BlockBehavior> {
    if method == super::BlockMethod::SpawnDestroyParticles
        && block.is_instance_of(ferraloom_data::BlockClass::StrawBedBlock)
    {
        return Some(&super::STRAW_BED_PARTICLES);
    }
    if method == super::BlockMethod::SpawnDestroyByEntityParticles
        && block.is_instance_of(ferraloom_data::BlockClass::BaseFireBlock)
    {
        return Some(&super::NO_DESTROY_PARTICLES);
    }
    if let Some(behavior) = super::families::ported_behavior(block, method) {
        return Some(behavior);
    }
    use super::BlockMethod as Method;
    use ferraloom_data::BlockClass;
    let supported = match block.class() {
        BlockClass::Block | BlockClass::AirBlock | BlockClass::RotatedPillarBlock => {
            matches!(method, Method::CollisionShape | Method::OutlineShape)
        }
        BlockClass::SandBlock | BlockClass::ColoredFallingBlock => matches!(
            method,
            Method::OnPlace
                | Method::UpdateShape
                | Method::UpdateFromNeighbourShapes
                | Method::Tick
        ),
        BlockClass::FlowerBlock | BlockClass::TallGrassBlock | BlockClass::BushBlock => matches!(
            method,
            Method::CanSurvive
                | Method::UpdateShape
                | Method::UpdateFromNeighbourShapes
                | Method::CollisionShape
        ),
        BlockClass::ChainBlock => matches!(
            method,
            Method::UpdateShape
                | Method::UpdateFromNeighbourShapes
                | Method::CanPlaceLiquid
                | Method::PlaceLiquid
                | Method::PickupBlock
        ),
        BlockClass::SlabBlock => matches!(
            method,
            Method::CanPlaceLiquid
                | Method::PlaceLiquid
                | Method::UpdateShape
                | Method::UpdateFromNeighbourShapes
        ),
        BlockClass::LiquidBlock => matches!(
            method,
            Method::IsPathfindable
                | Method::GetDrops
                | Method::OnPlace
                | Method::NeighborChanged
                | Method::UpdateShape
                | Method::UpdateFromNeighbourShapes
                | Method::Tick
                | Method::RandomTick
                | Method::CollisionShape
                | Method::OutlineShape
                | Method::PickupBlock
        ),
        _ => false,
    };
    supported.then(|| vanilla_behavior(block)).flatten()
}

/// Select the reflected base owner or the registered override. Shared live and
/// worldgen calls use this exact registration; live test fixtures may override it.
pub(crate) fn registered_behavior(
    block: ferraloom_data::Block,
    method: super::BlockMethod,
) -> Option<&'static dyn super::BlockBehavior> {
    use super::BlockMethod;
    if method
        .vanilla_method()
        .is_some_and(|method| !block.overrides(method))
    {
        if matches!(
            method,
            BlockMethod::CanPlaceLiquid | BlockMethod::PlaceLiquid | BlockMethod::PickupBlock
        ) {
            return Some(&super::fluid::SIMPLE_WATERLOGGED_BLOCK);
        }
        return Some(&super::BASE_BLOCK);
    }
    ported_behavior(block, method)
}
