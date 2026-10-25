//! ChiseledBookShelfBlockEntity data. This ListBackedContainer extends the
//! BlockEntity base, without BaseContainer's name, lock or lazy loot fields.
use super::{BlockEntity, BlockEntityData, BlockEntityRef, BlockEntityState};
use crate::{
    block::containers::storage,
    inventory::StackGenerations,
    level::{Level, LevelError},
};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::component::{
    BoundedList, ComponentTypeId, DataComponent, DataComponentMap, ItemStack, ItemStackTemplate,
};

pub const MAX_BOOKS_IN_STORAGE: usize = 6;

/// Port of ChiseledBookShelfBlockEntity's fixed, final six-slot list and scalar.
#[derive(Clone, Debug)]
pub struct ChiseledBookShelfBlockEntity {
    pub parent: BlockEntityData,
    pub items: [ItemStack; MAX_BOOKS_IN_STORAGE],
    stack_generations: StackGenerations,
    last_interacted_slot: i32,
}
impl ChiseledBookShelfBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            items: std::array::from_fn(|_| ItemStack::default()),
            stack_generations: StackGenerations::new(MAX_BOOKS_IN_STORAGE),
            last_interacted_slot: -1,
        }
    }
    /// Port of ChiseledBookShelfBlockEntity#getLastInteractedSlot.
    pub const fn last_interacted_slot(&self) -> i32 {
        self.last_interacted_slot
    }
    fn replace_item(&mut self, slot: usize, item: ItemStack) {
        self.stack_generations
            .replace(slot, &mut self.items[slot], item);
    }
    fn clear_items(&mut self) {
        for slot in 0..MAX_BOOKS_IN_STORAGE {
            self.replace_item(slot, ItemStack::default());
        }
    }
    /// Port of loadAdditional: clear the existing list, without its setItem
    /// filter, count limit, occupied-property updates or change callbacks.
    fn load(&mut self, tag: &CompoundTag) {
        self.clear_items();
        for (slot, item) in storage::item_entries(tag, MAX_BOOKS_IN_STORAGE) {
            self.replace_item(slot, item);
        }
        self.last_interacted_slot = tag.get_int_or("last_interacted_slot", -1);
    }
}

pub(super) struct ChiseledBookShelfBehavior;
pub(super) static CHISELED_BOOKSHELF_ENTITY: ChiseledBookShelfBehavior = ChiseledBookShelfBehavior;
impl BlockEntity for ChiseledBookShelfBehavior {
    fn load_additional(&self, state: &mut BlockEntityState, tag: &CompoundTag) {
        let BlockEntityState::ChiseledBookShelf(entity) = state else {
            unreachable!()
        };
        entity.load(tag);
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::ChiseledBookShelf(entity) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        storage::save_items(&mut tag, &entity.items, true)?;
        tag.insert(
            "last_interacted_slot",
            Tag::Int(entity.last_interacted_slot),
        );
        Ok(tag)
    }
    fn apply_implicit_components(
        &self,
        state: &mut BlockEntityState,
        map: &DataComponentMap,
    ) -> &'static [ComponentTypeId] {
        let BlockEntityState::ChiseledBookShelf(entity) = state else {
            unreachable!()
        };
        entity.clear_items();
        if let Some(DataComponent::Container(contents)) = map.get(ComponentTypeId::CONTAINER) {
            for (slot, item) in contents.0.iter().take(MAX_BOOKS_IN_STORAGE).enumerate() {
                if let Some(item) = item {
                    entity.replace_item(
                        slot,
                        ItemStack::new(item.item, item.count, item.components.clone()),
                    );
                }
            }
        }
        &[ComponentTypeId::CONTAINER]
    }
    fn collect_implicit_components(&self, state: &BlockEntityState, map: &mut DataComponentMap) {
        let BlockEntityState::ChiseledBookShelf(entity) = state else {
            unreachable!()
        };
        let end = entity
            .items
            .iter()
            .rposition(|item| !item.is_empty())
            .map_or(0, |slot| slot + 1);
        let contents = entity.items[..end]
            .iter()
            .map(|item| {
                if item.is_empty() {
                    None
                } else {
                    item.0.as_ref().map(|item| ItemStackTemplate {
                        item: item.item,
                        count: item.count,
                        components: item.components.clone(),
                    })
                }
            })
            .collect();
        storage::set(map, DataComponent::Container(BoundedList(contents)));
    }
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        tag.remove("Items");
    }
    fn pre_remove_side_effects(
        &self,
        handle: BlockEntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        Err(LevelError::UnsupportedBlockEntity(
            level.block_entity_state(handle)?.data().block_state.block(),
        ))
    }
}

#[cfg(test)]
mod tests;
