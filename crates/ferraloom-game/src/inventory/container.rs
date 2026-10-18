use super::{
    ItemStack, MenuError, Result, StackGenerations, count, is_same_item_same_components,
    max_stack_size, set_count, split,
};

/// Port of Container. The owner supplies live storage; menus never snapshot it.
pub trait Container {
    fn stack_parts(&self, _slot: usize) -> Option<(&StackGenerations, &ItemStack, usize)> {
        None
    }
    fn stack_parts_mut(
        &mut self,
        _slot: usize,
    ) -> Option<(&mut StackGenerations, &mut ItemStack, usize)> {
        None
    }
    fn size(&self) -> usize;
    fn get_item(&self, slot: usize) -> ItemStack;
    fn set_item(&mut self, slot: usize, stack: ItemStack);
    /// ItemStack#setCount on the existing receiver. Slot#setChanged is a
    /// separate operation; this must not invoke Container#setItem callbacks.
    fn set_item_count(&mut self, _slot: usize, _amount: i32) -> Result<()> {
        Err(MenuError::Unsupported("direct container stack mutation"))
    }
    fn set_changed(&mut self) {}
    fn max_stack_size(&self) -> i32 {
        99
    }
    fn still_valid(&self) -> bool {
        true
    }
    fn can_place_item(&self, _slot: usize, _stack: &ItemStack) -> bool {
        true
    }
    fn can_take_item(&self, _slot: usize, _stack: &ItemStack) -> bool {
        true
    }
    fn start_open(&mut self) {}
    fn stop_open(&mut self) {}
    fn count_item(&self, item: ferraloom_data::Item) -> i32 {
        (0..self.size())
            .map(|slot| self.get_item(slot))
            .filter(|stack| stack.0.as_ref().is_some_and(|s| s.item == item))
            .map(|stack| count(&stack))
            .sum()
    }
    fn has_any_of(&self, items: &[ferraloom_data::Item]) -> bool {
        (0..self.size()).any(|slot| {
            self.get_item(slot)
                .0
                .as_ref()
                .is_some_and(|s| s.count > 0 && items.contains(&s.item))
        })
    }
    fn is_empty(&self) -> bool {
        (0..self.size()).all(|i| self.get_item(i).is_empty())
    }
    fn remove_item(&mut self, slot: usize, amount: i32) -> ItemStack {
        let mut stack = self.get_item(slot);
        let result = split(&mut stack, amount);
        if !result.is_empty() {
            self.set_item(slot, stack);
        }
        result
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack;
    fn clear_content(&mut self) {
        for slot in 0..self.size() {
            self.remove_item_no_update(slot);
        }
        self.set_changed();
    }
}

/// Port of SimpleContainer. The counter exposes change notifications to its owner.
#[derive(Clone, Debug)]
pub struct SimpleContainer {
    items: Vec<ItemStack>,
    stack_generations: StackGenerations,
    pub times_changed: u32,
}
impl SimpleContainer {
    pub fn new(size: usize) -> Self {
        Self {
            items: vec![ItemStack::default(); size],
            stack_generations: StackGenerations::new(size),
            times_changed: 0,
        }
    }
    pub fn from_items(items: Vec<ItemStack>) -> Self {
        Self {
            stack_generations: StackGenerations::new(items.len()),
            items,
            times_changed: 0,
        }
    }
    pub fn items(&self) -> &[ItemStack] {
        &self.items
    }
    /// Port of SimpleContainer#addItem, including its single empty-slot insertion.
    pub fn add_item(&mut self, stack: &ItemStack) -> ItemStack {
        let mut remaining = stack.clone();
        if remaining.is_empty() {
            return remaining;
        }
        for slot in 0..self.size() {
            let target = self.get_item(slot);
            if is_same_item_same_components(&target, &remaining) {
                let n = count(&remaining)
                    .min(max_stack_size(&target).min(self.max_stack_size()) - count(&target));
                if n > 0 {
                    let total = count(&target) + n;
                    set_count(&mut self.items[slot], total);
                    let left = count(&remaining) - n;
                    set_count(&mut remaining, left);
                    self.set_changed();
                }
                if remaining.is_empty() {
                    return remaining;
                }
            }
        }
        for slot in 0..self.size() {
            if self.get_item(slot).is_empty() {
                self.set_item(slot, remaining);
                return ItemStack::default();
            }
        }
        remaining
    }
    pub fn can_add_item(&self, stack: &ItemStack) -> bool {
        self.items.iter().any(|target| {
            target.is_empty()
                || is_same_item_same_components(target, stack)
                    && count(target) < max_stack_size(target)
        })
    }
    pub fn remove_all_items(&mut self) -> Vec<ItemStack> {
        let removed = (0..self.size())
            .map(|slot| self.remove_item_no_update(slot))
            .filter(|stack| !stack.is_empty())
            .collect();
        self.clear_content();
        removed
    }
}
impl Container for SimpleContainer {
    fn stack_parts(&self, slot: usize) -> Option<(&StackGenerations, &ItemStack, usize)> {
        Some((&self.stack_generations, self.items.get(slot)?, slot))
    }
    fn stack_parts_mut(
        &mut self,
        slot: usize,
    ) -> Option<(&mut StackGenerations, &mut ItemStack, usize)> {
        Some((&mut self.stack_generations, self.items.get_mut(slot)?, slot))
    }
    fn remove_item(&mut self, slot: usize, amount: i32) -> ItemStack {
        if amount <= 0 {
            return ItemStack::default();
        }
        let result = split(&mut self.items[slot], amount);
        if !result.is_empty() {
            self.set_changed();
        }
        result
    }
    fn clear_content(&mut self) {
        for slot in 0..self.items.len() {
            self.stack_generations
                .replace(slot, &mut self.items[slot], ItemStack::default());
        }
        self.set_changed();
    }
    fn set_item_count(&mut self, slot: usize, amount: i32) -> Result<()> {
        set_count(&mut self.items[slot], amount);
        Ok(())
    }
    fn size(&self) -> usize {
        self.items.len()
    }
    fn get_item(&self, slot: usize) -> ItemStack {
        self.items.get(slot).cloned().unwrap_or_default()
    }
    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        let n = count(&stack).min(self.max_stack_size().min(max_stack_size(&stack)));
        set_count(&mut stack, n);
        self.stack_generations
            .replace(slot, &mut self.items[slot], stack);
        self.set_changed();
    }
    fn set_changed(&mut self) {
        self.times_changed = self.times_changed.wrapping_add(1);
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        if self.items[slot].is_empty() {
            return ItemStack::default();
        }
        self.stack_generations
            .replace(slot, &mut self.items[slot], ItemStack::default())
            .into_owned()
            .expect("live value removal requires an unrooted stack")
    }
}

