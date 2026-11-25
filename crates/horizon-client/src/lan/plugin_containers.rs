//! Ferraloom: containers a plugin made (`Server.createInventory`), the
//! slots of a menu shown with `Slots::Custom`. They belong to no block: a
//! plugin's Java object holds one by its id and frees it when collected.
//! Server thread only; empty and untouched without plugins.
use crate::items::ItemStack;
use std::{cell::RefCell, collections::HashMap};

thread_local! {
    static CONTAINERS: RefCell<(u32, HashMap<u32, Vec<Option<ItemStack>>>)> =
        RefCell::new((0, HashMap::new()));
}

/// A new container of `size` empty slots: its id.
pub fn create(size: usize) -> u32 {
    CONTAINERS.with_borrow_mut(|(next, all)| {
        *next = next.wrapping_add(1).max(1);
        all.insert(*next, vec![None; size]);
        *next
    })
}

pub fn remove(id: u32) {
    CONTAINERS.with_borrow_mut(|(_, all)| all.remove(&id));
}

pub fn exists(id: u32) -> bool {
    CONTAINERS.with_borrow(|(_, all)| all.contains_key(&id))
}

pub fn size(id: u32) -> Option<usize> {
    CONTAINERS.with_borrow(|(_, all)| all.get(&id).map(Vec::len))
}

pub fn get(id: u32, slot: usize) -> Option<ItemStack> {
    CONTAINERS.with_borrow(|(_, all)| all.get(&id)?.get(slot).cloned().flatten())
}

/// Sets a slot; false when there is no such container or slot.
pub fn set(id: u32, slot: usize, item: Option<ItemStack>) -> bool {
    CONTAINERS.with_borrow_mut(
        |(_, all)| match all.get_mut(&id).and_then(|c| c.get_mut(slot)) {
            Some(s) => {
                *s = item;
                true
            }
            None => false,
        },
    )
}

pub fn read(id: u32) -> Option<Vec<Option<ItemStack>>> {
    CONTAINERS.with_borrow(|(_, all)| all.get(&id).cloned())
}

pub fn write(id: u32, items: Vec<Option<ItemStack>>) {
    CONTAINERS.with_borrow_mut(|(_, all)| {
        if let Some(c) = all.get_mut(&id) {
            *c = items;
        }
    });
}
