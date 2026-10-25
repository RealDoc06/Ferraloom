//! ChiseledBookShelfBlockEntity data. This ListBackedContainer extends the
//! BlockEntity base, without BaseContainer's name, lock or lazy loot fields.
use super::{BlockEntity, BlockEntityData, BlockEntityRef, BlockEntityState};
use crate::{
    block::containers::storage,
    inventory::StackGenerations,
    level::{Level, LevelError},
};
use ferraloom_data::properties as p;
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::component::{
    BoundedList, ComponentTypeId, DataComponent, DataComponentMap, ItemStack, ItemStackTemplate,
};

pub const MAX_BOOKS_IN_STORAGE: usize = 6;

/// Port of ChiseledBookShelfBlockEntity#updateState. Its overrides do not call
/// setChanged: setBlockAndUpdate precedes BLOCK_CHANGE even for an identical
/// resulting state. Slot#setChanged remains its caller's separate operation.
pub(crate) fn update_state(
    handle: BlockEntityRef,
    slot: usize,
    level: &mut Level,
) -> Result<(), LevelError> {
    if slot >= MAX_BOOKS_IN_STORAGE {
        eprintln!("Expected slot 0-5, got {slot}");
        return Ok(());
    }
    let entity = level.block_entity_mut::<ChiseledBookShelfBlockEntity>(handle)?;
    entity.last_interacted_slot = slot as i32;
    let mut state = entity.parent.block_state;
    for (slot, property) in [
        p::SLOT_0_OCCUPIED,
        p::SLOT_1_OCCUPIED,
        p::SLOT_2_OCCUPIED,
        p::SLOT_3_OCCUPIED,
        p::SLOT_4_OCCUPIED,
        p::SLOT_5_OCCUPIED,
    ]
    .into_iter()
    .enumerate()
    {
        state = state
            .with(property, !entity.items[slot].is_empty())
            .expect("bookshelf occupied property");
    }
    if !entity.parent.has_level {
        return Err(LevelError::Inventory("bookshelf updateState has no level"));
    }
    level.set_block(
        handle.pos,
        state,
        crate::block::UpdateFlags::ALL,
        crate::block::UPDATE_LIMIT,
    )?;
    level.game_event(
        ferraloom_data::registries::game_event::Id::BLOCK_CHANGE,
        ferraloom_util::Vec3::new(
            f64::from(handle.pos.x) + 0.5,
            f64::from(handle.pos.y) + 0.5,
            f64::from(handle.pos.z) + 0.5,
        ),
        None,
        Some(state),
    )
}

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
        let contents = match map.get(ComponentTypeId::CONTAINER) {
            Some(DataComponent::Container(contents)) => Some(contents),
            _ => None,
        };
        for slot in 0..MAX_BOOKS_IN_STORAGE {
            let item = contents
                .and_then(|contents| contents.0.get(slot))
                .and_then(Option::as_ref)
                .map(|item| ItemStack::new(item.item, item.count, item.components.clone()))
                .unwrap_or_default();
            entity.replace_item(slot, item);
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
        if level.block_entity_state(handle)?.data().has_level {
            crate::block::containers::drops::drop_contents(handle, level)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