/// A short borrow of Player#getInventory, including LivingEntity's canonical equipment.
pub struct PlayerContainer<'a> {
    pub inventory: &'a mut crate::entity::player::inventory::Inventory,
    pub equipment: &'a mut crate::entity::player::equipment::EntityEquipment,
}
impl Container for PlayerContainer<'_> {
    fn stack_parts(&self, slot: usize) -> Option<(&StackGenerations, &ItemStack, usize)> {
        self.inventory.stack_parts(slot, self.equipment)
    }
    fn stack_parts_mut(
        &mut self,
        slot: usize,
    ) -> Option<(&mut StackGenerations, &mut ItemStack, usize)> {
        self.inventory.stack_parts_mut(slot, self.equipment)
    }
    fn set_item_count(&mut self, slot: usize, amount: i32) -> Result<()> {
        set_count(self.inventory.get_item_mut(slot, self.equipment), amount);
        Ok(())
    }
    fn size(&self) -> usize {
        43
    }
    fn get_item(&self, slot: usize) -> ItemStack {
        self.inventory.get_item(slot, self.equipment).clone()
    }
    fn set_item(&mut self, slot: usize, stack: ItemStack) {
        self.inventory.set_item(slot, stack, self.equipment);
    }
    fn set_changed(&mut self) {
        self.inventory.set_changed();
    }
    fn remove_item(&mut self, slot: usize, amount: i32) -> ItemStack {
        self.inventory.remove_item(slot, amount, self.equipment)
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        self.inventory.remove_item_no_update(slot, self.equipment)
    }
}

