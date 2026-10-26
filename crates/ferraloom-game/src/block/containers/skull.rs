//! Server-side SkullBlockEntity profile/name/sound data. Animation is client-only.
use crate::block::entity::{BlockEntity, BlockEntityData, BlockEntityState};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    Identifier,
    chat::Component,
    component::{ComponentTypeId, DataComponent, DataComponentMap, Persistent, ResolvableProfile},
};

/// Port of SkullBlockEntity's server-side profile, sound and name state.
#[derive(Clone, Debug)]
pub struct SkullBlockEntity {
    pub parent: BlockEntityData,
    pub owner: Option<ResolvableProfile>,
    pub note_block_sound: Option<Identifier>,
    pub custom_name: Option<Component>,
}
impl SkullBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            owner: None,
            note_block_sound: None,
            custom_name: None,
        }
    }
}
pub struct SkullBehavior;
pub static SKULL_ENTITY: SkullBehavior = SkullBehavior;
impl BlockEntity for SkullBehavior {
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        _registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        let BlockEntityState::Skull(e) = state else {
            unreachable!()
        };
        e.owner = tag
            .get("profile")
            .and_then(|v| ResolvableProfile::from_nbt(v).ok());
        e.note_block_sound = tag
            .get_string("note_block_sound")
            .and_then(|v| Identifier::parse(&v.to_string_lossy()).ok());
        e.custom_name = tag
            .get("custom_name")
            .and_then(|v| Component::from_nbt(v.clone()).ok());
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::Skull(e) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        if let Some(v) = &e.owner {
            tag.insert("profile", v.to_nbt()?);
        }
        if let Some(v) = &e.note_block_sound {
            tag.insert("note_block_sound", Tag::String(v.as_str().into()));
        }
        if let Some(v) = &e.custom_name {
            tag.insert("custom_name", v.to_nbt()?);
        }
        Ok(tag)
    }
    fn get_update_tag(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        self.save_additional(state)
    }
    fn sends_update_packet(&self) -> bool {
        true
    }
    fn apply_implicit_components(
        &self,
        state: &mut BlockEntityState,
        map: &DataComponentMap,
    ) -> &'static [ComponentTypeId] {
        let BlockEntityState::Skull(e) = state else {
            unreachable!()
        };
        e.owner = match map.get(ComponentTypeId::PROFILE) {
            Some(DataComponent::Profile(v)) => Some(v.clone()),
            _ => None,
        };
        e.note_block_sound = match map.get(ComponentTypeId::NOTE_BLOCK_SOUND) {
            Some(DataComponent::NoteBlockSound(v)) => Some(v.clone()),
            _ => None,
        };
        e.custom_name = match map.get(ComponentTypeId::CUSTOM_NAME) {
            Some(DataComponent::CustomName(v)) => Some((**v).clone()),
            _ => None,
        };
        &[
            ComponentTypeId::PROFILE,
            ComponentTypeId::NOTE_BLOCK_SOUND,
            ComponentTypeId::CUSTOM_NAME,
        ]
    }
    fn collect_implicit_components(&self, state: &BlockEntityState, map: &mut DataComponentMap) {
        let BlockEntityState::Skull(e) = state else {
            unreachable!()
        };
        if let Some(v) = &e.owner {
            super::storage::set(map, DataComponent::Profile(v.clone()));
        }
        if let Some(v) = &e.note_block_sound {
            super::storage::set(map, DataComponent::NoteBlockSound(v.clone()));
        }
        if let Some(v) = &e.custom_name {
            super::storage::set(map, DataComponent::CustomName(Box::new(v.clone())));
        }
    }
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        for key in ["profile", "note_block_sound", "custom_name"] {
            tag.remove(key);
        }
    }
}
