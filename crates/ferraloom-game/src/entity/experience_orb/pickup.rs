//! ExperienceOrb#playerTouch and the recursive Mending repair path.
use super::ExperienceOrb;
use crate::{
    entity::{
        EntityRef,
        player::{ServerPlayer, give_experience_points},
    },
    inventory::StackOperation,
    item::{durability, enchantment::helper},
    level::{Level, LevelError},
};
use ferraloom_data::registries::enchantment_effect_component_type::Id as EffectComponent;

/// Native Mending uses the selected object throughout value-effect callbacks;
/// each recursive selection rereads the current canonical equipment.
fn repair_player_items(
    player: EntityRef,
    amount: i32,
    operation: &mut StackOperation<'_>,
) -> Result<i32, LevelError> {
    let Some(selected) = helper::get_random_item_with(
        EffectComponent::REPAIR_WITH_XP,
        player,
        |stack| durability::is_damageable_item(stack) && durability::damage_value(stack) > 0,
        operation,
    )?
    else {
        return Ok(amount);
    };
    let to_repair = helper::modify_durability_to_repair_from_xp(selected.item, amount, operation)?;
    let repair = to_repair.min(durability::damage_value(
        operation
            .level
            .stack(selected.item)
            .map_err(crate::item::stack::receiver_error)?,
    ));
    let stack = operation
        .level
        .stack_mut(selected.item)
        .map_err(crate::item::stack::receiver_error)?;
    let damage = durability::damage_value(stack);
    durability::set_damage_value(stack, damage.wrapping_sub(repair));
    if repair > 0 {
        let remaining = amount.wrapping_sub(repair.wrapping_mul(amount) / to_repair);
        if remaining > 0 {
            return repair_player_items(player, remaining, operation);
        }
    }
    Ok(0)
}

pub(super) fn player_touch(
    entity: EntityRef,
    player: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    if level.entity::<ServerPlayer>(player)?.take_xp_delay != 0 {
        return Ok(());
    }
    level.retain_entity(entity)?;
    if let Err(error) = level.retain_entity(player) {
        level.release_entity(entity);
        return Err(error);
    }
    let result = (|| {
        level.entity_mut::<ServerPlayer>(player)?.take_xp_delay = 2;
        level
            .entity_behavior(player)?
            .take(player, entity, 1, level)?;
        let amount = level.entity::<ExperienceOrb>(entity)?.value();
        let remaining = repair_player_items(player, amount, &mut StackOperation::new(level))?;
        if remaining > 0 {
            give_experience_points(player, remaining, level)?;
        }
        let orb = level.entity_mut::<ExperienceOrb>(entity)?;
        orb.count = orb.count.wrapping_sub(1);
        if orb.count == 0 {
            level.remove_entity(entity)?;
        }
        Ok(())
    })();
    level.release_entity(player);
    level.release_entity(entity);
    result
}