impl SimpleContainer {
    /// Port of SimpleContainer#removeItemType, scanning backwards and comparing only item type.
    pub fn remove_item_type(&mut self, item: ferraloom_data::Item, amount: i32) -> ItemStack {
        let mut removed = ItemStack::new(item, 1, Default::default());
        set_count(&mut removed, 0);
        for slot in (0..self.size()).rev() {
            if self.items[slot].0.as_ref().is_some_and(|s| s.item == item) {
                let part = split(&mut self.items[slot], amount - count(&removed));
                let total = count(&removed) + count(&part);
                set_count(&mut removed, total);
                if count(&removed) == amount {
                    break;
                }
            }
        }
        if !removed.is_empty() {
            self.set_changed();
        }
        removed
    }
}

/// Port of ResultContainer's storage semantics. Every index aliases its one
/// result, and removeItem takes the entire result regardless of the requested count.
#[derive(Clone, Debug)]
pub struct ResultContainer {
    stack: ItemStack,
    stack_generations: StackGenerations,
}
impl Default for ResultContainer {
    fn default() -> Self {
        Self {
            stack: ItemStack::default(),
            stack_generations: StackGenerations::new(1),
        }
    }
}
impl Container for ResultContainer {
    fn stack_parts(&self, _slot: usize) -> Option<(&StackGenerations, &ItemStack, usize)> {
        Some((&self.stack_generations, &self.stack, 0))
    }
    fn stack_parts_mut(
        &mut self,
        _slot: usize,
    ) -> Option<(&mut StackGenerations, &mut ItemStack, usize)> {
        Some((&mut self.stack_generations, &mut self.stack, 0))
    }
    fn set_item_count(&mut self, _slot: usize, amount: i32) -> Result<()> {
        set_count(&mut self.stack, amount);
        Ok(())
    }
    fn size(&self) -> usize {
        1
    }
    fn get_item(&self, _slot: usize) -> ItemStack {
        self.stack.clone()
    }
    fn set_item(&mut self, _slot: usize, stack: ItemStack) {
        self.stack_generations.replace(0, &mut self.stack, stack);
    }
    fn remove_item(&mut self, _slot: usize, _amount: i32) -> ItemStack {
        self.remove_item_no_update(0)
    }
    fn remove_item_no_update(&mut self, _slot: usize) -> ItemStack {
        if self.stack.is_empty() {
            return ItemStack::default();
        }
        self.stack_generations
            .replace(0, &mut self.stack, ItemStack::default())
            .into_owned()
            .expect("live value removal requires an unrooted stack")
    }
    fn clear_content(&mut self) {
        self.stack_generations
            .replace(0, &mut self.stack, ItemStack::default());
    }
}

/// Port of TransientCraftingContainer storage. The menu dispatches slotsChanged
/// immediately after set/remove; setChanged and clearContent have no callback.
#[derive(Clone, Debug)]
pub struct TransientCraftingContainer {
    items: Vec<ItemStack>,
    stack_generations: StackGenerations,
}
impl TransientCraftingContainer {
    pub fn new(size: usize) -> Self {
        Self {
            items: vec![ItemStack::default(); size],
            stack_generations: StackGenerations::new(size),
        }
    }
    pub fn items(&self) -> &[ItemStack] {
        &self.items
    }
    pub fn grow_item(&mut self, index: usize, amount: i32) {
        let total = count(&self.items[index]).wrapping_add(amount);
        set_count(&mut self.items[index], total);
    }
}
impl Container for TransientCraftingContainer {
    fn stack_parts(&self, slot: usize) -> Option<(&StackGenerations, &ItemStack, usize)> {
        Some((&self.stack_generations, self.items.get(slot)?, slot))
    }
    fn stack_parts_mut(
        &mut self,
        slot: usize,
    ) -> Option<(&mut StackGenerations, &mut ItemStack, usize)> {
        Some((&mut self.stack_generations, self.items.get_mut(slot)?, slot))
    }
    fn set_item_count(&mut self, slot: usize, amount: i32) -> Result<()> {
        set_count(&mut self.items[slot], amount);
        Ok(())
    }
    fn size(&self) -> usize {
        self.items.len()
    }
    fn get_item(&self, index: usize) -> ItemStack {
        self.items.get(index).cloned().unwrap_or_default()
    }
    fn set_item(&mut self, index: usize, stack: ItemStack) {
        self.stack_generations
            .replace(index, &mut self.items[index], stack);
    }
    fn remove_item(&mut self, index: usize, amount: i32) -> ItemStack {
        if amount <= 0 || self.items[index].is_empty() {
            ItemStack::default()
        } else {
            split(&mut self.items[index], amount)
        }
    }
    fn remove_item_no_update(&mut self, index: usize) -> ItemStack {
        if self.items[index].is_empty() {
            return ItemStack::default();
        }
        self.stack_generations
            .replace(index, &mut self.items[index], ItemStack::default())
            .into_owned()
            .expect("live value removal requires an unrooted stack")
    }
    fn clear_content(&mut self) {
        for index in 0..self.items.len() {
            self.stack_generations
                .replace(index, &mut self.items[index], ItemStack::default());
        }
    }
}

