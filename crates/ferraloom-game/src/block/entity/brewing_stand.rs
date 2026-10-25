//! BrewingStandBlockEntity's persistent container and brewing state.
use super::{BlockEntity, BlockEntityData, BlockEntityRef, BlockEntityState};
use crate::{
    block::containers::storage::{self, BASE_COMPONENTS, BaseContainerBlockEntity},
    crafting::{CachedCheck, RecipeType},
    level::{Level, LevelError},
};
use ferraloom_data::{Block, Item};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::component::{ComponentTypeId, DataComponentMap};

/// Port of BrewingStandBlockEntity. Inventory/name/lock storage has one base;
/// brewing recipes/fuel ticking remain required until the live ticker is ported.
#[derive(Clone, Debug)]
pub struct BrewingStandBlockEntity {
    pub parent: BaseContainerBlockEntity,
    pub brew_time: i32,
    pub total_brew_time: i32,
    pub last_potion_count: Option<[bool; 3]>,
    pub ingredient: Option<Item>,
    pub fuel: i32,
    pub total_fuel: i32,
    pub speed_multiplier: f32,
    pub quick_check: CachedCheck,
}
impl BrewingStandBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent: BaseContainerBlockEntity::new(parent, 5),
            brew_time: 0,
            total_brew_time: 0,
            last_potion_count: None,
            ingredient: None,
            fuel: 0,
            total_fuel: 0,
            speed_multiplier: 1.0,
            quick_check: CachedCheck::new(RecipeType::Brewing),
        }
    }
    /// Port of the native ContainerData four-entry view.
    pub fn data(&self, index: usize) -> i32 {
        match index {
            0 => self.brew_time,
            1 => self.fuel,
            2 => self.total_brew_time,
            3 => self.total_fuel,
            _ => 0,
        }
    }
    pub fn set_data(&mut self, index: usize, value: i32) {
        match index {
            0 => self.brew_time = value,
            1 => self.fuel = value,
            2 => self.total_brew_time = value,
            3 => self.total_fuel = value,
            _ => {}
        }
    }
    fn load(&mut self, tag: &CompoundTag) {
        self.parent.load_base(tag);
        self.parent.load_items(tag);
        self.brew_time = tag.get_int_or("BrewTime", 0);
        self.total_brew_time = tag.get_int_or("total_brew_time", 400);
        if self.brew_time > 0 {
            self.ingredient = Some(self.parent.items[3].item());
        }
        self.fuel = tag.get_int_or("Fuel", 0);
        self.total_fuel = tag.get_int_or("total_fuel", 20);
        self.speed_multiplier = tag.get_float_or("speed_multiplier", 1.0);
    }
}
pub(super) struct BrewingStandBehavior;
pub(super) static BREWING_STAND_ENTITY: BrewingStandBehavior = BrewingStandBehavior;
impl BlockEntity for BrewingStandBehavior {
    fn has_server_ticker(&self) -> bool {
        true
    }
    fn tick(&self, _handle: BlockEntityRef, _level: &mut Level) -> Result<(), LevelError> {
        Err(LevelError::UnsupportedBlockEntity(Block::BREWING_STAND))
    }
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        _registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        let BlockEntityState::BrewingStand(entity) = state else {
            unreachable!()
        };
        entity.load(tag);
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::BrewingStand(entity) = state else {
            unreachable!()
        };
        let mut tag = entity.parent.save_base()?;
        storage::save_items(&mut tag, &entity.parent.items, true)?;
        for (name, value) in [
            ("BrewTime", entity.brew_time),
            ("total_brew_time", entity.total_brew_time),
            ("Fuel", entity.fuel),
            ("total_fuel", entity.total_fuel),
        ] {
            tag.insert(name, Tag::Int(value));
        }
        tag.insert("speed_multiplier", Tag::Float(entity.speed_multiplier));
        Ok(tag)
    }
    fn apply_implicit_components(
        &self,
        state: &mut BlockEntityState,
        map: &DataComponentMap,
    ) -> &'static [ComponentTypeId] {
        let BlockEntityState::BrewingStand(entity) = state else {
            unreachable!()
        };
        entity.parent.apply_components(map);
        BASE_COMPONENTS
    }
    fn collect_implicit_components(&self, state: &BlockEntityState, map: &mut DataComponentMap) {
        let BlockEntityState::BrewingStand(entity) = state else {
            unreachable!()
        };
        entity.parent.collect_components(map);
    }
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        for key in ["CustomName", "lock", "Items"] {
            tag.remove(key);
        }
    }
    fn pre_remove_side_effects(
        &self,
        handle: BlockEntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        crate::block::containers::drops::drop_contents(handle, level)
    }
}
