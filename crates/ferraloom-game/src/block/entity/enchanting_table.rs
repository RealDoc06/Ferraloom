//! EnchantingTableBlockEntity name persistence and client-only book state.
use super::{BlockEntity, BlockEntityData, BlockEntityState};
use ferraloom_nbt::CompoundTag;
use ferraloom_protocol::{
    chat::Component,
    component::{ComponentTypeId, DataComponent, DataComponentMap},
};

/// Port of EnchantingTableBlockEntity. Its book animation has no server ticker.
#[derive(Clone, Debug)]
pub struct EnchantingTableBlockEntity {
    pub parent: BlockEntityData,
    pub custom_name: Option<Component>,
    pub time: i32,
    pub flip: f32,
    pub old_flip: f32,
    pub flip_target: f32,
    pub flip_acceleration: f32,
    pub open: f32,
    pub old_open: f32,
    pub rotation: f32,
    pub old_rotation: f32,
    pub target_rotation: f32,
}
impl EnchantingTableBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            custom_name: None,
            time: 0,
            flip: 0.0,
            old_flip: 0.0,
            flip_target: 0.0,
            flip_acceleration: 0.0,
            open: 0.0,
            old_open: 0.0,
            rotation: 0.0,
            old_rotation: 0.0,
            target_rotation: 0.0,
        }
    }
    pub fn get_name(&self) -> Component {
        self.custom_name
            .clone()
            .unwrap_or_else(|| Component::translatable("container.enchant", Vec::new()))
    }
}
pub(super) struct EnchantingTableBehavior;
pub(super) static ENCHANTING_TABLE_ENTITY: EnchantingTableBehavior = EnchantingTableBehavior;
impl BlockEntity for EnchantingTableBehavior {
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        _registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        let BlockEntityState::EnchantingTable(entity) = state else {
            unreachable!()
        };
        entity.custom_name = tag
            .get("CustomName")
            .and_then(|value| Component::from_nbt(value.clone()).ok());
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::EnchantingTable(entity) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        if let Some(name) = &entity.custom_name {
            tag.insert("CustomName", name.to_nbt()?);
        }
        Ok(tag)
    }
    fn apply_implicit_components(
        &self,
        state: &mut BlockEntityState,
        map: &DataComponentMap,
    ) -> &'static [ComponentTypeId] {
        let BlockEntityState::EnchantingTable(entity) = state else {
            unreachable!()
        };
        entity.custom_name = match map.get(ComponentTypeId::CUSTOM_NAME) {
            Some(DataComponent::CustomName(name)) => Some((**name).clone()),
            _ => None,
        };
        &[ComponentTypeId::CUSTOM_NAME]
    }
    fn collect_implicit_components(&self, state: &BlockEntityState, map: &mut DataComponentMap) {
        let BlockEntityState::EnchantingTable(entity) = state else {
            unreachable!()
        };
        crate::block::containers::storage::collect_custom_name(map, entity.custom_name.as_ref());
    }
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        tag.remove("CustomName");
    }
}
