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
/// An owned new object or an existing Java ItemStack receiver. Assigning the
/// latter must preserve identity, including when it moves to another slot.
#[derive(Debug)]
pub enum StackValue {
    New(ItemStack),
    Existing(StackRef),
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
    forwarded: Option<StackRef>,
}
/// One counter per slot; retention bookkeeping exists only while an operation
/// holds receivers. The containing inventory/menu/BE owns both live and detached data.
#[derive(Debug)]
pub struct StackGenerations {
    generations: Vec<u64>,
    #[allow(
        clippy::box_collection,
        reason = "one optional pointer per container outside operations"
    )]
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
            .is_some_and(|row| row.detached.is_some() || row.forwarded.is_some());
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
                forwarded: None,
            });
        row.roots = row.roots.checked_add(1).expect("stack roots exhausted");
        true
    }
    /// Release the destination root as well when the final forwarding root
    /// disappears. The owner resolver follows that returned handle recursively.
    pub fn release(&mut self, reference: StackSlotRef) -> Option<StackRef> {
        let rows = self.retained.as_mut()?;
        let mut forwarded = None;
        if let Some(row) = rows.get_mut(&reference) {
            row.roots -= 1;
            if row.roots == 0 {
                forwarded = rows.remove(&reference).and_then(|row| row.forwarded);
            }
        }
        if rows.is_empty() {
            self.retained = None;
        }
        forwarded
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
    pub fn forwarded(&self, reference: StackSlotRef) -> Option<StackRef> {
        self.retained.as_ref()?.get(&reference)?.forwarded
    }
    /// Extract a rooted object during an atomic, callback-free move. The
    /// transaction must install it and bind its forwarding destination before
    /// invoking callbacks. No canonical receiver leaves storage across a callback.
    pub fn take_for_move(
        &mut self,
        reference: StackSlotRef,
        current: &mut ItemStack,
    ) -> Option<ItemStack> {
        let row = self.retained.as_ref()?.get(&reference)?;
        if row.forwarded.is_some() {
            return None;
        }
        if self.current(reference.slot) == Some(reference) {
            self.replace(reference.slot, current, ItemStack::default());
        }
        self.retained.as_mut()?.get_mut(&reference)?.detached.take()
    }
    /// The destination is already rooted. That root belongs to this forwarding
    /// row until its last holder releases it, even through further moves.
    pub fn forward_to(&mut self, reference: StackSlotRef, destination: StackRef) {
        let row = self
            .retained
            .as_mut()
            .and_then(|rows| rows.get_mut(&reference))
            .expect("moving a rooted stack");
        assert!(row.detached.is_none() && row.forwarded.is_none());
        row.forwarded = Some(destination);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        block::entity::BlockEntityRef,
        entity::player::inventory::{count, set_count},
    };
    use ferraloom_data::Item;
    use ferraloom_util::BlockPos;

    struct SwapFixture {
        slots: [ItemStack; 2],
        generations: StackGenerations,
    }
    impl SwapFixture {
        fn current(&self, slot: usize) -> StackRef {
            StackRef {
                container: ContainerHandle::BlockEntity(BlockEntityRef {
                    pos: BlockPos::new(0, 64, 0),
                    generation: 1,
                }),
                slot,
                generation: self.generations.current(slot).unwrap().generation,
            }
        }
        fn resolve(&self, mut reference: StackRef) -> StackRef {
            while let Some(next) = self.generations.forwarded(reference.local()) {
                reference = next;
            }
            reference
        }
        fn get(&self, reference: StackRef) -> &ItemStack {
            let reference = self.resolve(reference);
            self.generations
                .get(reference.local(), &self.slots[reference.slot])
                .unwrap()
        }
        fn set_count(&mut self, reference: StackRef, count: i32) {
            let reference = self.resolve(reference);
            set_count(
                self.generations
                    .get_mut(reference.local(), &mut self.slots[reference.slot])
                    .unwrap(),
                count,
            );
        }
        fn assign(&mut self, slot: usize, reference: StackRef) {
            let reference = self.resolve(reference);
            if self.current(slot) == reference {
                return;
            }
            let stack = self
                .generations
                .take_for_move(reference.local(), &mut self.slots[reference.slot])
                .unwrap();
            self.generations.replace(slot, &mut self.slots[slot], stack);
            let destination = self.current(slot);
            assert!(self.generations.retain(destination.local()));
            self.generations.forward_to(reference.local(), destination);
        }
        fn release(&mut self, reference: StackRef) {
            if let Some(destination) = self.generations.release(reference.local()) {
                self.release(destination);
            }
        }
        fn observe(&self, source: StackRef, target: StackRef) -> String {
            format!(
                "{}|{}|{}|{}",
                visible(&self.slots[0]),
                visible(&self.slots[1]),
                count(self.get(source)),
                count(self.get(target))
            )
        }
    }
    fn visible(stack: &ItemStack) -> String {
        format!("{}:{}", stack.0.as_ref().unwrap().item.name(), count(stack))
    }

    #[test]
    fn held_swap_refs_follow_moved_objects_and_later_replacement() {
        let mut checked = 0;
        for line in include_str!("../../tests/java/blocks/entities/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("menu-swap-source\t"))
        {
            let fields: Vec<_> = line.split('\t').collect();
            let mut fixture = SwapFixture {
                slots: [
                    ItemStack::new(Item::STONE, 10, Default::default()),
                    ItemStack::new(Item::DIAMOND, 7, Default::default()),
                ],
                generations: StackGenerations::new(2),
            };
            let source = fixture.current(0);
            let target = fixture.current(1);
            assert!(fixture.generations.retain(source.local()));
            assert!(fixture.generations.retain(target.local()));
            // Native inventory.setItem followed by target.setByPlayer, before
            // target's first setChanged. Both assignments are callback-free here.
            fixture.assign(0, target);
            fixture.assign(1, source);
            let assigned = fixture.current(1);
            fixture.assign(1, source);
            assert_eq!(fixture.current(1), assigned, "same receiver assignment");
            let first = fixture.observe(source, target);
            fixture.set_count(source, count(fixture.get(source)) - 2);
            fixture.set_count(target, count(fixture.get(target)) + 3);
            if fields[1] == "true" {
                fixture.generations.replace(
                    1,
                    &mut fixture.slots[1],
                    ItemStack::new(Item::DIRT, 5, Default::default()),
                );
                fixture.set_count(source, count(fixture.get(source)) - 3);
            }
            // Slot#setChanged and Slot#onTake observe the same held receivers.
            let second = fixture.observe(source, target);
            assert_eq!(format!("{first},{second},{second}"), fields[2], "{line}");
            assert_eq!(visible(&fixture.slots[0]), fields[3], "{line}");
            assert_eq!(visible(&fixture.slots[1]), fields[4], "{line}");
            assert_eq!(count(fixture.get(source)).to_string(), fields[5], "{line}");
            assert_eq!(count(fixture.get(target)).to_string(), fields[6], "{line}");
            fixture.release(source);
            fixture.release(target);
            assert!(fixture.generations.retained.is_none());
            checked += 1;
        }
        assert_eq!(checked, 2);
    }
}
