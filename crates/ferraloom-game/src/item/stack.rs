//! Handle-based ItemStack operations. Native copy() capture points create values;
//! mutable use receivers always remain in the caller's canonical operation.
use crate::{
    block::{InteractionResult, UnportedOperation, UseOnContext},
    entity::{EntityRef, EntityState, player::inventory},
    inventory::{MenuError, StackOperation, StackRef},
    level::{Level, LevelError},
};
use ferraloom_data::{Item, ItemMethod};
use ferraloom_protocol::component::{ComponentTypeId, DataComponent};

/// Port of ItemStack#useOn. Receiver and context.getItemInHand are distinct
/// captured objects. Every returned stack handle is scoped to this operation.
pub fn use_on(
    receiver: StackRef,
    context: &UseOnContext<'_>,
    operation: &mut StackOperation<'_>,
) -> Result<Option<InteractionResult>, LevelError> {
    let stack = operation.level.stack(receiver).map_err(receiver_error)?;
    let item = stack.item();
    if let Some(player) = context.player()
        && !operation
            .level
            .entity::<crate::entity::player::ServerPlayer>(player)?
            .may_build()
    {
        if inventory::component(stack, ComponentTypeId::CAN_PLACE_ON).is_none() {
            return Ok(Some(InteractionResult::Pass));
        }
        // AdventureModePredicate's cached BlockInWorld/BE receiver is not yet
        // ported. Do not substitute a fresh predicate evaluation for that cache.
        return unported_use_on(item, operation);
    }
    let before = stack.copy_with_count(inventory::count(stack));
    let Some(behavior) = super::ported_behavior(item, ItemMethod::UseOn) else {
        return unported_use_on(item, operation);
    };
    let Some(mut result) = behavior.use_on(item, receiver, context, operation)? else {
        return Ok(None);
    };
    if let Some(player) = context.player()
        && let InteractionResult::Success { item_context, .. } = &mut result
        && item_context.was_item_interaction
    {
        crate::stats::award_stat(operation.level, player, crate::stats::Stat::Used(item), 1)?;
        let transformed = item_context.held_item_transformed_to.unwrap_or(receiver);
        let used = operation.level.stack(transformed).map_err(receiver_error)?;
        if inventory::component(&before, ComponentTypeId::USE_REMAINDER).is_some()
            && !has_infinite_materials(Some(player), operation.level)?
            && inventory::count(used) < inventory::count(&before)
        {
            // The native fresh template and extra-created-item callback belong
            // to the consumables slice. Preserve the call site until it lands.
            return unported_use_on(item, operation);
        }
        if let Some(DataComponent::UseCooldown(cooldown)) =
            inventory::component(&before, ComponentTypeId::USE_COOLDOWN)
        {
            crate::entity::player::cooldowns::add_cooldown(
                player,
                &before,
                (cooldown.seconds.0 * 20.0) as i32,
                operation.level,
            )?;
        }
        if item_context.held_item_transformed_to.is_none() {
            // An existing receiver is retained by the outer complete-use frame;
            // producers of fresh results already root them with operation.own.
            operation
                .pin_existing(transformed)
                .map_err(receiver_error)?;
        }
        item_context.held_item_transformed_to = Some(transformed);
    }
    Ok(Some(result))
}

/// Port of ItemStack#consume. The actual owner ability is reread after callbacks.
pub fn consume(
    receiver: StackRef,
    amount: i32,
    owner: Option<EntityRef>,
    operation: &mut StackOperation<'_>,
) -> Result<(), LevelError> {
    if !has_infinite_materials(owner, operation.level)? {
        let stack = operation
            .level
            .stack_mut(receiver)
            .map_err(receiver_error)?;
        inventory::set_count(stack, inventory::count(stack).wrapping_sub(amount));
    }
    Ok(())
}

/// Port of ItemStack#causeUseVibration; the post-use receiver decides whether
/// USE_EFFECTS is present, then the causer's current position supplies the event.
pub fn cause_use_vibration(
    receiver: StackRef,
    causer: Option<EntityRef>,
    event: ferraloom_data::registries::game_event::Id,
    operation: &mut StackOperation<'_>,
) -> Result<(), LevelError> {
    let stack = operation.level.stack(receiver).map_err(receiver_error)?;
    if matches!(inventory::component(stack, ComponentTypeId::USE_EFFECTS), Some(DataComponent::UseEffects(effects)) if effects.interact_vibrations)
    {
        let causer = causer.ok_or(LevelError::Inventory("item vibration causer"))?;
        let position = operation
            .level
            .entity::<crate::entity::EntityData>(causer)?
            .position;
        operation
            .level
            .game_event(event, position, Some(causer), None);
    }
    Ok(())
}

fn has_infinite_materials(owner: Option<EntityRef>, level: &Level) -> Result<bool, LevelError> {
    if let Some(owner) = owner {
        level.entity::<crate::entity::LivingEntity>(owner)?;
    }
    Ok(
        match owner.map(|owner| level.entity_state(owner)).transpose()? {
            Some(EntityState::Player(player)) => player.instabuild(),
            _ => false,
        },
    )
}

pub(super) fn unported_use_on(
    item: Item,
    operation: &mut StackOperation<'_>,
) -> Result<Option<InteractionResult>, LevelError> {
    operation
        .level
        .unported_operation(UnportedOperation::ItemUseOn(item))?;
    Ok(None)
}

pub(super) fn receiver_error(error: MenuError) -> LevelError {
    match error {
        MenuError::Level(error) => error,
        _ => LevelError::Inventory("item use receiver"),
    }
}

pub(super) fn item_data_error(item: Item, error: ferraloom_protocol::Error) -> LevelError {
    LevelError::ItemData {
        item,
        source: error.to_string(),
    }
}

#[cfg(test)]
pub(in crate::item) mod tests;
