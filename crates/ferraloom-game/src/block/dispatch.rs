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
    classes[BlockClass::AirBlock as usize] = Some(&super::BASE_BLOCK);
    classes[BlockClass::RotatedPillarBlock as usize] = Some(&super::BASE_BLOCK);
    classes[BlockClass::SandBlock as usize] = Some(&super::foundational::FALLING_BLOCK);
    classes[BlockClass::ColoredFallingBlock as usize] = Some(&super::foundational::FALLING_BLOCK);
    classes[BlockClass::FlowerBlock as usize] = Some(&super::foundational::VEGETATION_BLOCK);
    classes[BlockClass::TallGrassBlock as usize] = Some(&super::foundational::VEGETATION_BLOCK);
    classes[BlockClass::BushBlock as usize] = Some(&super::foundational::VEGETATION_BLOCK);
    classes[BlockClass::LiquidBlock as usize] = Some(&super::fluid::LIQUID_BLOCK);
    classes
};
pub fn vanilla_behavior(block: ferraloom_data::Block) -> Option<&'static dyn super::BlockBehavior> {
    VANILLA_CLASSES[block.class() as usize]
}
