//! LivingEntity's equipment-break event and ordered location-effect cleanup.
use crate::{
    block::UnportedOperation,
    entity::{EntityMethod, EntityRef, LivingEntity, player::inventory::component},
    level::{Level, LevelError},
};
use ferraloom_data::registries::{
    attribute::Id as Attribute, data_component_type::Id as ComponentType,
};
use ferraloom_protocol::component::{DataComponent, EquipmentSlot, ItemStack};

/// Port of LivingEntity#onEquippedItemBroken's virtual entry, retaining the native copy.
pub fn on_equipped_item_broken(
    owner: EntityRef,
    broken: ItemStack,
    slot: EquipmentSlot,
    level: &mut Level,
) -> Result<(), LevelError> {
    level.entity::<LivingEntity>(owner)?;
    level
        .entity_behavior(owner)?
        .on_equipped_item_broken(owner, broken, slot, level)
}

/// Port of LivingEntity#entityEventForEquipmentBreak.
fn entity_event_for_equipment_break(slot: EquipmentSlot) -> i8 {
    match slot {
        EquipmentSlot::Mainhand => 47,
        EquipmentSlot::Offhand => 48,
        EquipmentSlot::Head => 49,
        EquipmentSlot::Chest => 50,
        EquipmentSlot::Legs => 51,
        EquipmentSlot::Feet => 52,
        EquipmentSlot::Body => 65,
        EquipmentSlot::Saddle => 68,
    }
}

/// Explicit LivingEntity parent body; ServerPlayer retains the same owned copy
/// while this borrow runs, then reads it for ITEM_BROKEN after cleanup.
pub(crate) fn on_equipped_item_broken_parent(
    owner: EntityRef,
    broken: &ItemStack,
    slot: EquipmentSlot,
    level: &mut Level,
) -> Result<(), LevelError> {
    level.entity::<LivingEntity>(owner)?;
    level.broadcast_entity_event(owner, entity_event_for_equipment_break(slot))?;
    stop_location_based_effects(owner, broken, slot, level)
}

/// Port of LivingEntity#stopLocationBasedEffects. Its per-item modifier loop
/// deliberately uses lazy getInstance, unlike AttributeMap#removeAttributeModifiers.
fn stop_location_based_effects(
    owner: EntityRef,
    broken: &ItemStack,
    slot: EquipmentSlot,
    level: &mut Level,
) -> Result<(), LevelError> {
    if let Some(DataComponent::AttributeModifiers(entries)) =
        component(broken, ComponentType::ATTRIBUTE_MODIFIERS)
    {
        for entry in entries.iter().filter(|entry| entry.slot.test(slot)) {
            // 5a registry-aware decoding will reject unresolved attribute holders earlier.
            let attribute = Attribute::from_name(entry.attribute.0.as_str())
                .ok_or(LevelError::Inventory("unresolved item modifier attribute"))?;
            if let Some(mut instance) = level
                .entity_mut::<LivingEntity>(owner)?
                .attributes
                .get_instance(attribute)
            {
                instance.remove_modifier(&entry.modifier.id);
            }
        }
    }
    use ferraloom_data::registries::enchantment_effect_component_type::Id as Effect;
    let kind = level.entity_state(owner)?.entity_type();
    // ItemStack#forEachModifier visits component callbacks before enchantments.
    if crate::item::enchantment::helper::has_matching_effect(
        broken,
        slot,
        Effect::ATTRIBUTES,
        level,
    )? {
        level.unported_operation(UnportedOperation::EntityMethod(
            kind,
            EntityMethod::EnchantmentAttributeModifiers,
        ))?;
    }
    if crate::item::enchantment::helper::has_matching_effect(
        broken,
        slot,
        Effect::LOCATION_CHANGED,
        level,
    )? {
        level.unported_operation(UnportedOperation::EntityMethod(
            kind,
            EntityMethod::StopLocationBasedEffects,
        ))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
