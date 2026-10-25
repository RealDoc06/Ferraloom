//! Mob parent bodies whose canonical inputs are supplied by their owning systems.
use crate::{
    entity::{EntityData, EntityRef},
    level::{Level, LevelError},
};
use ferraloom_util::Vec3;

/// Port of Mob#jumpInLiquid. The caller captures getNavigation().canFloat();
/// navigation owns that flag. Live mob dispatch waits for that canonical read.
pub fn jump_in_liquid(
    entity: EntityRef,
    can_float: bool,
    level: &mut Level,
) -> Result<(), LevelError> {
    if can_float {
        super::living::jump::jump_in_liquid(entity, level)
    } else {
        let entity = level.entity_mut::<EntityData>(entity)?;
        entity.set_delta_movement(entity.velocity + Vec3::new(0.0, 0.3, 0.0));
        Ok(())
    }
}

mod persistence;
pub use persistence::{DropChances, LeashAttachment, LeashData};
pub(crate) use persistence::{block_pos, default_variant};
