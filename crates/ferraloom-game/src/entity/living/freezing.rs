//! Port of freeze eligibility and LivingEntity's powder-snow speed modifier.
use crate::{
    entity::{
        Entity, EntityData, EntityRef, LivingEntity, ai::attributes::AttributeModifier, movement,
    },
    level::{Level, LevelError},
};
use ferraloom_data::{
    registries::attribute::Id as Attribute,
    tags::{EntityTypeTags, ItemTags},
};
use ferraloom_protocol::{
    Identifier,
    component::{AttributeOperation, EquipmentSlot, JavaF64},
};
use std::sync::Arc;

/// Port of Entity#canFreeze. Required tag context is never replaced with a
/// copied list of freeze-immune entity families.
pub fn entity_can_freeze(entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
    Ok(!level.entity_type_tags()?.contains_key(
        EntityTypeTags::FREEZE_IMMUNE_ENTITY_TYPES,
        level.entity::<EntityData>(entity)?.entity_type,
    ))
}
/// Port of LivingEntity#canFreeze. ARMOR excludes both hands and the saddle,
/// includes body armor, and visits slots in EquipmentSlot declaration order.
pub fn can_freeze<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &Level,
) -> Result<bool, LevelError> {
    if behavior.is_spectator(entity, level)? {
        return Ok(false);
    }
    let living = level.entity::<LivingEntity>(entity)?;
    let tags = level.item_tags()?;
    for slot in [
        EquipmentSlot::Feet,
        EquipmentSlot::Legs,
        EquipmentSlot::Chest,
        EquipmentSlot::Head,
        EquipmentSlot::Body,
    ] {
        if tags.contains_key(
            ItemTags::FREEZE_IMMUNE_WEARABLES,
            living.equipment.get(slot).item(),
        ) {
            return Ok(false);
        }
    }
    entity_can_freeze(entity, level)
}
fn modifier_id() -> Identifier {
    "minecraft:powder_snow"
        .parse()
        .expect("native modifier identifier")
}
/// Port of LivingEntity#removeFrost. getAttribute creates the canonical lazy
/// instance even if no powder-snow modifier has been installed yet.
pub fn remove_frost(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    if let Some(mut speed) = level
        .entity_mut::<LivingEntity>(entity)?
        .attributes
        .get_instance(Attribute::MOVEMENT_SPEED)
    {
        speed.remove_modifier(&modifier_id());
    }
    Ok(())
}
/// Port of LivingEntity#tryAddFrost. The legacy supporting-block read precedes
/// the frozen-counter check; -0.05 is multiplied in float before widening.
pub fn try_add_frost(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let pos = movement::get_on_pos(entity, 0.2_f32, level)?;
    if level.get_block_state(pos)?.is_air() {
        return Ok(());
    }
    let base = level.entity::<EntityData>(entity)?;
    if base.ticks_frozen() <= 0 {
        return Ok(());
    }
    let amount = f64::from(-0.05_f32 * base.percent_frozen());
    if let Some(mut speed) = level
        .entity_mut::<LivingEntity>(entity)?
        .attributes
        .get_instance(Attribute::MOVEMENT_SPEED)
    {
        speed
            .add_transient_modifier(Arc::new(AttributeModifier {
                id: modifier_id(),
                amount: JavaF64(amount),
                operation: AttributeOperation::AddValue,
            }))
            .expect("native duplicate powder-snow modifier: removeFrost must precede tryAddFrost");
    }
    Ok(())
}

/// Port of LivingEntity#aiStep's server freezing phase. The supporting-block
/// frost query precedes the periodic typed-damage callback, including at zero.
pub fn tick_freezing<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    if !level.entity::<EntityData>(entity)?.is_in_powder_snow
        || !behavior.can_freeze(entity, level)?
    {
        let base = level.entity_mut::<EntityData>(entity)?;
        base.set_ticks_frozen(base.ticks_frozen().wrapping_sub(2).max(0));
    }
    remove_frost(entity, level)?;
    try_add_frost(entity, level)?;
    let base = level.entity::<EntityData>(entity)?;
    if base.tick_count % 40 == 0 && base.is_fully_frozen() && behavior.can_freeze(entity, level)? {
        let source = level.damage_sources().freeze();
        behavior.hurt_server(entity, level, source, 1.0)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
