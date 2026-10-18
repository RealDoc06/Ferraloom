use super::{ItemStack, count, is_same_item_same_components, max_stack_size, set_count, split};

/// Port of Container. The owner supplies live storage; menus never snapshot it.
pub trait Container {
    fn size(&self) -> usize;
    fn get_item(&self, slot: usize) -> ItemStack;
    fn set_item(&mut self, slot: usize, stack: ItemStack);
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
    pub times_changed: u32,
}
impl SimpleContainer {
    pub fn new(size: usize) -> Self {
        Self {
            items: vec![ItemStack::default(); size],
            times_changed: 0,
        }
    }
    pub fn from_items(items: Vec<ItemStack>) -> Self {
        Self {
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
            let mut target = self.get_item(slot);
            if is_same_item_same_components(&target, &remaining) {
                let n = count(&remaining)
                    .min(max_stack_size(&target).min(self.max_stack_size()) - count(&target));
                if n > 0 {
                    let total = count(&target) + n;
                    set_count(&mut target, total);
                    let left = count(&remaining) - n;
                    set_count(&mut remaining, left);
                    self.set_item(slot, target);
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
        let removed = self
            .items
            .iter_mut()
            .map(std::mem::take)
            .filter(|s| !s.is_empty())
            .collect();
        self.set_changed();
        removed
    }
}
impl Container for SimpleContainer {
    fn size(&self) -> usize {
        self.items.len()
    }
    fn get_item(&self, slot: usize) -> ItemStack {
        self.items.get(slot).cloned().unwrap_or_default()
    }
    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        let n = count(&stack).min(self.max_stack_size().min(max_stack_size(&stack)));
        set_count(&mut stack, n);
        self.items[slot] = stack;
        self.set_changed();
    }
    fn set_changed(&mut self) {
        self.times_changed = self.times_changed.wrapping_add(1);
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        std::mem::take(&mut self.items[slot])
    }
}

/// A short borrow of Player#getInventory, including LivingEntity's canonical equipment.
pub struct PlayerContainer<'a> {
    pub inventory: &'a mut crate::entity::player::inventory::Inventory,
    pub equipment: &'a mut crate::entity::player::equipment::EntityEquipment,
}
impl Container for PlayerContainer<'_> {
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
