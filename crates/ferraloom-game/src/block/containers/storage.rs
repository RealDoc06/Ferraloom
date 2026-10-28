//! Port of BaseContainerBlockEntity, RandomizableContainerBlockEntity and ContainerHelper.
use crate::entity::player::inventory::{count, max_stack_size, set_count};
use crate::{
    block::entity::{BlockEntityData, components},
    inventory::{Container, StackGenerations, StackReplacement},
};
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_protocol::{
    Identifier,
    chat::Component,
    codec::{HolderSet, RegistryContext},
    component::{
        BoundedList, ComponentPredicate, ComponentTypeId, DataComponent, DataComponentMap,
        DataComponentMatchers, IntBounds, ItemSet, ItemStack, ItemStackTemplate, LockCode,
        Persistent, SeededContainerLoot,
    },
};

#[derive(Clone, Debug)]
pub struct BaseContainerBlockEntity {
    pub parent: BlockEntityData,
    pub items: Vec<ItemStack>,
    stack_generations: StackGenerations,
    pub custom_name: Option<Component>,
    pub lock: Option<LockCode>,
    pub times_changed: u32,
}
impl BaseContainerBlockEntity {
    pub fn new(parent: BlockEntityData, size: usize) -> Self {
        Self {
            parent,
            items: vec![ItemStack::default(); size],
            stack_generations: StackGenerations::new(size),
            custom_name: None,
            lock: None,
            times_changed: 0,
        }
    }
    pub fn replace_item(&mut self, slot: usize, item: ItemStack) -> StackReplacement {
        self.stack_generations
            .replace(slot, &mut self.items[slot], item)
    }
    pub fn load_items(&mut self, tag: &CompoundTag) {
        self.clear_content();
        for (slot, stack) in item_entries(tag, self.items.len()) {
            self.replace_item(slot, stack);
        }
    }
    pub fn load_base(&mut self, tag: &CompoundTag, registries: &dyn RegistryContext) {
        self.custom_name = tag
            .get("CustomName")
            .and_then(|tag| Component::from_nbt(tag.clone()).ok());
        self.lock = load_lock(tag, registries);
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
        self.clear_content();
        if let Some(DataComponent::Container(items)) = map.get(ComponentTypeId::CONTAINER) {
            for (slot, from) in items.0.iter().take(self.items.len()).enumerate() {
                if let Some(from) = from {
                    self.replace_item(
                        slot,
                        ItemStack::new(from.item, from.count, from.components.clone()),
                    );
                }
            }
        }
    }
    pub fn is_locked(&self) -> bool {
        self.lock.as_ref().is_some_and(|lock| {
            lock.items.is_some()
                || lock.count.min.is_some()
                || lock.count.max.is_some()
                || !lock.components.exact.is_empty()
                || !lock.components.partial.0.is_empty()
        })
    }
    pub fn collect_components(&self, map: &mut DataComponentMap) {
        collect_custom_name(map, self.custom_name.as_ref());
        if self.is_locked()
            && let Some(lock) = &self.lock
        {
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

/// LockCode#fromTag through ValueInput#read: optional record fields retain
/// their codec partials, while a non-map has no partial lock at all.
pub(crate) fn load_lock(tag: &CompoundTag, registries: &dyn RegistryContext) -> Option<LockCode> {
    let value = tag.get_compound("lock")?;
    let item_registry = Identifier::parse("minecraft:item").expect("native registry key");
    let items = value.get("items").and_then(|value| {
        if let Some(size) = value.collection_len() {
            // ListCodec keeps the successfully bound members as its partial.
            let mut entries = Vec::new();
            for index in 0..size {
                let entry = value.collection_get(index)?;
                if !matches!(entry, Tag::String(_)) {
                    continue;
                }
                if let Ok(ItemSet(HolderSet::Direct(keys))) = ItemSet::from_nbt(&entry) {
                    entries.extend(
                        keys.into_iter()
                            .filter(|key| registries.entry_id(&item_registry, key).is_some()),
                    );
                }
            }
            Some(ItemSet(HolderSet::Direct(entries)))
        } else {
            let items = ItemSet::from_nbt(value).ok()?;
            let bound = match &items.0 {
                HolderSet::Named(tag) => registries.has_tag(&item_registry, tag),
                HolderSet::Direct(keys) => keys
                    .iter()
                    .all(|key| registries.entry_id(&item_registry, key).is_some()),
            };
            bound.then_some(items)
        }
    });
    let count = value.get("count").map_or_else(IntBounds::default, |value| {
        let bounds = if let Some(number) = codec_int(value) {
            IntBounds {
                min: Some(number),
                max: Some(number),
            }
        } else if let Some(map) = value.as_compound() {
            IntBounds {
                min: map.get("min").and_then(codec_int),
                max: map.get("max").and_then(codec_int),
            }
        } else {
            IntBounds::default()
        };
        // Reuse the strict bounds validator after recovering optional fields.
        if bounds.to_persistent().is_ok() {
            bounds
        } else {
            IntBounds::default()
        }
    });
    let exact = components::load(value.get_compound("components"), registries).entries;
    let partial = value
        .get_compound("predicates")
        .into_iter()
        .flat_map(CompoundTag::iter)
        .filter_map(|(key, value)| {
            Identifier::parse(&key.to_string_lossy())
                .ok()
                .and_then(|key| ComponentPredicate::from_key_nbt(&key, value).ok())
        })
        .filter(|value| value.to_persistent().is_ok())
        .collect();
    Some(LockCode {
        items,
        count,
        components: DataComponentMatchers {
            exact,
            partial: BoundedList(partial),
        },
    })
}
// Codec.INT uses Number.intValue, not NumericTag's floor-based intValue.
fn codec_int(value: &Tag) -> Option<i32> {
    Some(match value {
        Tag::Byte(v) => i32::from(*v),
        Tag::Short(v) => i32::from(*v),
        Tag::Int(v) => *v,
        Tag::Long(v) => *v as i32,
        Tag::Float(v) => *v as i32,
        Tag::Double(v) => *v as i32,
        _ => return None,
    })
}
impl Container for BaseContainerBlockEntity {
    fn reassign_item(&mut self, slot: usize) -> crate::inventory::Result<()> {
        let size = count(&self.items[slot])
            .min(self.max_stack_size().min(max_stack_size(&self.items[slot])));
        set_count(&mut self.items[slot], size);
        self.set_changed();
        Ok(())
    }
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
        let result = crate::entity::player::inventory::split(&mut self.items[slot], amount);
        if !result.is_empty() {
            self.set_changed();
        }
        result
    }
    fn set_item_count(&mut self, slot: usize, amount: i32) -> crate::inventory::Result<()> {
        set_count(&mut self.items[slot], amount);
        Ok(())
    }
    fn size(&self) -> usize {
        self.items.len()
    }
    fn get_item(&self, slot: usize) -> ItemStack {
        self.items[slot].clone()
    }
    fn set_item(&mut self, slot: usize, stack: ItemStack) {
        self.replace_item(slot, stack);
        self.reassign_item(slot).expect("base container setter");
    }
    fn set_changed(&mut self) {
        self.times_changed = self.times_changed.wrapping_add(1);
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        if self.items.get(slot).is_none_or(ItemStack::is_empty) {
            return ItemStack::default();
        }
        self.replace_item(slot, ItemStack::default())
            .into_owned()
            .expect("live value removal requires an unrooted stack")
    }
    fn clear_content(&mut self) {
        for slot in 0..self.items.len() {
            self.replace_item(slot, ItemStack::default());
        }
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
    pub fn load(&mut self, tag: &CompoundTag, registries: &dyn RegistryContext) {
        self.parent.load_base(tag, registries);
        self.loot_table = tag
            .get_string("LootTable")
            .and_then(|id| Identifier::parse(&id.to_string_lossy()).ok());
        self.loot_table_seed = tag.get_long_or("LootTableSeed", 0);
        self.parent.clear_content();
        if self.loot_table.is_none() {
            self.parent.load_items(tag);
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
/// Port of ContainerHelper#loadAllItems: TagValueInput's typed-list iterator
/// retains ItemStackWithSlot codec partials, including an invalid count's 1.
pub(crate) fn item_entries(
    tag: &CompoundTag,
    size: usize,
) -> impl Iterator<Item = (usize, ItemStack)> + '_ {
    tag.get_list("Items")
        .into_iter()
        .flat_map(|list| list.iter())
        .filter_map(Tag::as_compound)
        .filter_map(move |entry| {
            let slot = usize::from(entry.get_byte_or("Slot", 0) as u8);
            let stack = ItemStack::from_nbt_with_partial(&Tag::Compound(entry.clone())).ok()?;
            (slot < size).then_some((slot, stack))
        })
}
/// Port of ItemContainerContents#createStackFromSlot/copyOne. Templates create
/// fresh native objects; no live container receiver is copied for mutation.
pub(crate) fn component_item(components: &DataComponentMap, slot: usize) -> ItemStack {
    match components.get(ComponentTypeId::CONTAINER) {
        Some(DataComponent::Container(contents)) => contents
            .0
            .get(slot)
            .and_then(Option::as_ref)
            .map(crate::item::stack::create_from_template)
            .unwrap_or_default(),
        _ => ItemStack::default(),
    }
}
/// Port of ItemContainerContents#fromItems: only native nonempty stacks become
/// templates, and trailing empty slots are omitted.
pub(crate) fn container_component(items: &[ItemStack]) -> DataComponent {
    let end = items
        .iter()
        .rposition(|item| !item.is_empty())
        .map_or(0, |slot| slot + 1);
    DataComponent::Container(BoundedList(
        items[..end]
            .iter()
            .map(|item| {
                if item.is_empty() {
                    None
                } else {
                    item.0.as_ref().map(|item| ItemStackTemplate {
                        item: item.item,
                        count: item.count,
                        components: item.components.clone(),
                    })
                }
            })
            .collect(),
    ))
}
pub fn load_items(tag: &CompoundTag, items: &mut [ItemStack]) {
    for (slot, stack) in item_entries(tag, items.len()) {
        items[slot] = stack;
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

#[cfg(test)]
mod partial_tests;

/// Port of DataComponentMap.Builder#set(CUSTOM_NAME, nullable): an absent
/// implicit name removes a value inherited from the extra component map.
pub(crate) fn collect_custom_name(map: &mut DataComponentMap, name: Option<&Component>) {
    if let Some(name) = name {
        set(map, DataComponent::CustomName(Box::new(name.clone())));
    } else {
        map.entries
            .retain(|value| value.component_type() != ComponentTypeId::CUSTOM_NAME);
    }
}
