//! Port of BaseContainerBlockEntity, RandomizableContainerBlockEntity and ContainerHelper.
use crate::entity::player::inventory::{count, max_stack_size, set_count};
use crate::{block::entity::BlockEntityData, inventory::Container};
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_protocol::{
    Identifier,
    chat::Component,
    component::{
        BoundedList, ComponentTypeId, DataComponent, DataComponentMap, ItemStack,
        ItemStackTemplate, LockCode, Persistent, SeededContainerLoot,
    },
};

#[derive(Clone, Debug)]
pub struct BaseContainerBlockEntity {
    pub parent: BlockEntityData,
    pub items: Vec<ItemStack>,
    pub custom_name: Option<Component>,
    pub lock: Option<LockCode>,
    pub times_changed: u32,
}
impl BaseContainerBlockEntity {
    pub fn new(parent: BlockEntityData, size: usize) -> Self {
        Self {
            parent,
            items: vec![ItemStack::default(); size],
            custom_name: None,
            lock: None,
            times_changed: 0,
        }
    }
    pub fn load_base(&mut self, tag: &CompoundTag) {
        self.custom_name = tag
            .get("CustomName")
            .and_then(|tag| Component::from_nbt(tag.clone()).ok());
        self.lock = tag.get("lock").and_then(|tag| LockCode::from_nbt(tag).ok());
    }
    pub fn save_base(&self) -> ferraloom_protocol::Result<CompoundTag> {
        let mut tag = CompoundTag::new();
        if let Some(name) = &self.custom_name {
            tag.insert("CustomName", name.to_nbt()?);
        }
        if let Some(lock) = &self.lock {
            tag.insert("lock", lock.to_nbt()?);
        }
        Ok(tag)
    }
    pub fn apply_components(&mut self, map: &DataComponentMap) {
        self.custom_name = match map.get(ComponentTypeId::CUSTOM_NAME) {
            Some(DataComponent::CustomName(name)) => Some((**name).clone()),
            _ => None,
        };
        self.lock = match map.get(ComponentTypeId::LOCK) {
            Some(DataComponent::Lock(lock)) => Some(lock.clone()),
            _ => None,
        };
        self.items.fill(ItemStack::default());
        if let Some(DataComponent::Container(items)) = map.get(ComponentTypeId::CONTAINER) {
            for (to, from) in self.items.iter_mut().zip(&items.0) {
                if let Some(from) = from {
                    *to = ItemStack::new(from.item, from.count, from.components.clone());
                }
            }
        }
    }
    pub fn collect_components(&self, map: &mut DataComponentMap) {
        if let Some(name) = &self.custom_name {
            set(map, DataComponent::CustomName(Box::new(name.clone())));
        }
        if let Some(lock) = &self.lock {
            set(map, DataComponent::Lock(lock.clone()));
        }
        let last = self
            .items
            .iter()
            .rposition(|stack| !stack.is_empty())
            .map_or(0, |i| i + 1);
        set(
            map,
            DataComponent::Container(BoundedList(
                self.items[..last]
                    .iter()
                    .map(|stack| {
                        stack.0.as_ref().map(|stack| ItemStackTemplate {
                            item: stack.item,
                            count: stack.count,
                            components: stack.components.clone(),
                        })
                    })
                    .collect(),
            )),
        );
    }
}
impl Container for BaseContainerBlockEntity {
    fn size(&self) -> usize {
        self.items.len()
    }
    fn get_item(&self, slot: usize) -> ItemStack {
        self.items[slot].clone()
    }
    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        let size = count(&stack).min(self.max_stack_size().min(max_stack_size(&stack)));
        set_count(&mut stack, size);
        self.items[slot] = stack;
        self.set_changed();
    }
    fn set_changed(&mut self) {
        self.times_changed = self.times_changed.wrapping_add(1);
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        self.items
            .get_mut(slot)
            .map(std::mem::take)
            .unwrap_or_default()
    }
    fn clear_content(&mut self) {
        self.items.fill(ItemStack::default());
    }
}
#[derive(Clone, Debug)]
pub struct RandomizableContainerBlockEntity {
    pub parent: BaseContainerBlockEntity,
    pub loot_table: Option<Identifier>,
    pub loot_table_seed: i64,
}
impl RandomizableContainerBlockEntity {
    pub fn new(parent: BlockEntityData, size: usize) -> Self {
        Self {
            parent: BaseContainerBlockEntity::new(parent, size),
            loot_table: None,
            loot_table_seed: 0,
        }
    }
    pub fn load(&mut self, tag: &CompoundTag) {
        self.parent.load_base(tag);
        self.loot_table = tag
            .get_string("LootTable")
            .and_then(|id| Identifier::parse(&id.to_string_lossy()).ok());
        self.loot_table_seed = tag.get_long_or("LootTableSeed", 0);
        self.parent.items.fill(ItemStack::default());
        if self.loot_table.is_none() {
            load_items(tag, &mut self.parent.items);
        }
    }
    pub fn save(&self, also_when_empty: bool) -> ferraloom_protocol::Result<CompoundTag> {
        let mut tag = self.parent.save_base()?;
        if let Some(loot) = &self.loot_table {
            tag.insert("LootTable", Tag::String(loot.as_str().into()));
            if self.loot_table_seed != 0 {
                tag.insert("LootTableSeed", Tag::Long(self.loot_table_seed));
            }
        } else {
            save_items(&mut tag, &self.parent.items, also_when_empty)?;
        }
        Ok(tag)
    }
    pub fn apply_components(&mut self, map: &DataComponentMap) {
        self.parent.apply_components(map);
        if let Some(DataComponent::ContainerLoot(loot)) = map.get(ComponentTypeId::CONTAINER_LOOT) {
            self.loot_table = Some(loot.loot_table.clone());
            self.loot_table_seed = loot.seed;
        }
    }
    pub fn collect_components(&self, map: &mut DataComponentMap) {
        self.parent.collect_components(map);
        if let Some(loot) = &self.loot_table {
            set(
                map,
                DataComponent::ContainerLoot(SeededContainerLoot {
                    loot_table: loot.clone(),
                    seed: self.loot_table_seed,
                }),
            );
        }
    }
}
pub const BASE_COMPONENTS: &[ComponentTypeId] = &[
    ComponentTypeId::CUSTOM_NAME,
    ComponentTypeId::LOCK,
    ComponentTypeId::CONTAINER,
];
pub const RANDOMIZABLE_COMPONENTS: &[ComponentTypeId] = &[
    ComponentTypeId::CUSTOM_NAME,
    ComponentTypeId::LOCK,
    ComponentTypeId::CONTAINER,
    ComponentTypeId::CONTAINER_LOOT,
];
pub fn set(map: &mut DataComponentMap, component: DataComponent) {
    if let Some(old) = map
        .entries
        .iter_mut()
        .find(|entry| entry.component_type() == component.component_type())
    {
        *old = component;
    } else {
        map.entries.push(component);
    }
}
pub fn load_items(tag: &CompoundTag, items: &mut [ItemStack]) {
    for entry in tag
        .get_list("Items")
        .into_iter()
        .flat_map(|list| list.0.iter())
        .filter_map(Tag::as_compound)
    {
        let slot = usize::from(entry.get_byte_or("Slot", 0) as u8);
        if slot < items.len()
            && let Ok(stack) = ItemStack::from_nbt(&Tag::Compound(entry.clone()))
        {
            items[slot] = stack;
        }
    }
}
pub fn save_items(
    tag: &mut CompoundTag,
    items: &[ItemStack],
    also_when_empty: bool,
) -> ferraloom_protocol::Result<()> {
    let mut list = Vec::new();
    for (slot, stack) in items
        .iter()
        .enumerate()
        .filter(|(_, stack)| !stack.is_empty())
    {
        if let Tag::Compound(mut entry) = stack.to_nbt()? {
            entry.insert("Slot", Tag::Byte(slot as i8));
            list.push(Tag::Compound(entry));
        }
    }
    if also_when_empty || !list.is_empty() {
        tag.insert("Items", Tag::List(ListTag(list)));
    }
    Ok(())
}
/// Port of AbstractContainerMenu#getRedstoneSignalFromContainer, including f32 fill ratios.
pub fn analog_output(container: &impl Container) -> i32 {
    let mut full = 0.0f32;
    for slot in 0..container.size() {
        let stack = container.get_item(slot);
        if !stack.is_empty() {
            full += count(&stack) as f32
                / container.max_stack_size().min(max_stack_size(&stack)) as f32;
        }
    }
    full /= container.size() as f32;
    (full * 14.0).floor() as i32 + i32::from(full > 0.0)
}
