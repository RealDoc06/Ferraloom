//! ItemStack's live durability mutation, shared by wear and conversion paths.
use crate::{
    entity::{EntityRef, player::inventory},
    inventory::{StackOperation, StackRef},
    item::stack::receiver_error,
    level::LevelError,
};
use ferraloom_protocol::{
    codec::VarInt,
    component::{ComponentTypeId, DataComponent, ItemStack, ItemStackContents},
};

/// Port of ItemStack#isDamageableItem. Component presence, rather than a
/// positive damage value, controls eligibility.
pub fn is_damageable_item(stack: &ItemStack) -> bool {
    inventory::component(stack, ComponentTypeId::MAX_DAMAGE).is_some()
        && inventory::component(stack, ComponentTypeId::UNBREAKABLE).is_none()
        && inventory::component(stack, ComponentTypeId::DAMAGE).is_some()
}
/// Port of ItemStack#getMaxDamage.
pub fn max_damage(stack: &ItemStack) -> i32 {
    match inventory::component(stack, ComponentTypeId::MAX_DAMAGE) {
        Some(DataComponent::MaxDamage(value)) => value.0,
        _ => 0,
    }
}
/// Port of ItemStack#getDamageValue. Mth#clamp remains well-defined for a
/// negative maximum supplied by a native component mutation.
pub fn damage_value(stack: &ItemStack) -> i32 {
    let damage = match inventory::component(stack, ComponentTypeId::DAMAGE) {
        Some(DataComponent::Damage(value)) => value.0,
        _ => 0,
    };
    damage.max(0).min(max_damage(stack))
}
/// Port of ItemStack#isBroken.
pub fn is_broken(stack: &ItemStack) -> bool {
    is_damageable_item(stack) && damage_value(stack) >= max_damage(stack)
}
/// Port of ItemStack#nextDamageWillBreak, with Java integer subtraction.
pub fn next_damage_will_break(stack: &ItemStack) -> bool {
    is_damageable_item(stack) && damage_value(stack) >= max_damage(stack).wrapping_sub(1)
}

/// Port of ItemStack#applyDamage. The criterion observes old damage and the
/// unclamped new value. Its synchronous rewards finish before any mutation or
/// reread, and the native broken copy is made before shrinking the receiver.
pub fn apply_damage(
    receiver: StackRef,
    new_damage: i32,
    player: Option<EntityRef>,
    operation: &mut StackOperation<'_>,
    mut on_break: impl FnMut(ItemStack, &mut StackOperation<'_>) -> Result<(), LevelError>,
) -> Result<(), LevelError> {
    if let Some(player) = player {
        crate::advancements::criteria::item_durability_changed(
            operation.level,
            player,
            receiver,
            new_damage,
        )?;
    }
    let stack = operation
        .level
        .stack_mut(receiver)
        .map_err(receiver_error)?;
    set_damage_value(stack, new_damage);
    if is_broken(stack) {
        let broken = stack.copy_with_count(inventory::count(stack));
        inventory::set_count(stack, inventory::count(stack).wrapping_sub(1));
        on_break(broken, operation)?;
    }
    Ok(())
}

/// Port of ItemStack#setDamageValue. The mutation targets the retained object,
/// including a payload whose count became zero during a criterion callback.
pub fn set_damage_value(stack: &mut ItemStack, value: i32) {
    let value = value.max(0).min(max_damage(stack));
    // EMPTY also owns a hidden PatchedDataComponentMap in vanilla. Retain that
    // write while AIR/count keeps every empty-stack component query empty.
    let contents = stack.0.get_or_insert_with(|| ItemStackContents {
        item: ferraloom_data::Item::AIR,
        count: 0,
        components: Default::default(),
    });
    contents
        .components
        .set(DataComponent::Damage(VarInt(value)));
    contents.components.sanitize(
        ferraloom_protocol::component::DataComponentMap::item_defaults(contents.item)
            .expect("generated vanilla item prototype"),
    );
}

#[cfg(test)]
mod tests;
