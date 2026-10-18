//! Port of EntityEquipment, embedded once in LivingEntity.
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    Result,
    component::{EquipmentSlot, ItemStack, Persistent},
};

/// Native EnumMap traversal order differs from the equipment packet's numeric IDs.
pub const SLOTS: [EquipmentSlot; 8] = [
    EquipmentSlot::Mainhand,
    EquipmentSlot::Offhand,
    EquipmentSlot::Feet,
    EquipmentSlot::Legs,
    EquipmentSlot::Chest,
    EquipmentSlot::Head,
    EquipmentSlot::Body,
    EquipmentSlot::Saddle,
];
fn index(slot: EquipmentSlot) -> usize {
    match slot {
        EquipmentSlot::Mainhand => 0,
        EquipmentSlot::Offhand => 1,
        EquipmentSlot::Feet => 2,
        EquipmentSlot::Legs => 3,
        EquipmentSlot::Chest => 4,
        EquipmentSlot::Head => 5,
        EquipmentSlot::Body => 6,
        EquipmentSlot::Saddle => 7,
    }
}
pub fn slot_name(slot: EquipmentSlot) -> &'static str {
    match slot {
        EquipmentSlot::Mainhand => "mainhand",
        EquipmentSlot::Offhand => "offhand",
        EquipmentSlot::Feet => "feet",
        EquipmentSlot::Legs => "legs",
        EquipmentSlot::Chest => "chest",
        EquipmentSlot::Head => "head",
        EquipmentSlot::Body => "body",
        EquipmentSlot::Saddle => "saddle",
    }
}
#[derive(Clone, Debug)]
pub struct EntityEquipment {
    items: [ItemStack; 8],
}
impl Default for EntityEquipment {
    fn default() -> Self {
        Self {
            items: std::array::from_fn(|_| ItemStack::default()),
        }
    }
}
impl EntityEquipment {
    pub fn get(&self, slot: EquipmentSlot) -> &ItemStack {
        &self.items[index(slot)]
    }
    pub fn get_mut(&mut self, slot: EquipmentSlot) -> &mut ItemStack {
        &mut self.items[index(slot)]
    }
    pub fn set(&mut self, slot: EquipmentSlot, stack: ItemStack) -> ItemStack {
        std::mem::replace(self.get_mut(slot), stack)
    }
    pub fn is_empty(&self) -> bool {
        self.items.iter().all(ItemStack::is_empty)
    }
    pub fn clear(&mut self) {
        self.items.fill_with(ItemStack::default);
    }
    pub fn set_all(&mut self, other: &Self) {
        self.clone_from(other);
    }
    pub fn iter(&self) -> impl Iterator<Item = (EquipmentSlot, &ItemStack)> {
        SLOTS.into_iter().map(|slot| (slot, self.get(slot)))
    }
    /// EntityEquipment.CODEC omits empty stacks and uses serialized slot names.
    pub fn save(&self) -> Result<CompoundTag> {
        let mut tag = CompoundTag::new();
        for (slot, stack) in self.iter().filter(|(_, stack)| !stack.is_empty()) {
            tag.insert(slot_name(slot), stack.to_nbt()?);
        }
        Ok(tag)
    }
    pub fn load(tag: &CompoundTag) -> Result<Self> {
        let mut equipment = Self::default();
        for slot in SLOTS {
            if let Some(stack) = tag.get(slot_name(slot)) {
                equipment.set(slot, ItemStack::from_nbt(stack)?);
            }
        }
        // Vanilla's unbounded enum-keyed codec rejects unknown slot names.
        for (name, _) in tag.iter() {
            if !SLOTS
                .iter()
                .any(|slot| name.to_string_lossy() == slot_name(*slot))
            {
                return Err(ferraloom_protocol::Error::invalid(
                    "equipment slot",
                    name.to_string_lossy(),
                ));
            }
        }
        Ok(equipment)
    }
    pub fn from_tag(tag: &Tag) -> Result<Self> {
        tag.as_compound()
            .ok_or_else(|| ferraloom_protocol::Error::invalid("equipment", "expected compound"))
            .and_then(Self::load)
    }
}
