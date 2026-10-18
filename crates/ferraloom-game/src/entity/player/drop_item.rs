//! LivingEntity#drop and #createItemStackToDrop, using canonical entity storage.
use crate::{
    entity::{
        EntityData, EntityRef, EntityReference, EntityState, effects::EntityEffect,
        item::ItemEntity, player::inventory,
    },
    level::{Level, LevelError},
};
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::{Vec3, mth, random::RandomSource};

/// Port of LivingEntity#drop's server data mutation. Swing, dropped-item
/// statistics and prediction-aware animation delivery are ordered output hooks.
pub fn drop_item(
    player: EntityRef,
    stack: ItemStack,
    thrown_from_hand: bool,
    level: &mut Level,
) -> Result<Option<EntityRef>, LevelError> {
    if stack.is_empty() {
        return Ok(None);
    }
    let count = inventory::count(&stack);
    let kind = stack.0.as_ref().unwrap().item;
    let owner = level.entity::<EntityData>(player)?;
    let position = Vec3::new(
        owner.position.x,
        owner.position.y + f64::from(owner.eye_height) - 0.30000001192092896,
        owner.position.z,
    );
    let yaw = owner.yaw;
    let pitch = owner.pitch;
    let mut item = ItemEntity::new_at(level, position, stack);
    item.pickup_delay = 40;
    if thrown_from_hand {
        item.thrower = Some(EntityReference::of(player, level)?);
    }
    let owner = level.entity_mut::<EntityData>(player)?;
    let sin_x = mth::sin(f64::from(pitch * 0.017453292_f32));
    let cos_x = mth::cos(f64::from(pitch * 0.017453292_f32));
    let sin_y = mth::sin(f64::from(yaw * 0.017453292_f32));
    let cos_y = mth::cos(f64::from(yaw * 0.017453292_f32));
    let dir = owner.random.next_float() * mth::TWO_PI;
    let power = 0.02 * owner.random.next_float();
    item.parent.velocity = Vec3::new(
        f64::from(-sin_y * cos_x * 0.3) + f64::from(dir).cos() * f64::from(power),
        f64::from(
            -sin_x * 0.3 + 0.1 + (owner.random.next_float() - owner.random.next_float()) * 0.1,
        ),
        f64::from(cos_y * cos_x * 0.3) + f64::from(dir).sin() * f64::from(power),
    );
    let entity = level.next_entity_ref();
    level.add_entity(entity, EntityState::Item(Box::new(item)))?;
    level.emit_entity_effect(EntityEffect::ItemDrop {
        player,
        entity,
        item: kind,
        count,
        thrown_from_hand,
    });
    Ok(Some(entity))
}