/// Canonical player-owned menu storage. Block inventories are borrowed separately.
#[derive(Clone, Debug)]
pub enum MenuContainer {
    Simple(SimpleContainer),
    Crafting(TransientCraftingContainer),
    Result(ResultContainer),
}
impl MenuContainer {
    pub fn crafting(size: usize) -> Self {
        Self::Crafting(TransientCraftingContainer::new(size))
    }
    pub fn simple(size: usize) -> Self {
        Self::Simple(SimpleContainer::new(size))
    }
}
impl Container for MenuContainer {
    fn stack_parts(&self, slot: usize) -> Option<(&StackGenerations, &ItemStack, usize)> {
        match self {
            Self::Simple(container) => container.stack_parts(slot),
            Self::Crafting(container) => container.stack_parts(slot),
            Self::Result(container) => container.stack_parts(slot),
        }
    }
    fn stack_parts_mut(
        &mut self,
        slot: usize,
    ) -> Option<(&mut StackGenerations, &mut ItemStack, usize)> {
        match self {
            Self::Simple(container) => container.stack_parts_mut(slot),
            Self::Crafting(container) => container.stack_parts_mut(slot),
            Self::Result(container) => container.stack_parts_mut(slot),
        }
    }
    fn set_item_count(&mut self, slot: usize, amount: i32) -> Result<()> {
        match self {
            Self::Simple(container) => container.set_item_count(slot, amount),
            Self::Crafting(container) => container.set_item_count(slot, amount),
            Self::Result(container) => container.set_item_count(slot, amount),
        }
    }
    fn size(&self) -> usize {
        match self {
            Self::Simple(c) => c.size(),
            Self::Crafting(c) => c.size(),
            Self::Result(c) => c.size(),
        }
    }
    fn get_item(&self, slot: usize) -> ItemStack {
        match self {
            Self::Simple(c) => c.get_item(slot),
            Self::Crafting(c) => c.get_item(slot),
            Self::Result(c) => c.get_item(slot),
        }
    }
    fn set_item(&mut self, slot: usize, stack: ItemStack) {
        match self {
            Self::Simple(c) => c.set_item(slot, stack),
            Self::Crafting(c) => c.set_item(slot, stack),
            Self::Result(c) => c.set_item(slot, stack),
        }
    }
    fn set_changed(&mut self) {
        match self {
            Self::Simple(c) => c.set_changed(),
            Self::Crafting(c) => c.set_changed(),
            Self::Result(c) => c.set_changed(),
        }
    }
    fn remove_item(&mut self, slot: usize, amount: i32) -> ItemStack {
        match self {
            Self::Simple(c) => c.remove_item(slot, amount),
            Self::Crafting(c) => c.remove_item(slot, amount),
            Self::Result(c) => c.remove_item(slot, amount),
        }
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        match self {
            Self::Simple(c) => c.remove_item_no_update(slot),
            Self::Crafting(c) => c.remove_item_no_update(slot),
            Self::Result(c) => c.remove_item_no_update(slot),
        }
    }
    fn clear_content(&mut self) {
        match self {
            Self::Simple(c) => c.clear_content(),
            Self::Crafting(c) => c.clear_content(),
            Self::Result(c) => c.clear_content(),
        }
    }
}
