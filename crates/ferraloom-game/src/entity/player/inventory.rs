//! Port of Player's Inventory and the ItemStack operations used by containers.
#[cfg(test)]
mod tool_tests;
use super::equipment::EntityEquipment;
use crate::inventory::{StackGenerations, StackReplacement};
use ferraloom_data::registries::data_component_type::Id as ComponentType;
use ferraloom_nbt::{ListTag, Tag};
use ferraloom_protocol::{
    Result,
    component::{DataComponent, EquipmentSlot, ItemStack, Persistent},
};

/// Access a stack's effective component without cloning its immutable prototype.
pub fn component(stack: &ItemStack, key: ComponentType) -> Option<&DataComponent> {
    stack
        .component(key)
        .expect("generated vanilla item components")
}
/// Port of ItemStack#getDestroySpeed, Item#getDestroySpeed and Tool#getMiningSpeed.
/// Rules without a speed do not resolve their holder set at all.
pub fn get_destroy_speed(
    stack: &ItemStack,
    state: ferraloom_data::BlockState,
    level: &crate::level::Level,
) -> std::result::Result<f32, crate::level::LevelError> {
    if stack.is_empty() {
        return Ok(1.0);
    }
    let Some(DataComponent::Tool(tool)) = component(stack, ComponentType::TOOL) else {
        return Ok(1.0);
    };
    for rule in &tool.rules {
        if let Some(speed) = rule.speed
            && tool_blocks_match(&rule.blocks, state, level)?
        {
            return Ok(speed.0);
        }
    }
    Ok(tool.default_mining_speed.0)
}

