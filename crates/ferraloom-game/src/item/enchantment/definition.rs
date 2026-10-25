//! Registry definition and equipment/item eligibility shared by effect callers.
use crate::{
    loot::{LootError, Result, codec},
    predicates::{PredicateContext, validate_static_holders},
};
use ferraloom_data::Item;
use ferraloom_protocol::{
    codec::HolderSet,
    component::{EquipmentSlot, EquipmentSlotGroup, ItemStack, Persistent},
};
use serde_json::Value;

/// Port of Enchantment.Cost, including Java's overflowing integer arithmetic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EnchantmentCost {
    pub base: i32,
    pub per_level_above_first: i32,
}
impl EnchantmentCost {
    pub fn decode(value: &Value) -> Result<Self> {
        Ok(Self {
            base: codec::int(codec::field(value, "base")?)?,
            per_level_above_first: codec::int(codec::field(value, "per_level_above_first")?)?,
        })
    }
    pub fn calculate(self, level: i32) -> i32 {
        self.base.wrapping_add(
            self.per_level_above_first
                .wrapping_mul(level.wrapping_sub(1)),
        )
    }
}

/// Port of Enchantment.EnchantmentDefinition. Holder sets retain datapack tags
/// rather than expanding them into a second item-membership table.
#[derive(Clone, Debug, PartialEq)]
pub struct EnchantmentDefinition {
    pub supported_items: HolderSet,
    pub primary_items: Option<HolderSet>,
    pub weight: i32,
    pub max_level: i32,
    pub min_cost: EnchantmentCost,
    pub max_cost: EnchantmentCost,
    pub anvil_cost: i32,
    pub slots: Vec<EquipmentSlotGroup>,
}
impl EnchantmentDefinition {
    pub fn decode(value: &Value) -> Result<Self> {
        let items = |value: &Value| -> Result<HolderSet> {
            let set = HolderSet::from_json(value)?;
            validate_static_holders(&set, "item", |id| Item::from_name(id).is_some())?;
            Ok(set)
        };
        Ok(Self {
            supported_items: items(codec::field(value, "supported_items")?)?,
            primary_items: value
                .get("primary_items")
                .filter(|value| !value.is_null())
                .map(items)
                .transpose()?,
            weight: bounded_int(value, "weight", 1, 1024)?,
            max_level: bounded_int(value, "max_level", 1, 255)?,
            min_cost: EnchantmentCost::decode(codec::field(value, "min_cost")?)?,
            max_cost: EnchantmentCost::decode(codec::field(value, "max_cost")?)?,
            anvil_cost: bounded_int(value, "anvil_cost", 0, i32::MAX)?,
            slots: codec::list(codec::field(value, "slots")?, |value| {
                Ok(EquipmentSlotGroup::from_json(value)?)
            })?,
        })
    }

    /// Port of Enchantment#matchingSlot and EquipmentSlotGroup#test. BODY is
    /// animal armor; SADDLE is a separate slot type and does not match ARMOR.
    pub fn matching_slot(&self, slot: EquipmentSlot) -> bool {
        self.slots.iter().any(|group| match group {
            EquipmentSlotGroup::Any => true,
            EquipmentSlotGroup::Mainhand => slot == EquipmentSlot::Mainhand,
            EquipmentSlotGroup::Offhand => slot == EquipmentSlot::Offhand,
            EquipmentSlotGroup::Hand => {
                matches!(slot, EquipmentSlot::Mainhand | EquipmentSlot::Offhand)
            }
            EquipmentSlotGroup::Feet => slot == EquipmentSlot::Feet,
            EquipmentSlotGroup::Legs => slot == EquipmentSlot::Legs,
            EquipmentSlotGroup::Chest => slot == EquipmentSlot::Chest,
            EquipmentSlotGroup::Head => slot == EquipmentSlot::Head,
            EquipmentSlotGroup::Armor => matches!(
                slot,
                EquipmentSlot::Feet
                    | EquipmentSlot::Legs
                    | EquipmentSlot::Chest
                    | EquipmentSlot::Head
                    | EquipmentSlot::Body
            ),
            EquipmentSlotGroup::Body => slot == EquipmentSlot::Body,
            EquipmentSlotGroup::Saddle => slot == EquipmentSlot::Saddle,
        })
    }

    /// Port of Enchantment#isSupportedItem / canEnchant. ItemStack#typeHolder
    /// observes AIR for a zero-count receiver, even when it retains a payload.
    pub fn is_supported_item(
        &self,
        stack: &ItemStack,
        facts: &dyn PredicateContext,
    ) -> Result<bool> {
        item_matches(&self.supported_items, stack, facts)
    }

    /// Port of Enchantment#isPrimaryItem, preserving supported-before-primary
    /// short circuiting and the optional primary set's native meaning.
    pub fn is_primary_item(&self, stack: &ItemStack, facts: &dyn PredicateContext) -> Result<bool> {
        if !self.is_supported_item(stack, facts)? {
            return Ok(false);
        }
        self.primary_items
            .as_ref()
            .map_or(Ok(true), |primary| item_matches(primary, stack, facts))
    }
}

fn item_matches(set: &HolderSet, stack: &ItemStack, facts: &dyn PredicateContext) -> Result<bool> {
    let item = stack.item();
    match set {
        HolderSet::Direct(items) => Ok(items.iter().any(|id| id.as_str() == item.name())),
        HolderSet::Named(id) => {
            let tags = facts.item_tags()?;
            let tag = tags.resolve_name(&id.as_str().parse()?).ok_or_else(|| {
                LootError::MissingReference {
                    registry: "item tag",
                    id: id.clone(),
                }
            })?;
            Ok(tags.contains(tag, item))
        }
    }
}
fn bounded_int(value: &Value, key: &str, min: i32, max: i32) -> Result<i32> {
    let number = codec::int(codec::field(value, key)?)?;
    if !(min..=max).contains(&number) {
        return Err(codec::error(format!("{key} must be in {min}..={max}")));
    }
    Ok(number)
}

#[cfg(test)]
mod tests;
