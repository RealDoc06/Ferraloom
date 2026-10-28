use super::storage::{RANDOMIZABLE_COMPONENTS, RandomizableContainerBlockEntity};
use crate::block::entity::{BlockEntity, BlockEntityData, BlockEntityState};
use ferraloom_nbt::CompoundTag;
use ferraloom_protocol::component::{ComponentTypeId, DataComponentMap};

/// Port of ChestBlockEntity and TrappedChestBlockEntity's canonical storage.
#[derive(Clone, Debug)]
pub struct ChestBlockEntity {
    pub parent: RandomizableContainerBlockEntity,
    pub open_count: i32,
    /// ContainerOpenersCounter's transient search radius, updated after opener callbacks.
    pub max_interaction_range: f64,
    pub lid_open: bool,
}
impl ChestBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent: RandomizableContainerBlockEntity::new(parent, 27),
            open_count: 0,
            max_interaction_range: 0.0,
            lid_open: false,
        }
    }
    pub fn trapped_signal(&self) -> i32 {
        self.open_count.clamp(0, 15)
    }
}
#[derive(Clone, Debug)]
pub struct BarrelBlockEntity {
    pub parent: RandomizableContainerBlockEntity,
    pub open_count: i32,
}
impl BarrelBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent: RandomizableContainerBlockEntity::new(parent, 27),
            open_count: 0,
        }
    }
}
#[derive(Clone, Debug)]
pub struct ShulkerBoxBlockEntity {
    pub parent: RandomizableContainerBlockEntity,
    pub open_count: i32,
}
impl ShulkerBoxBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent: RandomizableContainerBlockEntity::new(parent, 27),
            open_count: 0,
        }
    }
}

pub struct StorageBehavior;
pub static STORAGE_ENTITY: StorageBehavior = StorageBehavior;
impl BlockEntity for StorageBehavior {
    fn pre_remove_side_effects(
        &self,
        entity: crate::block::entity::BlockEntityRef,
        level: &mut crate::level::Level,
    ) -> Result<(), crate::level::LevelError> {
        if level.block_entity_state(entity)?.data().kind
            == crate::block::entity::BlockEntityType::SHULKER_BOX
        {
            return Ok(());
        }
        super::drops::drop_contents(entity, level)
    }
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        for key in ["CustomName", "lock", "Items", "LootTable", "LootTableSeed"] {
            tag.remove(key);
        }
    }
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        _registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        state
            .randomizable_mut()
            .expect("randomizable container")
            .load(tag);
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        state
            .randomizable()
            .expect("randomizable container")
            .save(!matches!(state, BlockEntityState::ShulkerBox(_)))
    }
    fn apply_implicit_components(
        &self,
        state: &mut BlockEntityState,
        map: &DataComponentMap,
    ) -> &'static [ComponentTypeId] {
        state
            .randomizable_mut()
            .expect("randomizable container")
            .apply_components(map);
        RANDOMIZABLE_COMPONENTS
    }
    fn collect_implicit_components(&self, state: &BlockEntityState, map: &mut DataComponentMap) {
        state
            .randomizable()
            .expect("randomizable container")
            .collect_components(map);
    }
    fn trigger_event(&self, state: &mut BlockEntityState, event: i32, value: i32) -> bool {
        match state {
            BlockEntityState::Chest(chest) if event == 1 => {
                chest.lid_open = value > 0;
                true
            }
            _ => false,
        }
    }
}
