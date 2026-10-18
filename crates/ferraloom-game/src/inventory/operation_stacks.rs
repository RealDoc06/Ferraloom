//! Fresh Java ItemStack receivers held by a menu operation before installation.
use super::{ItemStack, StackGenerations, StackRef, StackSlotRef};

#[derive(Debug)]
pub(crate) struct OperationStackArena {
    items: Vec<ItemStack>,
    generations: StackGenerations,
    active: Vec<bool>,
    free: Vec<usize>,
}
impl Default for OperationStackArena {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            generations: StackGenerations::new(0),
            active: Vec::new(),
            free: Vec::new(),
        }
    }
}
impl OperationStackArena {
    /// The initial root belongs to the operation that created the object.
    pub(crate) fn insert(&mut self, item: ItemStack) -> StackSlotRef {
        let slot = if let Some(slot) = self.free.pop() {
            self.generations.replace(slot, &mut self.items[slot], item);
            self.active[slot] = true;
            slot
        } else {
            let slot = self.generations.append_slot();
            self.items.push(item);
            self.active.push(true);
            slot
        };
        let reference = self.generations.current(slot).expect("new operation slot");
        assert!(self.generations.retain(reference));
        reference
    }
    pub(crate) fn parts(&self, slot: usize) -> Option<(&StackGenerations, &ItemStack, usize)> {
        self.active.get(slot).copied().filter(|active| *active)?;
        Some((&self.generations, &self.items[slot], slot))
    }
    pub(crate) fn parts_mut(
        &mut self,
        slot: usize,
    ) -> Option<(&mut StackGenerations, &mut ItemStack, usize)> {
        self.active.get(slot).copied().filter(|active| *active)?;
        Some((&mut self.generations, &mut self.items[slot], slot))
    }
    pub(crate) fn release(&mut self, reference: StackSlotRef) -> Option<StackRef> {
        if !self.active.get(reference.slot).copied().unwrap_or(false) {
            return None;
        }
        let forwarded = self.generations.release(reference);
        if !self.generations.retains_slot(reference.slot) {
            self.active[reference.slot] = false;
            self.items[reference.slot] = ItemStack::default();
            self.free.push(reference.slot);
        }
        forwarded
    }
}
