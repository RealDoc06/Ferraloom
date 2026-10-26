//! Native helper iteration and item-context value-effect execution.
use super::{ConditionalEffect, EnchantmentValueEffect};
use crate::{
    block::containers::loot_runtime::LiveLootRuntime,
    entity::{
        EntityRef, LivingEntity,
        player::{ServerPlayer, equipment::SLOTS, inventory},
    },
    inventory::{ContainerHandle, ContainerSlot, StackOperation, StackRef},
    item::stack::receiver_error,
    level::LevelError,
    loot::{Identifier, LootData, LootParams, LootRandomSelection, LootRuntime},
};
use ferraloom_data::registries::enchantment_effect_component_type::Id as EffectComponent;
use ferraloom_protocol::component::{ComponentTypeId, DataComponent};
use ferraloom_protocol::component::{EquipmentSlot, ItemStack};
use ferraloom_util::random::RandomSource;

/// Port of EnchantedItemInUse's borrowed equipment receiver. The operation roots
/// the selected object, so effect callbacks cannot redirect it to another item.
#[derive(Clone, Copy, Debug)]
pub struct EnchantedItemInUse {
    pub item: StackRef,
    pub slot: EquipmentSlot,
    pub owner: EntityRef,
}

/// Port of EnchantmentHelper#getRandomItemWith. Each matching enchantment entry
/// adds a candidate, rather than deduplicating stacks or equipment slots.
pub fn get_random_item_with(
    component: EffectComponent,
    source: EntityRef,
    predicate: impl Fn(&ItemStack) -> bool,
    operation: &mut StackOperation<'_>,
) -> Result<Option<EnchantedItemInUse>, LevelError> {
    operation.level.entity::<LivingEntity>(source)?;
    let behavior = operation.level.entity_behavior(source)?;
    let mut candidates = Vec::new();
    for (index, slot) in SLOTS.into_iter().enumerate() {
        let stack = behavior
            .item_by_slot(source, slot, operation.level)?
            .expect("living equipment getter");
        if !predicate(stack) {
            continue;
        }
        let Some(DataComponent::Enchantments(enchantments)) =
            inventory::component(stack, ComponentTypeId::ENCHANTMENTS)
        else {
            continue;
        };
        if enchantments.0.is_empty() {
            continue;
        }
        let registry = operation.level.context().enchantments()?;
        // Native holder identity-map order is intentionally represented by the
        // component's stored order, as in run_iteration_on_item below.
        for (enchantment, _) in &enchantments.0 {
            let value = registry
                .get(&enchantment.0)
                .ok_or_else(|| missing_enchantment(&enchantment.0))?;
            if value.has_effect(component) && value.definition.matching_slot(slot) {
                let address = if slot == EquipmentSlot::Mainhand
                    && let Ok(player) = operation.level.entity::<ServerPlayer>(source)
                {
                    ContainerSlot {
                        container: ContainerHandle::PlayerInventory(source),
                        slot: player.inventory.selected_slot(),
                    }
                } else {
                    ContainerSlot {
                        container: ContainerHandle::LivingEquipment(source),
                        slot: index,
                    }
                };
                candidates.push(EnchantedItemInUse {
                    item: operation.level.stack_ref(address).map_err(receiver_error)?,
                    slot,
                    owner: source,
                });
            }
        }
    }
    if candidates.is_empty() {
        return Ok(None);
    }
    let index = operation
        .level
        .entity_mut::<LivingEntity>(source)?
        .random
        .next_int_bound(candidates.len() as i32) as usize;
    let mut selected = candidates[index];
    // The scan and nextInt have no mutable world callback; pin the chosen
    // object before returning to the caller that can execute effect callbacks.
    selected.item = operation
        .pin_existing(selected.item)
        .map_err(receiver_error)?;
    Ok(Some(selected))
}

fn missing_enchantment(id: &Identifier) -> LevelError {
    // TODO(5a): registry-aware component decoding rejects an unknown holder.
    LevelError::EnchantmentEffect {
        enchantment: id.clone(),
        source: "unknown enchantment registry holder".into(),
    }
}

/// Read the same matching-slot enchantments used by forEachModifier and
/// stopLocationBasedEffects. Absent or empty effect lists are proven no-ops;
/// nonempty unported cleanup remains required at the caller's native slot.
pub(crate) fn has_matching_effect(
    stack: &ItemStack,
    slot: EquipmentSlot,
    component: EffectComponent,
    level: &crate::level::Level,
) -> Result<bool, LevelError> {
    let Some(DataComponent::Enchantments(enchantments)) =
        inventory::component(stack, ComponentTypeId::ENCHANTMENTS)
    else {
        return Ok(false);
    };
    if enchantments.0.is_empty() {
        return Ok(false);
    }
    let registry = level.context().enchantments()?;
    for (id, _) in &enchantments.0 {
        let enchantment = registry
            .get(&id.0)
            .ok_or_else(|| missing_enchantment(&id.0))?;
        if enchantment.definition.matching_slot(slot) {
            match enchantment.effect(component) {
                None => {}
                Some(super::registry::EnchantmentEffect::Unported(value))
                    if value.as_array().is_some_and(Vec::is_empty) => {}
                Some(_) => return Ok(true),
            }
        }
    }
    Ok(false)
}

