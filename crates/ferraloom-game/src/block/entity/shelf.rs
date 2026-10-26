//! ShelfBlockEntity's fixed list and alignment state. Live swaps are a later port.
use super::{BlockEntity, BlockEntityData, BlockEntityState};
use crate::{block::containers::storage, inventory::StackGenerations};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::component::{ComponentTypeId, DataComponentMap, ItemStack};

/// Port of ShelfBlockEntity's final three-slot NonNullList.
#[derive(Clone, Debug)]
pub struct ShelfBlockEntity {
    pub parent: BlockEntityData,
    pub items: [ItemStack; 3],
    pub align_items_to_bottom: bool,
    stack_generations: StackGenerations,
}
impl ShelfBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            items: std::array::from_fn(|_| ItemStack::default()),
            align_items_to_bottom: false,
            stack_generations: StackGenerations::new(3),
        }
    }
    fn replace_item(&mut self, slot: usize, item: ItemStack) {
        self.stack_generations
            .replace(slot, &mut self.items[slot], item);
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
    /// Port of ListBackedContainer#clearContent, without setChanged callbacks.
    pub fn clear_content(&mut self) {
        for slot in 0..3 {
            self.replace_item(slot, ItemStack::default());
        }
    }
}
pub(super) struct ShelfBehavior;
pub(super) static SHELF_ENTITY: ShelfBehavior = ShelfBehavior;
impl BlockEntity for ShelfBehavior {
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        _registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        let BlockEntityState::Shelf(entity) = state else {
            unreachable!()
        };
        entity.clear_content();
        for (slot, item) in storage::item_entries(tag, 3) {
            entity.replace_item(slot, item);
        }
        entity.align_items_to_bottom = tag.get_boolean_or("align_items_to_bottom", false);
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::Shelf(entity) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        storage::save_items(&mut tag, &entity.items, true)?;
        tag.insert(
            "align_items_to_bottom",
            Tag::Byte(i8::from(entity.align_items_to_bottom)),
        );
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
        let BlockEntityState::Shelf(entity) = state else {
            unreachable!()
        };
        for slot in 0..3 {
            entity.replace_item(slot, storage::component_item(map, slot));
        }
        &[ComponentTypeId::CONTAINER]
    }
    fn collect_implicit_components(&self, state: &BlockEntityState, map: &mut DataComponentMap) {
        let BlockEntityState::Shelf(entity) = state else {
            unreachable!()
        };
        storage::set(map, storage::container_component(&entity.items));
    }
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        tag.remove("Items");
    }
}