fn tool_blocks_match(
    blocks: &ferraloom_protocol::component::BlockSet,
    state: ferraloom_data::BlockState,
    level: &crate::level::Level,
) -> std::result::Result<bool, crate::level::LevelError> {
    use crate::level::LevelError;
    use ferraloom_protocol::codec::HolderSet;
    Ok(match &blocks.0 {
        HolderSet::Direct(blocks) => blocks.iter().any(|id| id.as_str() == state.block().name()),
        HolderSet::Named(name) => {
            let tags = level.block_tags()?;
            let key = ferraloom_data::datapack::Identifier::new(name.namespace(), name.path())
                .map_err(|_| LevelError::MissingBlockTag)?;
            // TODO(5a): registry-aware component decoding must reject missing tags.
            let tag = tags.resolve_name(&key).ok_or(LevelError::MissingBlockTag)?;
            tags.contains(tag, state.block())
        }
    })
}
/// Port of ItemStack#isCorrectToolForDrops, Item#isCorrectToolForDrops and
/// Tool#isCorrectForDrops. Membership uses the current datapack's block tags.
pub fn is_correct_tool_for_drops(
    stack: &ItemStack,
    state: ferraloom_data::BlockState,
    level: &crate::level::Level,
) -> std::result::Result<bool, crate::level::LevelError> {
    if stack.is_empty() {
        return Ok(false);
    }
    let Some(DataComponent::Tool(tool)) = component(stack, ComponentType::TOOL) else {
        return Ok(false);
    };
    for rule in &tool.rules {
        let Some(correct) = rule.correct_for_drops else {
            continue;
        };
        if tool_blocks_match(&rule.blocks, state, level)? {
            return Ok(correct);
        }
    }
    Ok(false)
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
    stack.copy_with_count(amount)
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
    a.is_same_item_same_components(b)
        .expect("canonical item components")
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
    stack_generations: StackGenerations,
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
            stack_generations: StackGenerations::new(36),
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
    pub fn set_selected_item(&mut self, stack: ItemStack) -> StackReplacement {
        self.stack_generations
            .replace(self.selected, &mut self.items[self.selected], stack)
    }
    pub fn stack_parts<'a>(
        &'a self,
        slot: usize,
        equipment: &'a EntityEquipment,
    ) -> Option<(&'a StackGenerations, &'a ItemStack, usize)> {
        if slot < self.items.len() {
            Some((&self.stack_generations, &self.items[slot], slot))
        } else {
            Some(equipment.stack_parts(equipment_slot(slot)?))
        }
    }
    pub fn stack_parts_mut<'a>(
        &'a mut self,
        slot: usize,
        equipment: &'a mut EntityEquipment,
    ) -> Option<(&'a mut StackGenerations, &'a mut ItemStack, usize)> {
        if slot < self.items.len() {
            Some((&mut self.stack_generations, &mut self.items[slot], slot))
        } else {
            Some(equipment.stack_parts_mut(equipment_slot(slot)?))
        }
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
    ) -> StackReplacement {
        if slot < 36 {
            self.stack_generations
                .replace(slot, &mut self.items[slot], stack)
        } else if let Some(slot) = equipment_slot(slot) {
            equipment.set(slot, stack)
        } else {
            StackReplacement::Owned(ItemStack::default())
        }
    }
    pub fn remove_item(
        &mut self,
        slot: usize,
        amount: i32,
        equipment: &mut EntityEquipment,
    ) -> ItemStack {
        if slot < 36 && amount <= 0 {
            return ItemStack::default();
        }
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
            .into_owned()
            .expect("value-only removal requires an unrooted stack; live callers use StackRef")
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
        self.stack_generations
            .swap(&mut self.items, self.selected, slot);
    }
    pub fn add_and_pick_item(&mut self, stack: ItemStack) {
        self.selected = self.get_suitable_hotbar_slot();
        if !self.items[self.selected].is_empty()
            && let Some(free) = self.get_free_slot()
        {
            self.stack_generations
                .move_to(&mut self.items, self.selected, free);
        }
        self.set_selected_item(stack);
    }
    pub fn add(
        &mut self,
        stack: &mut ItemStack,
        equipment: &mut EntityEquipment,
        infinite_materials: bool,
    ) -> bool {
        crate::inventory::add_inventory_value(self, equipment, stack, None, infinite_materials)
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
        crate::inventory::add_inventory_value(self, equipment, stack, slot, infinite_materials)
    }
    /// Transient ItemStack#setPopTime used by the shared insertion kernel.
    pub(crate) fn set_pop_time(&mut self, slot: usize, ticks: i32) {
        self.pop_times[slot] = ticks;
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
        for slot in 0..self.items.len() {
            self.stack_generations
                .replace(slot, &mut self.items[slot], ItemStack::default());
        }
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
        for slot in 0..self.items.len() {
            self.stack_generations
                .replace(slot, &mut self.items[slot], ItemStack::default());
        }
        for value in entries.iter() {
            let Some(tag) = value.as_compound() else {
                continue;
            };
            let slot = usize::from(tag.get_byte_or("Slot", 0) as u8);
            if slot < 36
                && let Ok(stack) = ItemStack::from_nbt(value)
            {
                self.stack_generations
                    .replace(slot, &mut self.items[slot], stack);
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
                        item.item().name(),
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

#[cfg(test)]
mod removal_tests {
    use super::*;
    use ferraloom_protocol::component::DataComponentPatch;
    #[test]
    fn nonpositive_main_slot_removal_is_empty_but_equipment_splits_directly() {
        let mut inventory = Inventory::default();
        let mut equipment = EntityEquipment::default();
        inventory.items[0] = ItemStack::new(
            ferraloom_data::Item::STONE,
            5,
            DataComponentPatch::default(),
        );
        equipment.set(EquipmentSlot::Offhand, inventory.items[0].clone());
        for amount in [0, -1] {
            assert!(inventory.remove_item(0, amount, &mut equipment).is_empty());
            assert_eq!(count(&inventory.items[0]), 5);
        }
        assert!(inventory.remove_item(40, -1, &mut equipment).is_empty());
        assert_eq!(count(equipment.get(EquipmentSlot::Offhand)), 6);
    }
}
