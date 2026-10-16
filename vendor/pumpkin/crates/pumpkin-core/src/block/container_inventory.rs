//! World-aware menu callbacks with the underlying container's exact identity.

use std::{any::Any, sync::Arc};

use pumpkin_data::item_stack::ItemStack;
use pumpkin_inventory::{Clearable, Inventory, double::DoubleInventory};

use crate::{
    block::{
        entities::BlockEntity,
        viewer::{ContainerViewerCount, ViewerCountListener},
    },
    world::World,
};

pub trait ViewedContainer: BlockEntity + Inventory + ViewerCountListener {
    fn viewers(&self) -> &ContainerViewerCount;
}

/// The upstream Inventory callbacks have no World argument. Menus retain this
/// adapter, while storage users still receive the original block entity. No
/// world lookup is used to choose which entity receives an open or close.
pub struct ContainerMenuInventory<T> {
    inner: Arc<T>,
    world: Arc<World>,
}

impl<T: ViewedContainer + 'static> ContainerMenuInventory<T> {
    pub(crate) fn new(inner: Arc<T>, world: Arc<World>) -> Arc<Self> {
        Arc::new(Self { inner, world })
    }
}

impl<T: ViewedContainer + 'static> Inventory for ContainerMenuInventory<T> {
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
        self.inner
            .viewers()
            .change_in_world(self.inner.as_ref(), &self.world, true);
    }
    fn on_close(&self) {
        self.inner
            .viewers()
            .change_in_world(self.inner.as_ref(), &self.world, false);
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
        // Transparent identity: direct and double-half recounts downcast to the
        // retained entity, including when its old position has a replacement.
        Inventory::as_any(self.inner.as_ref())
    }
}

impl<T: ViewedContainer> Clearable for ContainerMenuInventory<T> {
    fn clear(&self) {
        self.inner.clear();
    }
}

/// The pinned `DoubleInventory` exposes no half accessors. Keep its original
/// inventories alive and delegate operations to it; recounts need exact identity,
/// including when a menu outlives replacement of a block at the same position.
pub struct DoubleContainerInventory {
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
