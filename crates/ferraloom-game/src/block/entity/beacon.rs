//! BeaconBlockEntity data, attachment and removal. Beam/effect ticking is required.
use super::{BlockEntity, BlockEntityData, BlockEntityRef, BlockEntityState};
use crate::{
    block::containers::storage,
    level::{Level, LevelError},
};
use ferraloom_data::registries::{mob_effect::Id as MobEffect, sound_event::Id as SoundEvent};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    chat::Component,
    component::{ComponentTypeId, DataComponent, DataComponentMap, LockCode, Persistent},
    play::SoundSource,
};
use ferraloom_util::Vec3;

/// Port of BeaconBeamOwner.Section.
#[derive(Clone, Debug)]
pub struct BeamSection {
    pub color: i32,
    pub height: i32,
}
/// Port of BeaconBlockEntity; its name and lock do not make it a Container.
#[derive(Clone, Debug)]
pub struct BeaconBlockEntity {
    pub parent: BlockEntityData,
    pub beam_sections: Vec<BeamSection>,
    pub checking_beam_sections: Vec<BeamSection>,
    pub levels: i32,
    pub last_check_y: i32,
    pub primary_power: Option<MobEffect>,
    pub secondary_power: Option<MobEffect>,
    pub custom_name: Option<Component>,
    pub lock: Option<LockCode>,
}
impl BeaconBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            beam_sections: Vec::new(),
            checking_beam_sections: Vec::new(),
            levels: 0,
            last_check_y: 0,
            primary_power: None,
            secondary_power: None,
            custom_name: None,
            lock: None,
        }
    }
    /// Port of BeaconBlockEntity#setLevel.
    pub fn set_level(&mut self, min_y: i32) {
        self.parent.has_level = true;
        self.last_check_y = min_y.wrapping_sub(1);
    }
    pub fn get_name(&self) -> Component {
        self.custom_name
            .clone()
            .unwrap_or_else(|| Component::translatable("container.beacon", Vec::new()))
    }
}
fn load_effect(tag: &CompoundTag, field: &str) -> Option<MobEffect> {
    let name = tag.get_string(field)?.to_string_checked().ok()?;
    let name = ferraloom_protocol::Identifier::parse(&name).ok()?;
    let effect = MobEffect::from_name(name.as_str())?;
    [
        MobEffect::SPEED,
        MobEffect::HASTE,
        MobEffect::RESISTANCE,
        MobEffect::JUMP_BOOST,
        MobEffect::STRENGTH,
        MobEffect::REGENERATION,
    ]
    .contains(&effect)
    .then_some(effect)
}
fn has_lock(lock: &LockCode) -> bool {
    lock.items.is_some()
        || lock.count.min.is_some()
        || lock.count.max.is_some()
        || !lock.components.exact.is_empty()
        || !lock.components.partial.0.is_empty()
}
pub(super) struct BeaconBehavior;
pub(super) static BEACON_ENTITY: BeaconBehavior = BeaconBehavior;
impl BlockEntity for BeaconBehavior {
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        _registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        let BlockEntityState::Beacon(entity) = state else {
            unreachable!()
        };
        entity.primary_power = load_effect(tag, "primary_effect");
        entity.secondary_power = load_effect(tag, "secondary_effect");
        entity.custom_name = tag
            .get("CustomName")
            .and_then(|tag| Component::from_nbt(tag.clone()).ok());
        entity.lock = tag.get("lock").and_then(|tag| LockCode::from_nbt(tag).ok());
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::Beacon(entity) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        for (field, power) in [
            ("primary_effect", entity.primary_power),
            ("secondary_effect", entity.secondary_power),
        ] {
            if let Some(power) = power {
                tag.insert(field, Tag::String(power.name().into()));
            }
        }
        tag.insert("Levels", Tag::Int(entity.levels));
        if let Some(name) = &entity.custom_name {
            tag.insert("CustomName", name.to_nbt()?);
        }
        if let Some(lock) = &entity.lock
            && has_lock(lock)
        {
            tag.insert("lock", lock.to_nbt()?);
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
        let BlockEntityState::Beacon(entity) = state else {
            unreachable!()
        };
        entity.custom_name = match map.get(ComponentTypeId::CUSTOM_NAME) {
            Some(DataComponent::CustomName(name)) => Some((**name).clone()),
            _ => None,
        };
        entity.lock = match map.get(ComponentTypeId::LOCK) {
            Some(DataComponent::Lock(lock)) => Some(lock.clone()),
            _ => None,
        };
        &[ComponentTypeId::CUSTOM_NAME, ComponentTypeId::LOCK]
    }
    fn collect_implicit_components(&self, state: &BlockEntityState, map: &mut DataComponentMap) {
        let BlockEntityState::Beacon(entity) = state else {
            unreachable!()
        };
        storage::collect_custom_name(map, entity.custom_name.as_ref());
        if let Some(lock) = &entity.lock
            && has_lock(lock)
        {
            storage::set(map, DataComponent::Lock(lock.clone()));
        }
    }
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        tag.remove("CustomName");
        tag.remove("lock");
    }
    fn has_server_ticker(&self) -> bool {
        true
    }
    fn tick(&self, handle: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
        Err(LevelError::UnsupportedBlockEntity(
            level.block_entity_state(handle)?.data().block_state.block(),
        ))
    }
    /// Native deactivation sound precedes BlockEntity#setRemoved.
    fn set_removed_live(
        &self,
        handle: BlockEntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level.play_sound(
            None,
            Vec3::new(
                f64::from(handle.pos.x) + 0.5,
                f64::from(handle.pos.y) + 0.5,
                f64::from(handle.pos.z) + 0.5,
            ),
            SoundEvent::BLOCK_BEACON_DEACTIVATE,
            SoundSource::Blocks,
            1.0,
            1.0,
        )?;
        level
            .block_entity_mut::<BlockEntityData>(handle)?
            .set_removed();
        Ok(())
    }
}
