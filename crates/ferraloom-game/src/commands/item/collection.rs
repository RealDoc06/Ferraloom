use ferraloom_protocol::component::ItemStack;

/// Adapter to canonical SlotAccess/container operations. Replacement values are
/// owned copies, installed through StackOperation by the runtime adapter.
pub trait SlotOperation<H> {
    type Error;
    fn get(&mut self, handle: &H) -> Result<ItemStack, Self::Error>;
    fn set(&mut self, handle: &H, value: ItemStack) -> Result<bool, Self::Error>;
}

/// Port of SlotCollection#of(Collection). Preserve handle order and duplicates;
/// never prefetch stack values across preceding slot mutations.
#[derive(Clone, Debug)]
pub struct SlotCollection<H> {
    slots: Vec<H>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlotUpdate {
    pub selected: i32,
    pub updated: i32,
}
impl<H> SlotCollection<H> {
    pub fn new(slots: Vec<H>) -> Self {
        Self { slots }
    }
    pub fn size(&self) -> usize {
        self.slots.len()
    }
    /// Port of SlotCollection#itemCopies. Empty stacks are retained.
    pub fn item_copies<O: SlotOperation<H>>(
        &self,
        operation: &mut O,
    ) -> Result<Vec<ItemStack>, O::Error> {
        self.slots
            .iter()
            .map(|slot| operation.get(slot).map(|value| copy_stack(&value)))
            .collect()
    }
    /// Port of replaceSlotItems with ArrayItemProvider and tracking(ANY_SLOT).
    /// The provider consumes a value even when the slot rejects the replacement.
    pub fn replace<O: SlotOperation<H>>(
        &self,
        items: &[ItemStack],
        operation: &mut O,
    ) -> Result<SlotUpdate, O::Error> {
        let mut result = SlotUpdate::default();
        for (slot, item) in self.slots.iter().zip(items) {
            operation.get(slot)?;
            result.selected += 1;
            if operation.set(slot, copy_stack(item))? {
                result.updated += 1;
            }
        }
        Ok(result)
    }
    /// Port of modifySlots with tracking(NON_EMPTY_SLOTS) and CountingModifier.
    /// CountingModifier obtains its own copy after the selector's separate read,
    /// and calls set even when the modifier produces an equal value.
    pub fn modify<O: SlotOperation<H>>(
        &self,
        operation: &mut O,
        mut modifier: impl FnMut(ItemStack) -> Result<ItemStack, O::Error>,
    ) -> Result<SlotUpdate, O::Error> {
        let mut result = SlotUpdate::default();
        for slot in &self.slots {
            if operation.get(slot)?.is_empty() {
                continue;
            }
            result.selected += 1;
            let value = modifier(copy_stack(&operation.get(slot)?))?;
            if operation.set(slot, value)? {
                result.updated += 1;
            }
        }
        Ok(result)
    }
}

fn copy_stack(value: &ItemStack) -> ItemStack {
    if value.is_empty() {
        ItemStack::default()
    } else {
        value.clone()
    }
}

#[cfg(test)]
mod tests;
