//! Port of Player's Inventory and the ItemStack operations used by containers.
use super::equipment::EntityEquipment;
use ferraloom_data::registries::data_component_type::Id as ComponentType;
use ferraloom_nbt::{ListTag, Tag};
use ferraloom_protocol::{
    Result,
    component::{DataComponent, DataComponentMap, EquipmentSlot, ItemStack, Persistent},
};

/// Access a stack's effective component without cloning its immutable prototype.
pub fn component(stack: &ItemStack, key: ComponentType) -> Option<&DataComponent> {
    let stack = stack.0.as_ref()?;
    if let Some(value) = stack
        .components
        .added
        .iter()
        .rev()
        .find(|v| v.component_type() == key)
    {
        return Some(value);
    }
    if stack.components.removed.contains(&key) {
        return None;
    }
    DataComponentMap::item_defaults(stack.item)
        .expect("generated vanilla item components")
        .get(key)
}
pub fn count(stack: &ItemStack) -> i32 {
    if stack.is_empty() {
        0
    } else {
        stack.0.as_ref().unwrap().count
    }
}
/// Keep the payload when its count becomes zero: vanilla mutable stack references
/// can subsequently restore that count (notably ItemEntity#playerTouch).
pub fn set_count(stack: &mut ItemStack, count: i32) {
    if let Some(value) = &mut stack.0 {
        value.count = count;
    }
}
pub fn copy_with_count(stack: &ItemStack, amount: i32) -> ItemStack {
    if stack.is_empty() {
        return ItemStack::default();
    }
    let mut copy = stack.clone();
    set_count(&mut copy, amount);
    copy
}
pub fn split(stack: &mut ItemStack, amount: i32) -> ItemStack {
    let amount = amount.min(count(stack));
    let result = copy_with_count(stack, amount);
    set_count(stack, count(stack).wrapping_sub(amount));
    result
}
pub fn max_stack_size(stack: &ItemStack) -> i32 {
    match component(stack, ComponentType::MAX_STACK_SIZE) {
        Some(DataComponent::MaxStackSize(v)) => v.0,
        _ => 1,
    }
}
pub fn is_damaged(stack: &ItemStack) -> bool {
    if component(stack, ComponentType::MAX_DAMAGE).is_none()
        || component(stack, ComponentType::UNBREAKABLE).is_some()
    {
        return false;
    }
    matches!(component(stack, ComponentType::DAMAGE), Some(DataComponent::Damage(v)) if v.0 > 0)
}
pub fn is_stackable(stack: &ItemStack) -> bool {
    max_stack_size(stack) > 1 && !is_damaged(stack)
}
pub fn is_enchanted(stack: &ItemStack) -> bool {
    matches!(component(stack, ComponentType::ENCHANTMENTS), Some(DataComponent::Enchantments(v)) if !v.0.is_empty())
}
pub fn is_same_item_same_components(a: &ItemStack, b: &ItemStack) -> bool {
    if a.is_empty() || b.is_empty() {
        return a.is_empty() && b.is_empty();
    }
    if a.0.as_ref().unwrap().item != b.0.as_ref().unwrap().item {
        return false;
    }
    let a = a.components().expect("canonical item components");
    let b = b.components().expect("canonical item components");
    a.entries.len() == b.entries.len()
        && a.entries
            .iter()
            .all(|value| b.get(value.component_type()) == Some(value))
}
/// Inventory's 26.3 mapping includes body armor and saddle after offhand.
pub fn equipment_slot(slot: usize) -> Option<EquipmentSlot> {
    match slot {
        36 => Some(EquipmentSlot::Feet),
        37 => Some(EquipmentSlot::Legs),
        38 => Some(EquipmentSlot::Chest),
        39 => Some(EquipmentSlot::Head),
        40 => Some(EquipmentSlot::Offhand),
        41 => Some(EquipmentSlot::Body),
        42 => Some(EquipmentSlot::Saddle),
        _ => None,
    }
}
#[derive(Clone, Debug)]
pub struct Inventory {
    pub items: [ItemStack; 36],
    selected: usize,
    times_changed: i32,
    // ItemStack's client pop animation is transient, absent from its wire/NBT
    // value. Keep that per-slot transient state alongside the canonical stacks.
    pop_times: [i32; 43],
}
impl Default for Inventory {
    fn default() -> Self {
        Self {
            items: std::array::from_fn(|_| ItemStack::default()),
            selected: 0,
            times_changed: 0,
            pop_times: [0; 43],
        }
    }
}
/// Effects of placeItemBackInInventory are emitted in the native loop order.
#[derive(Debug, PartialEq)]
pub enum InventoryEffect {
    SetSlot { slot: usize, stack: ItemStack },
    Drop(ItemStack),
}
impl Inventory {
    pub fn selected_slot(&self) -> usize {
        self.selected
    }
    pub fn set_selected_slot(&mut self, selected: usize) {
        assert!(selected < 9, "invalid selected slot");
        self.selected = selected;
    }
    pub fn selected_item(&self) -> &ItemStack {
        &self.items[self.selected]
    }
    pub fn selected_item_mut(&mut self) -> &mut ItemStack {
        &mut self.items[self.selected]
    }
    pub fn set_selected_item(&mut self, stack: ItemStack) -> ItemStack {
        std::mem::replace(&mut self.items[self.selected], stack)
    }
    pub fn get_item<'a>(&'a self, slot: usize, equipment: &'a EntityEquipment) -> &'a ItemStack {
        if slot < 36 {
            &self.items[slot]
        } else if let Some(slot) = equipment_slot(slot) {
            equipment.get(slot)
        } else {
            static EMPTY: std::sync::OnceLock<ItemStack> = std::sync::OnceLock::new();
            EMPTY.get_or_init(ItemStack::default)
        }
    }
    /// Only valid canonical slots may be mutably borrowed.
    pub fn get_item_mut<'a>(
        &'a mut self,
        slot: usize,
        equipment: &'a mut EntityEquipment,
    ) -> &'a mut ItemStack {
        if slot < 36 {
            &mut self.items[slot]
        } else {
            equipment.get_mut(equipment_slot(slot).expect("inventory slot"))
        }
    }
    pub fn set_item(
        &mut self,
        slot: usize,
        stack: ItemStack,
        equipment: &mut EntityEquipment,
    ) -> ItemStack {
        if slot < 36 {
            std::mem::replace(&mut self.items[slot], stack)
        } else if let Some(slot) = equipment_slot(slot) {
            equipment.set(slot, stack)
        } else {
            ItemStack::default()
        }
    }
    pub fn remove_item(
        &mut self,
        slot: usize,
        amount: i32,
        equipment: &mut EntityEquipment,
    ) -> ItemStack {
        if slot < 36 || equipment_slot(slot).is_some() {
            split(self.get_item_mut(slot, equipment), amount)
        } else {
            ItemStack::default()
        }
    }
    pub fn remove_item_no_update(
        &mut self,
        slot: usize,
        equipment: &mut EntityEquipment,
    ) -> ItemStack {
        self.set_item(slot, ItemStack::default(), equipment)
    }
    pub fn get_free_slot(&self) -> Option<usize> {
        self.items.iter().position(ItemStack::is_empty)
    }
    pub fn find_slot_matching_item(&self, stack: &ItemStack) -> Option<usize> {
        self.items
            .iter()
            .position(|s| !s.is_empty() && is_same_item_same_components(s, stack))
    }
    fn has_remaining_space(&self, slot: &ItemStack, stack: &ItemStack) -> bool {
        !slot.is_empty()
            && is_same_item_same_components(slot, stack)
            && is_stackable(slot)
            && count(slot) < max_stack_size(slot).min(99)
    }
    pub fn get_slot_with_remaining_space(
        &self,
        stack: &ItemStack,
        equipment: &EntityEquipment,
    ) -> Option<usize> {
        if self.has_remaining_space(self.get_item(self.selected, equipment), stack) {
            return Some(self.selected);
        }
        if self.has_remaining_space(self.get_item(40, equipment), stack) {
            return Some(40);
        }
        self.items
            .iter()
            .position(|s| self.has_remaining_space(s, stack))
    }
    pub fn get_suitable_hotbar_slot(&self) -> usize {
        for offset in 0..9 {
            let slot = (self.selected + offset) % 9;
            if self.items[slot].is_empty() {
                return slot;
            }
        }
        for offset in 0..9 {
            let slot = (self.selected + offset) % 9;
            if !is_enchanted(&self.items[slot]) {
                return slot;
            }
        }
        self.selected
    }
    pub fn pick_slot(&mut self, slot: usize) {
        self.selected = self.get_suitable_hotbar_slot();
        self.items.swap(self.selected, slot);
    }
    pub fn add_and_pick_item(&mut self, stack: ItemStack) {
        self.selected = self.get_suitable_hotbar_slot();
        if !self.items[self.selected].is_empty()
            && let Some(free) = self.get_free_slot()
        {
            self.items[free] = self.items[self.selected].clone();
        }
        self.items[self.selected] = stack;
    }
    fn add_resource(
        &mut self,
        slot: usize,
        stack: &ItemStack,
        equipment: &mut EntityEquipment,
    ) -> i32 {
        if self.get_item(slot, equipment).is_empty() {
            self.set_item(slot, copy_with_count(stack, 0), equipment);
        }
        let target = self.get_item_mut(slot, equipment);
        // A zero-count stack retains its component payload for this calculation.
        let max = max_stack_size(target).min(99);
        let old = target.0.as_ref().map_or(0, |v| v.count);
        let added = count(stack).min(max - old);
        if added == 0 {
            return count(stack);
        }
        set_count(target, old.wrapping_add(added));
        self.pop_times[slot] = 5;
        count(stack) - added
    }
    pub fn add(
        &mut self,
        stack: &mut ItemStack,
        equipment: &mut EntityEquipment,
        infinite_materials: bool,
    ) -> bool {
        self.add_to_slot(None, stack, equipment, infinite_materials)
    }
    /// Port of Inventory#add, including its last-iteration return value when an
    /// inventory fills after accepting only part of the incoming stack.
    pub fn add_to_slot(
        &mut self,
        slot: Option<usize>,
        stack: &mut ItemStack,
        equipment: &mut EntityEquipment,
        infinite_materials: bool,
    ) -> bool {
        if stack.is_empty() {
            return false;
        }
        if is_damaged(stack) {
            if let Some(slot) = slot.or_else(|| self.get_free_slot()) {
                self.items[slot] = stack.clone();
                set_count(stack, 0);
                self.pop_times[slot] = 5;
                return true;
            }
            if infinite_materials {
                set_count(stack, 0);
                return true;
            }
            return false;
        }
        let last_size = loop {
            let previous = count(stack);
            let target = slot
                .or_else(|| self.get_slot_with_remaining_space(stack, equipment))
                .or_else(|| self.get_free_slot());
            if let Some(target) = target {
                let remaining = self.add_resource(target, stack, equipment);
                set_count(stack, remaining);
            }
            if stack.is_empty() || count(stack) >= previous {
                break previous;
            }
        };
        if count(stack) == last_size && infinite_materials {
            set_count(stack, 0);
            true
        } else {
            count(stack) < last_size
        }
    }
    pub fn place_item_back_in_inventory(
        &mut self,
        stack: &mut ItemStack,
        send_slot: bool,
        equipment: &mut EntityEquipment,
        infinite_materials: bool,
    ) -> Vec<InventoryEffect> {
        let mut effects = Vec::new();
        while !stack.is_empty() {
            let Some(slot) = self
                .get_slot_with_remaining_space(stack, equipment)
                .or_else(|| self.get_free_slot())
            else {
                effects.push(InventoryEffect::Drop(std::mem::take(stack)));
                break;
            };
            let space = max_stack_size(stack) - count(self.get_item(slot, equipment));
            let mut part = split(stack, space);
            if self.add_to_slot(Some(slot), &mut part, equipment, infinite_materials) && send_slot {
                effects.push(InventoryEffect::SetSlot {
                    slot,
                    stack: self.get_item(slot, equipment).clone(),
                });
            }
        }
        effects
    }
    pub fn set_changed(&mut self) {
        self.times_changed = self.times_changed.wrapping_add(1);
    }
    pub fn times_changed(&self) -> i32 {
        self.times_changed
    }
    pub fn clear_content(&mut self, equipment: &mut EntityEquipment) {
        self.items.fill_with(ItemStack::default);
        equipment.clear();
    }
    pub fn is_empty(&self, equipment: &EntityEquipment) -> bool {
        self.items.iter().all(ItemStack::is_empty) && equipment.is_empty()
    }
    /// Inventory saves only its 36 main slots. LivingEntity saves equipment.
    pub fn save(&self) -> Result<ListTag> {
        let mut entries = Vec::new();
        for (slot, stack) in self.items.iter().enumerate().filter(|(_, s)| !s.is_empty()) {
            let mut tag = stack
                .to_nbt()?
                .as_compound()
                .expect("stack map codec")
                .clone();
            tag.insert("Slot", Tag::Byte(slot as i8));
            entries.push(Tag::Compound(tag));
        }
        Ok(ListTag(entries))
    }
    pub fn load(&mut self, entries: &ListTag) {
        self.items.fill_with(ItemStack::default);
        for value in entries.iter() {
            let Some(tag) = value.as_compound() else {
                continue;
            };
            let slot = usize::from(tag.get_byte_or("Slot", 0) as u8);
            if slot < 36
                && let Ok(stack) = ItemStack::from_nbt(value)
            {
                self.items[slot] = stack;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_protocol::{codec::VarInt, component::DataComponentPatch};
    fn stack(item: ferraloom_data::Item, amount: i32) -> ItemStack {
        ItemStack::new(item, amount, DataComponentPatch::default())
    }
    #[test]
    fn vanilla_inventory_add_priority_partial_return_and_creative_discard() {
        let mut n = 0;
        for row in include_str!("../../../tests/java/entity/vectors.tsv")
            .lines()
            .filter(|s| s.starts_with("inventory\t"))
        {
            let f: Vec<_> = row.split('\t').collect();
            let case: i32 = f[1].parse().unwrap();
            let mut inventory = Inventory::default();
            let mut equipment = EntityEquipment::default();
            let mut incoming = stack(ferraloom_data::Item::STONE, if case == 0 { 70 } else { 8 });
            if case == 1 || case == 5 {
                inventory
                    .items
                    .fill_with(|| stack(ferraloom_data::Item::STONE, 64));
                inventory.items[0] =
                    stack(ferraloom_data::Item::STONE, if case == 1 { 63 } else { 64 });
                set_count(&mut incoming, 2);
            }
            if case == 2 {
                inventory.items[0] = stack(ferraloom_data::Item::DIRT, 64);
                equipment.set(
                    EquipmentSlot::Offhand,
                    stack(ferraloom_data::Item::STONE, 60),
                );
            }
            if case == 3 {
                inventory.set_selected_slot(5);
                inventory.items[5] = stack(ferraloom_data::Item::STONE, 60);
                equipment.set(
                    EquipmentSlot::Offhand,
                    stack(ferraloom_data::Item::STONE, 60),
                );
            }
            if case == 4 {
                incoming = ItemStack::new(
                    ferraloom_data::Item::DIAMOND_PICKAXE,
                    1,
                    DataComponentPatch {
                        added: vec![DataComponent::Damage(VarInt(9))],
                        removed: Vec::new(),
                    },
                );
            }
            let added = inventory.add(&mut incoming, &mut equipment, case == 5);
            assert_eq!(added, f[2] == "true", "{row}");
            assert_eq!(count(&incoming), f[3].parse::<i32>().unwrap(), "{row}");
            let mut slots = Vec::new();
            for slot in 0..43 {
                let item = inventory.get_item(slot, &equipment);
                if !item.is_empty() {
                    let damage = match component(item, ComponentType::DAMAGE) {
                        Some(DataComponent::Damage(v)) => v.0,
                        _ => 0,
                    };
                    slots.push(format!(
                        "{}:{}:{}:{}",
                        slot,
                        item.0.as_ref().unwrap().item.name(),
                        count(item),
                        damage
                    ));
                }
            }
            assert_eq!(slots.join(";"), f[4], "{row}");
            n += 1;
        }
        assert_eq!(n, 6);
    }
    #[test]
    fn equipment_and_inventory_have_separate_nbt_codecs_and_split_counts() {
        let mut inventory = Inventory::default();
        let mut equipment = EntityEquipment::default();
        inventory.items[4] = stack(ferraloom_data::Item::STONE, 10);
        equipment.set(
            EquipmentSlot::Head,
            stack(ferraloom_data::Item::DIAMOND_HELMET, 1),
        );
        equipment.set(
            EquipmentSlot::Offhand,
            stack(ferraloom_data::Item::STONE, 7),
        );
        let saved = inventory.save().unwrap();
        assert_eq!(saved.len(), 1);
        let mut loaded = Inventory::default();
        loaded.load(&saved);
        assert_eq!(loaded.items, inventory.items);
        let mut equipment = EntityEquipment::load(&equipment.save().unwrap()).unwrap();
        assert_eq!(count(&loaded.remove_item(40, 3, &mut equipment)), 3);
        assert_eq!(count(equipment.get(EquipmentSlot::Offhand)), 4);
        assert_eq!(count(&loaded.remove_item_no_update(39, &mut equipment)), 1);
        assert!(equipment.get(EquipmentSlot::Head).is_empty());
    }
}
