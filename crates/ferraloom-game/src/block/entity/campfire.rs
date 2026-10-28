//! Campfire storage; cooking and cooling tickers remain required behavior ports.
use super::{BlockEntity, BlockEntityData, BlockEntityRef, BlockEntityState};
use crate::{
    block::containers::storage,
    inventory::StackGenerations,
    level::{Level, LevelError},
};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::component::{ComponentTypeId, DataComponentMap, ItemStack};

/// Port of CampfireBlockEntity's final four-slot list and timer arrays.
#[derive(Clone, Debug)]
pub struct CampfireBlockEntity {
    pub parent: BlockEntityData,
    pub items: [ItemStack; 4],
    pub cooking_progress: [i32; 4],
    pub cooking_time: [i32; 4],
    stack_generations: StackGenerations,
}
impl CampfireBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            items: std::array::from_fn(|_| ItemStack::default()),
            cooking_progress: [0; 4],
            cooking_time: [0; 4],
            stack_generations: StackGenerations::new(4),
        }
    }
    pub fn stack_parts(&self, slot: usize) -> Option<(&StackGenerations, &ItemStack, usize)> {
        Some((&self.stack_generations, self.items.get(slot)?, slot))
    }
    pub fn stack_parts_mut(
        &mut self,
        slot: usize,
    ) -> Option<(&mut StackGenerations, &mut ItemStack, usize)> {
        Some((&mut self.stack_generations, self.items.get_mut(slot)?, slot))
    }
    fn replace_item(&mut self, slot: usize, item: ItemStack) {
        self.stack_generations
            .replace(slot, &mut self.items[slot], item);
    }
    /// Port of clearContent: the timers belong to separate final arrays.
    pub fn clear_content(&mut self) {
        for slot in 0..4 {
            self.replace_item(slot, ItemStack::default());
        }
    }
}
fn load_timer(tag: &CompoundTag, key: &str, target: &mut [i32; 4]) {
    if let Some(values) = tag.get_int_array(key) {
        let count = values.len().min(target.len());
        target[..count].copy_from_slice(&values[..count]);
    } else {
        target.fill(0);
    }
}
pub(super) struct CampfireBehavior;
pub(super) static CAMPFIRE_ENTITY: CampfireBehavior = CampfireBehavior;
impl BlockEntity for CampfireBehavior {
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        _registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        let BlockEntityState::Campfire(entity) = state else {
            unreachable!()
        };
        entity.clear_content();
        for (slot, item) in storage::item_entries(tag, 4) {
            entity.replace_item(slot, item);
        }
        // Native arraycopy preserves the old tail, including for an empty array.
        load_timer(tag, "CookingTimes", &mut entity.cooking_progress);
        load_timer(tag, "CookingTotalTimes", &mut entity.cooking_time);
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::Campfire(entity) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        storage::save_items(&mut tag, &entity.items, true)?;
        tag.insert(
            "CookingTimes",
            Tag::IntArray(entity.cooking_progress.to_vec()),
        );
        tag.insert(
            "CookingTotalTimes",
            Tag::IntArray(entity.cooking_time.to_vec()),
        );
        Ok(tag)
    }
    fn get_update_tag(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::Campfire(entity) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        storage::save_items(&mut tag, &entity.items, true)?;
        Ok(tag)
    }
    fn sends_update_packet(&self) -> bool {
        true
    }
    fn has_server_ticker(&self) -> bool {
        true
    }
    fn tick(&self, entity: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
        Err(LevelError::UnsupportedBlockEntity(
            level.block_entity_state(entity)?.data().block_state.block(),
        ))
    }
    fn pre_remove_side_effects(
        &self,
        handle: BlockEntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let entity = level.block_entity::<CampfireBlockEntity>(handle)?;
        // Containers#dropItemStack draws three offsets even for each empty
        // slot, so an attached empty campfire is not a no-effect removal.
        if entity.parent.has_level {
            return Err(LevelError::UnsupportedBlockEntity(
                entity.parent.block_state.block(),
            ));
        }
        Ok(())
    }
    fn apply_implicit_components(
        &self,
        state: &mut BlockEntityState,
        components: &DataComponentMap,
    ) -> &'static [ComponentTypeId] {
        let BlockEntityState::Campfire(entity) = state else {
            unreachable!()
        };
        for slot in 0..4 {
            entity.replace_item(slot, storage::component_item(components, slot));
        }
        &[ComponentTypeId::CONTAINER]
    }
    fn collect_implicit_components(
        &self,
        state: &BlockEntityState,
        components: &mut DataComponentMap,
    ) {
        let BlockEntityState::Campfire(entity) = state else {
            unreachable!()
        };
        storage::set(components, storage::container_component(&entity.items));
    }
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        tag.remove("Items");
    }
}
