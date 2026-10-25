//! EntityType#loadEntityRecursive before any fresh-entity registration.
use super::{EntityPersistenceError, load_in_place};
use crate::{
    entity::{EntityRef, EntitySpawnRequest, start_riding},
    level::{Level, LevelError},
};
use ferraloom_data::EntityType;
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{Identifier, play::Difficulty};

#[cfg(test)]
mod tests;

/// Port of EntityType#loadEntityRecursive. The callback runs after full subclass
/// loading and before constructing passengers; each complete child then rides.
///
/// A returned root owns ONE counted retain, including its traced passenger
/// tree. The caller must release_entity on every exit after finalization and
/// try_add_fresh_entity_with_passengers. Lookup, sections, tracking and ticking
/// remain unpublished until that later add; canonical reads already work.
pub fn load_entity_recursive(
    tag: &CompoundTag,
    request: EntitySpawnRequest,
    level: &mut Level,
    mut post_load: impl FnMut(EntityRef, &mut Level) -> Result<(), LevelError>,
) -> Result<Option<EntityRef>, EntityPersistenceError> {
    load(tag, request, level, &mut post_load)
}

fn load(
    tag: &CompoundTag,
    request: EntitySpawnRequest,
    level: &mut Level,
    post_load: &mut impl FnMut(EntityRef, &mut Level) -> Result<(), LevelError>,
) -> Result<Option<EntityRef>, EntityPersistenceError> {
    let kind = tag
        .get_string("id")
        .and_then(|name| Identifier::parse(&name.to_string_lossy()).ok())
        .and_then(|name| EntityType::from_name(name.as_str()));
    let Some(kind) = kind else {
        eprintln!("Skipping entity with an unknown or missing id");
        return Ok(None);
    };
    if !request.ignore_checks
        && !kind.data().allowed_in_peaceful
        && level.difficulty() == Difficulty::Peaceful
    {
        return Ok(None);
    }
    // EntityTypes.PLAYER uses Builder#createNothing. This is a native null
    // factory, distinct from a real factory whose port is still required.
    if kind == EntityType::PLAYER {
        return Ok(None);
    }
    let entity = level.prepare_unmanaged_entity(kind)?;
    let result = (|| {
        match load_in_place(entity, tag, level) {
            Ok(()) => {}
            Err(error @ EntityPersistenceError::Base(_)) => {
                eprintln!("Exception loading entity: {error}");
                return Ok(None);
            }
            Err(error) => return Err(error),
        }
        post_load(entity, level)?;
        if let Some(passengers) = tag.get_list("Passengers") {
            for passenger_tag in passengers.iter().filter_map(Tag::as_compound) {
                if let Some(passenger) = load(passenger_tag, request, level, post_load)? {
                    let result = start_riding(passenger, entity, true, level);
                    level.release_entity(passenger);
                    result?;
                }
            }
        }
        Ok(Some(entity))
    })();
    if !matches!(result, Ok(Some(_))) {
        level.release_entity(entity);
    }
    result
}
