//! Identity for held ItemStack receivers. Replacement detaches rooted objects;
//! direct count/component edits keep the current generation.
use super::{ContainerHandle, ItemStack};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct StackRef {
    pub container: ContainerHandle,
    pub slot: usize,
    pub generation: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct StackSlotRef {
    pub slot: usize,
    pub generation: u64,
}
impl StackRef {
    pub fn local(self) -> StackSlotRef {
        StackSlotRef {
            slot: self.slot,
            generation: self.generation,
        }
    }
}
#[derive(Debug)]
struct RetainedStack {
    roots: usize,
    detached: Option<ItemStack>,
}
/// One counter per slot; retention bookkeeping exists only while an operation
/// holds receivers. The containing inventory/menu/BE owns both live and detached data.
#[derive(Debug)]
pub struct StackGenerations {
    generations: Vec<u64>,
    retained: Option<Box<HashMap<StackSlotRef, RetainedStack>>>,
}
impl Clone for StackGenerations {
    /// A copied container belongs to a new owner identity. Operation roots and
    /// orphaned objects continue to belong solely to their original owner.
    fn clone(&self) -> Self {
        Self {
            generations: self.generations.clone(),
            retained: None,
        }
    }
}
impl StackGenerations {
    pub fn new(size: usize) -> Self {
        Self {
            generations: vec![1; size],
            retained: None,
        }
    }
    pub fn current(&self, slot: usize) -> Option<StackSlotRef> {
        Some(StackSlotRef {
            slot,
            generation: *self.generations.get(slot)?,
        })
    }
    pub fn retain(&mut self, reference: StackSlotRef) -> bool {
        let current = self.current(reference.slot) == Some(reference);
        let detached = self
            .retained
            .as_ref()
            .and_then(|rows| rows.get(&reference))
            .is_some_and(|row| row.detached.is_some());
        if !current && !detached {
            return false;
        }
        let row = self
            .retained
            .get_or_insert_with(|| Box::new(HashMap::new()))
            .entry(reference)
            .or_insert(RetainedStack {
                roots: 0,
                detached: None,
            });
        row.roots = row.roots.checked_add(1).expect("stack roots exhausted");
        true
    }
    pub fn release(&mut self, reference: StackSlotRef) {
        let Some(rows) = self.retained.as_mut() else {
            return;
        };
        if let Some(row) = rows.get_mut(&reference) {
            row.roots -= 1;
            if row.roots == 0 {
                rows.remove(&reference);
            }
        }
        if rows.is_empty() {
            self.retained = None;
        }
    }
    /// Container#setItem's object replacement. Install the new live object and
    /// generation before callers invoke any native callbacks.
    pub fn replace(&mut self, slot: usize, current: &mut ItemStack, replacement: ItemStack) {
        let reference = self.current(slot).expect("container stack slot");
        let next = reference
            .generation
            .checked_add(1)
            .expect("stack generations exhausted");
        let old = std::mem::replace(current, replacement);
        self.generations[slot] = next;
        if let Some(row) = self
            .retained
            .as_mut()
            .and_then(|rows| rows.get_mut(&reference))
        {
            row.detached = Some(old);
        }
    }
    pub fn get<'a>(
        &'a self,
        reference: StackSlotRef,
        current: &'a ItemStack,
    ) -> Option<&'a ItemStack> {
        if self.current(reference.slot) == Some(reference) {
            return Some(current);
        }
        self.retained.as_ref()?.get(&reference)?.detached.as_ref()
    }
    pub fn get_mut<'a>(
        &'a mut self,
        reference: StackSlotRef,
        current: &'a mut ItemStack,
    ) -> Option<&'a mut ItemStack> {
        if self.current(reference.slot) == Some(reference) {
            return Some(current);
        }
        self.retained
            .as_mut()?
            .get_mut(&reference)?
            .detached
            .as_mut()
    }
}
