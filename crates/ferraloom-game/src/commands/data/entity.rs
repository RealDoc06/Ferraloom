use super::{DataError, error};
use crate::{
    entity::{EntityData, EntityRef, EntityState, load_in_place},
    level::Level,
};
use ferraloom_nbt::CompoundTag;

/// Port of EntityDataAccessor#setData. The accessor retains the receiver across
/// callbacks; UUID restoration follows a successful load and precedes the virtual.
pub fn set_entity_data(
    entity: EntityRef,
    tag: &CompoundTag,
    level: &mut Level,
) -> Result<(), DataError> {
    if matches!(level.entity_state(entity)?, EntityState::Player(_)) {
        return Err(error("commands.data.entity.invalid", &[]).into());
    }
    let uuid = level.entity::<EntityData>(entity)?.uuid;
    load_in_place(entity, tag, level)?;
    level.entity_mut::<EntityData>(entity)?.uuid = uuid;
    level
        .entity_behavior(entity)?
        .post_data_manipulated(entity, level)?;
    Ok(())
}

#[cfg(test)]
mod tests;