/// Port of EnchantmentHelper#processDurabilityChange. Conditional ITEM_DAMAGE
/// effects use Level.random and update a MutableFloat before Java's int cast.
pub fn process_durability_change(
    receiver: StackRef,
    amount: i32,
    operation: &mut StackOperation<'_>,
) -> Result<i32, LevelError> {
    let mut value = amount as f32;
    run_iteration_on_item(receiver, operation, |id, level, operation| {
        let registry = operation.level.context().enchantments()?.clone();
        let enchantment = registry.get(id).ok_or_else(|| missing_enchantment(id))?;
        let effects = match enchantment
            .value_effect(EffectComponent::ITEM_DAMAGE)
            .map_err(|error| LevelError::EnchantmentEffect {
                enchantment: id.clone(),
                source: error.to_string(),
            })? {
            Some(super::EnchantmentValueEffectComponent::ItemDamage(effects)) => effects.as_slice(),
            Some(_) => unreachable!("typed enchantment component key"),
            None => &[],
        };
        value = modify_item_filtered_count(id, effects, level, receiver, value, operation)?;
        Ok(())
    })?;
    Ok(value as i32)
}

/// Port of EnchantmentHelper#modifyDurabilityToRepairFromXp, preserving all
/// per-enchantment f32 updates before MutableFloat.intValue's saturated cast.
pub fn modify_durability_to_repair_from_xp(
    receiver: StackRef,
    amount: i32,
    operation: &mut StackOperation<'_>,
) -> Result<i32, LevelError> {
    let mut value = amount as f32;
    run_iteration_on_item(receiver, operation, |id, level, operation| {
        let registry = operation.level.context().enchantments()?.clone();
        let enchantment = registry.get(id).ok_or_else(|| missing_enchantment(id))?;
        let effects = match enchantment
            .value_effect(EffectComponent::REPAIR_WITH_XP)
            .map_err(|error| LevelError::EnchantmentEffect {
                enchantment: id.clone(),
                source: error.to_string(),
            })? {
            Some(super::EnchantmentValueEffectComponent::RepairWithXp(effects)) => {
                effects.as_slice()
            }
            Some(_) => unreachable!("typed enchantment component key"),
            None => &[],
        };
        value = modify_item_filtered_count(id, effects, level, receiver, value, operation)?;
        Ok(())
    })?;
    Ok((value as i32).max(0))
}

/// Port of EnchantmentHelper#runIterationOnItem without equipment-slot filtering.
pub fn run_iteration_on_item(
    receiver: StackRef,
    operation: &mut StackOperation<'_>,
    mut action: impl FnMut(&Identifier, i32, &mut StackOperation<'_>) -> Result<(), LevelError>,
) -> Result<(), LevelError> {
    let stack = operation.level.stack(receiver).map_err(receiver_error)?;
    let Some(DataComponent::Enchantments(enchantments)) =
        inventory::component(stack, ComponentTypeId::ENCHANTMENTS)
    else {
        return Ok(());
    };
    // Vanilla iterates an identity-hashed Holder.Reference map, whose order
    // varies between JVM runs. We deterministically retain the component's
    // stored entry order. Lists inside each enchantment retain native order.
    let entries = enchantments.0.clone();
    for (enchantment, level) in entries {
        action(&enchantment.0, level.0, operation)?;
    }
    Ok(())
}

/// Port of Enchantment#applyEffects for conditional value effects. Predicate
/// draws and effect draws share the caller's selected stream in list order.
pub fn apply_value_effects(
    effects: &[ConditionalEffect<EnchantmentValueEffect>],
    enchantment_level: i32,
    params: &LootParams,
    data: &LootData,
    runtime: &mut dyn LootRuntime,
    mut value: f32,
) -> crate::loot::Result<f32> {
    for effect in effects {
        if effect.matches_with_runtime(data, params, runtime)? {
            let mut processed = Ok(value);
            runtime.with_random(&mut |random| {
                processed = effect.effect.process(enchantment_level, random, value);
            });
            value = processed?;
        }
    }
    Ok(value)
}

/// Port of Enchantment#modifyItemFilteredCount and its itemContext. The owning
/// enchantment supplies the component's validated conditional-effect list.
pub fn modify_item_filtered_count(
    enchantment: &Identifier,
    effects: &[ConditionalEffect<EnchantmentValueEffect>],
    enchantment_level: i32,
    receiver: StackRef,
    value: f32,
    operation: &mut StackOperation<'_>,
) -> Result<f32, LevelError> {
    let stack = operation.level.stack(receiver).map_err(receiver_error)?;
    // Native itemContext captures TOOL here. Synchronous conditions only read
    // it, so an owned value is equivalent; the original receiver stays rooted
    // and durability rereads it after criteria and their synchronous rewards.
    let params = LootParams {
        tool: Some(stack.copy_with_count(inventory::count(stack))),
        enchantment_level: Some(enchantment_level),
        ..Default::default()
    };
    let resources = operation.level.container_resources()?;
    let mut runtime = LiveLootRuntime::new(operation.level, LootRandomSelection::Level);
    apply_value_effects(
        effects,
        enchantment_level,
        &params,
        &resources.loot,
        &mut runtime,
        value,
    )
    .map_err(|error| LevelError::EnchantmentEffect {
        enchantment: enchantment.clone(),
        source: error.to_string(),
    })
}

#[cfg(test)]
mod tests;
