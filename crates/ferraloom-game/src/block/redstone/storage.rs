//! Canonical inventory state owned by redstone block entities.
use crate::block::{
    containers::storage::{RANDOMIZABLE_COMPONENTS, RandomizableContainerBlockEntity},
    entity::{BlockEntity, BlockEntityData, BlockEntityDataView, BlockEntityRef, BlockEntityState},
};
use crate::level::{Level, LevelError};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::component::{ComponentTypeId, DataComponentMap};
use ferraloom_util::random::RandomSource;

/// Port of DispenserBlockEntity, also the parent storage of DropperBlockEntity.
#[derive(Clone, Debug)]
pub struct DispenserBlockEntity {
    pub parent: RandomizableContainerBlockEntity,
}
impl DispenserBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent: RandomizableContainerBlockEntity::new(parent, 9),
        }
    }
    /// Port of DispenserBlockEntity#getRandomSlot after unpackLootTable. The
    /// caller must resolve loot first; each occupied slot consumes a bound draw.
    pub fn get_random_slot(&self, random: &mut impl RandomSource) -> Option<usize> {
        let mut chosen = None;
        let mut odds = 1;
        for (i, item) in self.parent.parent.items.iter().enumerate() {
            if !item.is_empty() {
                if random.next_int_bound(odds) == 0 {
                    chosen = Some(i);
                }
                odds += 1;
            }
        }
        chosen
    }
}
/// Port of HopperBlockEntity. Facing is read from the parent's canonical state
/// so BlockEntity#setBlockState never leaves a second direction cache stale.
#[derive(Clone, Debug)]
pub struct HopperBlockEntity {
    pub parent: RandomizableContainerBlockEntity,
    pub cooldown_time: i32,
    pub ticked_game_time: i64,
}
impl HopperBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent: RandomizableContainerBlockEntity::new(parent, 5),
            cooldown_time: -1,
            ticked_game_time: 0,
        }
    }
    pub fn is_on_cooldown(&self) -> bool {
        self.cooldown_time > 0
    }
    pub fn is_on_custom_cooldown(&self) -> bool {
        self.cooldown_time > 8
    }
}

pub struct DispenserEntityBehavior;
pub static DISPENSER_ENTITY: DispenserEntityBehavior = DispenserEntityBehavior;
impl BlockEntity for DispenserEntityBehavior {
    fn load_additional(&self, state: &mut BlockEntityState, tag: &CompoundTag) {
        if let Some(e) = DispenserBlockEntity::view_mut(state) {
            e.parent.load(tag);
        }
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        DispenserBlockEntity::view(state)
            .expect("dispenser dispatcher")
            .parent
            .save(false)
    }
    fn apply_implicit_components(
        &self,
        state: &mut BlockEntityState,
        components: &DataComponentMap,
    ) -> &'static [ComponentTypeId] {
        if let Some(e) = DispenserBlockEntity::view_mut(state) {
            e.parent.apply_components(components);
        }
        RANDOMIZABLE_COMPONENTS
    }
    fn collect_implicit_components(
        &self,
        state: &BlockEntityState,
        components: &mut DataComponentMap,
    ) {
        if let Some(e) = DispenserBlockEntity::view(state) {
            e.parent.collect_components(components);
        }
    }
    fn pre_remove_side_effects(
        &self,
        entity: BlockEntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let _ = entity;
        let _ = level;
        Err(LevelError::UnsupportedBlockEntity(
            ferraloom_data::Block::DISPENSER,
        ))
    }
}
pub struct HopperEntityBehavior;
pub static HOPPER_ENTITY: HopperEntityBehavior = HopperEntityBehavior;
impl BlockEntity for HopperEntityBehavior {
    fn load_additional(&self, state: &mut BlockEntityState, tag: &CompoundTag) {
        if let Some(e) = HopperBlockEntity::view_mut(state) {
            e.parent.load(tag);
            e.cooldown_time = tag.get_int_or("TransferCooldown", -1);
        }
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let e = HopperBlockEntity::view(state).expect("hopper dispatcher");
        let mut tag = e.parent.save(false)?;
        tag.insert("TransferCooldown", Tag::Int(e.cooldown_time));
        Ok(tag)
    }
    fn apply_implicit_components(
        &self,
        state: &mut BlockEntityState,
        components: &DataComponentMap,
    ) -> &'static [ComponentTypeId] {
        if let Some(e) = HopperBlockEntity::view_mut(state) {
            e.parent.apply_components(components);
        }
        RANDOMIZABLE_COMPONENTS
    }
    fn collect_implicit_components(
        &self,
        state: &BlockEntityState,
        components: &mut DataComponentMap,
    ) {
        if let Some(e) = HopperBlockEntity::view(state) {
            e.parent.collect_components(components);
        }
    }
    fn has_server_ticker(&self) -> bool {
        true
    }
    fn tick(&self, _entity: BlockEntityRef, _level: &mut Level) -> Result<(), LevelError> {
        Err(LevelError::UnsupportedBlockEntity(
            ferraloom_data::Block::HOPPER,
        ))
    }
    fn pre_remove_side_effects(
        &self,
        _entity: BlockEntityRef,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Err(LevelError::UnsupportedBlockEntity(
            ferraloom_data::Block::HOPPER,
        ))
    }
}
