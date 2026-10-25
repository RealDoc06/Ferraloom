//! Mutable and shared publication use one registration/load stream. The latter
//! capability cannot cross a mutable native tracking callback during acquisition.
use super::{EntityRef, EntityState, Level, LevelError, Visibility};

pub(super) enum EntityPublication<'a> {
    Mutable(&'a mut Level),
    Shared(&'a Level),
}
impl std::ops::Deref for EntityPublication<'_> {
    type Target = Level;
    fn deref(&self) -> &Level {
        match self {
            Self::Mutable(level) => level,
            Self::Shared(level) => level,
        }
    }
}
impl EntityPublication<'_> {
    pub(super) fn publish(
        &mut self,
        id: EntityRef,
        state: EntityState,
        loaded: bool,
        path: &'static str,
    ) -> Result<(), LevelError> {
        match self {
            Self::Mutable(level) => level.add_entity_with_origin(id, state, loaded),
            Self::Shared(level) => {
                let base = state.as_entity();
                // ServerLevel.EntityCallbacks#onCreated needs mutable access
                // even for hidden new transmitters; never buffer that callback.
                let creates_waypoint = if let Some(living) = state.as_living().filter(|_| !loaded) {
                    let attribute =
                        ferraloom_data::registries::attribute::Id::WAYPOINT_TRANSMIT_RANGE;
                    living.attributes.value(attribute).map_err(|_| {
                        LevelError::MissingEntityAttribute {
                            entity_type: base.entity_type,
                            attribute,
                        }
                    })? > 0.0
                } else {
                    false
                };
                if creates_waypoint
                    || base.entity_type == ferraloom_data::EntityType::PLAYER
                    || level
                        .entity_chunk_visibility(base.chunk_position)
                        .is_accessible()
                {
                    return Err(LevelError::UnsupportedSharedEntityTracking(path));
                }
                let status = level.publish_entity_data(id, state, loaded)?;
                debug_assert_eq!(status, Visibility::Hidden);
                Ok(())
            }
        }
    }
    pub(super) fn start_tracking(&mut self, entity: EntityRef) -> Result<(), LevelError> {
        match self {
            Self::Mutable(level) => level.start_entity_tracking(entity),
            Self::Shared(_) => Err(LevelError::UnsupportedSharedEntityTracking(
                "start_entity_tracking",
            )),
        }
    }
    pub(super) fn stop_tracking(&mut self, entity: EntityRef) -> Result<(), LevelError> {
        match self {
            Self::Mutable(level) => level.stop_entity_tracking(entity),
            Self::Shared(_) => Err(LevelError::UnsupportedSharedEntityTracking(
                "stop_entity_tracking",
            )),
        }
    }
}
