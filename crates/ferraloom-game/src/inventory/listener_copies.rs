//! Native lastSlots/currentCopy objects. Menu entries and active callbacks may
//! alias the same copy; this arena owns each object exactly once.
use super::ItemStack;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ListenerCopyRef {
    pub(crate) slot: usize,
    pub(crate) generation: u64,
}

#[derive(Debug)]
struct ListenerCopy {
    generation: u64,
    roots: usize,
    stack: Option<ItemStack>,
}

/// Port of the objects retained by AbstractContainerMenu.lastSlots and its
/// memoized currentCopy suppliers. Slot replacement never changes these objects.
#[derive(Debug)]
pub struct ListenerCopyArena {
    objects: Vec<ListenerCopy>,
    free: Vec<usize>,
}
impl Default for ListenerCopyArena {
    fn default() -> Self {
        Self {
            // ItemStack#copy returns the shared EMPTY object for empty stacks.
            objects: vec![ListenerCopy {
                generation: 1,
                roots: 0,
                stack: Some(ItemStack::default()),
            }],
            free: Vec::new(),
        }
    }
}
impl ListenerCopyArena {
    /// Bind a standalone adapter's objects into its player's canonical arena.
    /// This callback-free ownership move preserves aliases between its entries.
    pub(crate) fn import(&mut self, mut source: Self, references: &mut [Option<ListenerCopyRef>]) {
        let mut moved = std::collections::HashMap::new();
        for reference in references.iter_mut().flatten() {
            let old = *reference;
            *reference = if let Some(&new) = moved.get(&old) {
                assert!(self.retain(new));
                new
            } else {
                let stack = if old.slot == 0 {
                    ItemStack::default()
                } else {
                    source.objects[old.slot]
                        .stack
                        .take()
                        .expect("standalone listener object")
                };
                let new = self.insert(stack);
                moved.insert(old, new);
                new
            };
        }
    }
    /// Allocate one native copy with its initial ownership root.
    pub fn insert(&mut self, stack: ItemStack) -> ListenerCopyRef {
        if stack.is_empty() {
            let reference = self.reference(0).expect("shared empty listener object");
            assert!(self.retain(reference));
            return reference;
        }
        let slot = if let Some(slot) = self.free.pop() {
            let object = &mut self.objects[slot];
            object.generation = object
                .generation
                .checked_add(1)
                .expect("listener copy generations exhausted");
            object.roots = 1;
            object.stack = Some(stack);
            slot
        } else {
            let slot = self.objects.len();
            self.objects.push(ListenerCopy {
                generation: 1,
                roots: 1,
                stack: Some(stack),
            });
            slot
        };
        self.reference(slot).expect("new listener copy")
    }
    pub(crate) fn reference(&self, slot: usize) -> Option<ListenerCopyRef> {
        let object = self.objects.get(slot)?;
        object.stack.as_ref()?;
        Some(ListenerCopyRef {
            slot,
            generation: object.generation,
        })
    }
    pub fn get(&self, reference: ListenerCopyRef) -> Option<&ItemStack> {
        let object = self.objects.get(reference.slot)?;
        (object.generation == reference.generation)
            .then_some(object.stack.as_ref())
            .flatten()
    }
    pub fn get_mut(&mut self, reference: ListenerCopyRef) -> Option<&mut ItemStack> {
        let object = self.objects.get_mut(reference.slot)?;
        (object.generation == reference.generation)
            .then_some(object.stack.as_mut())
            .flatten()
    }
    pub fn retain(&mut self, reference: ListenerCopyRef) -> bool {
        let Some(object) = self.objects.get_mut(reference.slot) else {
            return false;
        };
        if object.generation != reference.generation || object.stack.is_none() {
            return false;
        }
        object.roots = object
            .roots
            .checked_add(1)
            .expect("listener roots exhausted");
        true
    }
    pub fn release(&mut self, reference: ListenerCopyRef) {
        let Some(object) = self.objects.get_mut(reference.slot) else {
            return;
        };
        if object.generation != reference.generation || object.roots == 0 {
            return;
        }
        object.roots -= 1;
        if object.roots == 0 && reference.slot != 0 {
            object.stack = None;
            self.free.push(reference.slot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::player::inventory::{count, max_stack_size, set_count};
    use ferraloom_data::Item;

    fn describe(arena: &ListenerCopyArena, reference: ListenerCopyRef) -> String {
        let stack = arena.get(reference).unwrap();
        format!(
            "{},{},{}",
            stack.item().id(),
            count(stack),
            max_stack_size(stack)
        )
    }

    #[test]
    fn transfer_state_shares_native_listener_objects_until_the_final_root_retires() {
        let mut cases = 0;
        for line in include_str!("../../tests/java/inventory/listeners.tsv")
            .lines()
            .filter(|line| line.starts_with("listener\talias-"))
        {
            let fields: Vec<_> = line.split('\t').collect();
            let mut arena = ListenerCopyArena::default();
            let mut source = arena.insert(ItemStack::new(Item::STONE, 5, Default::default()));
            let held = source;
            assert!(arena.retain(held)); // In-flight callback.
            let mut target = source;
            assert!(arena.retain(target)); // transferState's second menu owner.
            let mut events = vec![format!("alias:{}", held == target)];
            set_count(arena.get_mut(held).unwrap(), 8);
            events.push(format!(
                "shared:{}:{}",
                describe(&arena, source),
                describe(&arena, target)
            ));
            if fields[1] == "alias-source" {
                arena.release(source);
                source = arena.insert(ItemStack::new(Item::DIRT, 2, Default::default()));
            } else {
                arena.release(target);
                target = arena.insert(ItemStack::new(Item::STONE, 5, Default::default()));
            }
            events.push(format!("owners:{}:{}", held == source, held == target));
            set_count(arena.get_mut(held).unwrap(), 9);
            events.push(format!("held:{}", describe(&arena, held)));
            events.push(format!(
                "final:{}:{}",
                describe(&arena, source),
                describe(&arena, target)
            ));
            assert_eq!(events.join(";"), fields[2], "{}", fields[1]);
            arena.release(source);
            arena.release(target);
            assert!(arena.get(held).is_some());
            arena.release(held);
            assert!(arena.get(held).is_none());
            let replacement = arena.insert(ItemStack::new(Item::STONE, 1, Default::default()));
            assert_ne!(replacement, held);
            assert!(!arena.retain(held));
            cases += 1;
        }
        assert_eq!(cases, 2);
    }
}
