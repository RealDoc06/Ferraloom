//! Mob's inherited control-facing virtual bodies, without a second controller
//! state or movement engine. Concrete limits remain subclass virtuals.
use crate::{
    entity::{Entity, EntityRef, LivingEntity},
    level::{Level, LevelError},
};

/// Port of Mob#setSpeed: LivingEntity's parent write is visible before the
/// virtual setZza callback. The receiver stays rooted through that callback.
pub fn set_speed<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    speed: f32,
    level: &mut Level,
) -> Result<(), LevelError> {
    level.retain_entity(entity)?;
    let result = (|| {
        level.entity_mut::<LivingEntity>(entity)?.set_speed(speed);
        behavior.set_zza(entity, speed, level)
    })();
    level.release_entity(entity);
    result
}

#[cfg(test)]
mod tests;
