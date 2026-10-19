//! LivingEntity's jump strength and liquid impulses, before subclass side effects.
use crate::{
    entity::{Entity, EntityData, EntityRef, LivingEntity},
    level::{Level, LevelError},
};
use ferraloom_data::registries::{attribute::Id as Attribute, mob_effect::Id as MobEffect};
use ferraloom_util::{Vec3, java, mth};

/// Port of LivingEntity#getJumpBoostPower with native float amplifier arithmetic.
pub fn jump_boost_power(entity: EntityRef, level: &Level) -> Result<f32, LevelError> {
    Ok(level
        .entity::<LivingEntity>(entity)?
        .get_effect(MobEffect::JUMP_BOOST)
        .map_or(0.0, |effect| 0.1 * (effect.amplifier() as f32 + 1.0)))
}

/// Port of LivingEntity#getJumpPower(float). Attribute capture precedes the
/// required block query; the effect is read after that query returns.
pub fn jump_power<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    multiplier: f32,
    level: &Level,
) -> Result<f32, LevelError> {
    let power = super::attribute(entity, Attribute::JUMP_STRENGTH, level)? as f32 * multiplier;
    let factor = behavior.block_jump_factor(entity, level)?;
    Ok(power * factor + jump_boost_power(entity, level)?)
}

/// Port of LivingEntity#jumpFromGround. Subclasses own their subsequent stat
/// and exhaustion hooks; this parent body only mutates canonical movement.
pub fn jump_from_ground<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    let power = behavior.jump_power(entity, 1.0, level)?;
    if power <= 1.0e-5 {
        return Ok(());
    }
    {
        let base = level.entity_mut::<EntityData>(entity)?;
        base.velocity.y = java::max(f64::from(power), base.velocity.y);
        if base.shared_flag(3) {
            let angle = base.yaw * 0.017453292;
            base.velocity = base.velocity
                + Vec3::new(
                    f64::from(-mth::sin(f64::from(angle))) * 0.2,
                    0.0,
                    f64::from(mth::cos(f64::from(angle))) * 0.2,
                );
        }
        base.needs_sync = true;
    }
    Ok(())
}

/// Port of LivingEntity#jumpInLiquid; its tag argument does not affect the body.
pub fn jump_in_liquid(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let base = level.entity_mut::<EntityData>(entity)?;
    base.velocity = base.velocity + Vec3::new(0.0, 0.03999999910593033, 0.0);
    Ok(())
}

#[cfg(test)]
mod tests;
