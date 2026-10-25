//! LivingEntity's complete dirty-attribute observer foundation.
use crate::{
    entity::{Entity, EntityRef, LivingEntity},
    level::{Level, LevelError},
};
use ferraloom_data::registries::attribute::Id as Attribute;

/// Port of LivingEntity#refreshDirtyAttributes. Native handlers do not modify
/// dirty-set membership; retain its existing identity iteration and clear only
/// after all callbacks, leaving the completed prefix visible on failure.
/// Future live effects call this from onEffectUpdated(doRefreshAttributes)
/// before sendEffectToPassengers, and from onEffectsRemoved after its packets.
/// onEffectAdded only marks modifiers dirty; it does not call this method.
pub fn refresh_dirty_attributes(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let attributes: Vec<_> = level
        .entity::<LivingEntity>(entity)?
        .attributes
        .attributes_to_update()
        .map(|instance| instance.attribute())
        .collect();
    for attribute in attributes {
        level
            .entity_behavior(entity)?
            .on_attribute_updated(entity, attribute, level)?;
    }
    level
        .entity_mut::<LivingEntity>(entity)?
        .attributes
        .clear_attributes_to_update();
    Ok(())
}
/// Port of LivingEntity#onAttributeUpdated, retaining the passed derived
/// dispatcher for SCALE's virtual dimensions and the canonical absorption setter.
pub fn on_attribute_updated_parent<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    attribute: Attribute,
    level: &mut Level,
) -> Result<(), LevelError> {
    match attribute {
        Attribute::MAX_HEALTH => {
            let living = level.entity_mut::<LivingEntity>(entity)?;
            let max = living.max_health();
            if living.health() > max {
                living.set_health(max);
            }
        }
        Attribute::MAX_ABSORPTION => {
            let max = level.entity::<LivingEntity>(entity)?.max_absorption();
            if behavior.absorption_amount(entity, level)? > max {
                behavior.set_absorption_amount(entity, max, level)?;
            }
        }
        Attribute::SCALE => crate::entity::dimensions::refresh_dimensions(behavior, entity, level)?,
        Attribute::WAYPOINT_TRANSMIT_RANGE => {
            if super::attribute(entity, attribute, level)? > 0.0 {
                level.track_waypoint(entity)?;
            } else {
                level.untrack_waypoint(entity)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests;
