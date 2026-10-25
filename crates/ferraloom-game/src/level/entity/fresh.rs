//! Native fresh registration of already constructed, canonical entity objects.
use super::{EntityManagementEvent, Level, LevelError, Visibility};
use crate::entity::{EntityData, EntityRef, EntityState, persistence::EntityPersistenceError};
use ferraloom_data::EntityType;

impl Level {
    pub(crate) fn prepare_unmanaged_entity(
        &mut self,
        kind: EntityType,
    ) -> Result<EntityRef, EntityPersistenceError> {
        let state = crate::entity::persistence::create_entity_state(self, kind)?;
        let entity = self.next_entity_ref();
        self.retain_unmanaged_entity(entity, state);
        self.retain_entity(entity)?;
        Ok(entity)
    }

    /// Port of ServerLevel#tryAddFreshEntityWithPassengers. The preflight only
    /// consults UUIDs already owned by the manager. Duplicates inside the new
    /// tree can fail their individual add after the preflight has succeeded.
    pub fn try_add_fresh_entity_with_passengers(
        &mut self,
        entity: EntityRef,
    ) -> Result<bool, LevelError> {
        if self.has_loaded_uuid_in_tree(entity)? {
            return Ok(false);
        }
        self.add_prepared_tree(entity)?;
        Ok(true)
    }

    fn has_loaded_uuid_in_tree(&self, entity: EntityRef) -> Result<bool, LevelError> {
        let base = self.entity::<EntityData>(entity)?;
        if self.region().entities.registration.knows_uuid(base.uuid) {
            return Ok(true);
        }
        for &passenger in &base.passengers {
            if self.has_loaded_uuid_in_tree(passenger)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn add_prepared_tree(&mut self, entity: EntityRef) -> Result<(), LevelError> {
        // getSelfAndPassengers captures this immutable passenger list when its
        // stream is built, before invoking addFreshEntity on the root.
        let passengers = self.entity::<EntityData>(entity)?.passengers.clone();
        self.add_prepared_entity(entity)?;
        for passenger in passengers {
            self.add_prepared_tree(passenger)?;
        }
        Ok(())
    }

    fn add_prepared_entity(&mut self, entity: EntityRef) -> Result<(), LevelError> {
        let storage = &self.region().entities;
        let state = self.entity_state(entity)?;
        let base = state.as_entity();
        if base.removed {
            eprintln!(
                "Tried to add an entity already marked removed: {}",
                base.entity_type.name()
            );
            return Ok(());
        }
        if !storage.registration.is_unmanaged(entity) {
            return Err(LevelError::DuplicateEntity(entity));
        }
        if !storage.registration.reserve_uuid(base.uuid) {
            eprintln!("UUID of a newly added entity already exists: {}", base.uuid);
            return Ok(());
        }
        let status = if matches!(state, EntityState::Player(_)) {
            Visibility::Ticking
        } else {
            storage.visibility(base.chunk_position)
        };
        if status.is_accessible() && !storage.lookup.add(base.network_id, base.uuid, entity) {
            storage.registration.forget_uuid(base.uuid);
            return Err(LevelError::DuplicateEntity(entity));
        }
        storage.sections.move_entity(entity, base.position, status);
        storage.registration.publish_unmanaged(entity);
        storage
            .registration
            .push_event(EntityManagementEvent::Created(entity));
        self.complete_entity_registration(entity, status, true)
    }
}
