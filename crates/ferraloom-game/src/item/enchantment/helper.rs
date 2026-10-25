//! Native helper iteration and item-context value-effect execution.
use super::{ConditionalEffect, EnchantmentValueEffect};
use crate::{
    block::containers::loot_runtime::LiveLootRuntime,
    entity::player::inventory,
    inventory::{StackOperation, StackRef},
    item::stack::receiver_error,
    level::LevelError,
    loot::{Identifier, LootData, LootParams, LootRandomSelection, LootRuntime},
};
use ferraloom_protocol::component::{ComponentTypeId, DataComponent};

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
