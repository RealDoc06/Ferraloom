//! Retain double-container identity without changing upstream storage behavior.

use std::{any::Any, sync::Arc};

use pumpkin_data::item_stack::ItemStack;
use pumpkin_inventory::{Clearable, Inventory, double::DoubleInventory};

/// The pinned DoubleInventory exposes no half accessors. Keep its original
/// inventories alive and delegate operations to it; recounts need exact identity,
/// including when a menu outlives replacement of a block at the same position.
pub(crate) struct DoubleContainerInventory {
    inner: Arc<DoubleInventory>,
    first: Arc<dyn Inventory>,
    second: Arc<dyn Inventory>,
}

impl DoubleContainerInventory {
    pub(crate) fn new(first: Arc<dyn Inventory>, second: Arc<dyn Inventory>) -> Arc<Self> {
        Arc::new(Self {
            inner: DoubleInventory::new(first.clone(), second.clone()),
            first,
            second,
        })
    }

    pub(crate) fn contains<T: Inventory + 'static>(&self, inventory: &T) -> bool {
        [&self.first, &self.second].into_iter().any(|half| {
            half.as_any()
                .downcast_ref::<T>()
                .is_some_and(|half| std::ptr::eq(half, inventory))
        })
    }
}

impl Inventory for DoubleContainerInventory {
    fn size(&self) -> usize {
        self.inner.size()
    }
    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    fn get_stack(&self, slot: usize) -> ItemStack {
        self.inner.get_stack(slot)
    }
    fn remove_stack(&self, slot: usize) -> ItemStack {
        self.inner.remove_stack(slot)
    }
    fn remove_stack_specific(&self, slot: usize, amount: u8) -> ItemStack {
        self.inner.remove_stack_specific(slot, amount)
    }
    fn set_stack(&self, slot: usize, stack: ItemStack) {
        self.inner.set_stack(slot, stack);
    }
    fn on_open(&self) {
        self.inner.on_open();
    }
    fn on_close(&self) {
        self.inner.on_close();
    }
    fn get_max_count_per_stack(&self) -> u8 {
        self.inner.get_max_count_per_stack()
    }
    fn mark_dirty(&self) {
        self.inner.mark_dirty();
    }
    fn is_valid_slot_for(&self, slot: usize, stack: &ItemStack) -> bool {
        self.inner.is_valid_slot_for(slot, stack)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Clearable for DoubleContainerInventory {
    fn clear(&self) {
        self.inner.clear();
    }
}
