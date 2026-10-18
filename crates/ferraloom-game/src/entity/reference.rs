//! Port of EntityReference: a UUID or a cached, typed canonical entity handle.
use super::{EntityData, EntityDataView, EntityRef};
use crate::level::{Level, LevelError};
use ferraloom_util::Uuid;
use std::marker::PhantomData;

#[derive(Clone, Copy, Debug)]
enum ReferenceValue {
    Uuid(Uuid),
    Cached(EntityRef),
}

/// A cached object remains held until a lookup observes its removal. Keeping
/// only its UUID would lose vanilla's lifetime and hidden-entity semantics.
#[derive(Debug)]
pub struct EntityReference<T: EntityDataView> {
    value: ReferenceValue,
    projection: PhantomData<fn() -> T>,
}
impl<T: EntityDataView> Copy for EntityReference<T> {}
impl<T: EntityDataView> Clone for EntityReference<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: EntityDataView> EntityReference<T> {
    pub fn unresolved(uuid: Uuid) -> Self {
        Self {
            value: ReferenceValue::Uuid(uuid),
            projection: PhantomData,
        }
    }

    pub fn of(entity: EntityRef, level: &Level) -> Result<Self, LevelError> {
        level.entity::<T>(entity)?;
        Ok(Self {
            value: ReferenceValue::Cached(entity),
            projection: PhantomData,
        })
    }

    /// Port of EntityReference#getUUID. Cached references read the object's
    /// current UUID, including after removal and before its cache is cleared.
    pub fn uuid(self, level: &Level) -> Result<Uuid, LevelError> {
        match self.value {
            ReferenceValue::Uuid(uuid) => Ok(uuid),
            ReferenceValue::Cached(entity) => Ok(level.entity::<EntityData>(entity)?.uuid),
        }
    }

    pub fn cached_entity(self) -> Option<EntityRef> {
        match self.value {
            ReferenceValue::Cached(entity) => Some(entity),
            ReferenceValue::Uuid(_) => None,
        }
    }

    /// Port of EntityReference#getEntity. A live cached object bypasses the
    /// visible UUID lookup; an invalidated cache may resolve to a replacement.
    pub fn resolve(&mut self, level: &Level) -> Result<Option<EntityRef>, LevelError> {
        if let ReferenceValue::Cached(entity) = self.value {
            let base = level.entity::<EntityData>(entity)?;
            if !base.removed {
                return Ok(Some(entity));
            }
            self.value = ReferenceValue::Uuid(base.uuid);
        }
        let ReferenceValue::Uuid(uuid) = self.value else {
            unreachable!("live cached entity already returned")
        };
        if let Some(entity) = level.entity_by_uuid(uuid)
            && !level.entity::<EntityData>(entity)?.removed
            && T::view(level.entity_state(entity)?).is_some()
        {
            self.value = ReferenceValue::Cached(entity);
            return Ok(Some(entity));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        entity::{
            EntityState, LivingEntity, RemovalReason,
            animal::Cow,
            item::{ItemEntity, PrimedTnt, item_entity, primed_tnt},
            player::{GameType, ServerPlayer, drop_item},
        },
        level::{Region, entity::Visibility},
    };
    use ferraloom_data::Item;
    use ferraloom_protocol::component::ItemStack;
    use ferraloom_util::{ChunkPos, Vec3};

    fn level() -> Level {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        )
    }

    #[test]
    fn vanilla_reference_reads_current_uuid_and_replaces_removed_typed_cache() {
        let mut level = level();
        let original = level.next_entity_ref();
        level
            .add_entity(original, EntityState::Cow(Box::default()))
            .unwrap();
        let mut reference = EntityReference::<LivingEntity>::of(original, &level).unwrap();
        let uuid = Uuid::new(3, 4);
        level.entity_mut::<EntityData>(original).unwrap().uuid = uuid;
        let current_uuid = reference.uuid(&level).unwrap() == uuid;
        assert_eq!(level.entity_by_uuid(uuid), None);
        let hidden = reference.resolve(&level).unwrap() == Some(original);
        level
            .remove_entity_with_reason(original, RemovalReason::Discarded)
            .unwrap();
        let replacement = level.next_entity_ref();
        let mut cow = Cow::default();
        cow.as_living_mut().parent.uuid = uuid;
        level
            .add_entity(replacement, EntityState::Cow(Box::new(cow)))
            .unwrap();
        level
            .update_entity_chunk_status(ChunkPos::ZERO, Visibility::Tracked)
            .unwrap();
        let replaced = reference.resolve(&level).unwrap() == Some(replacement);
        level
            .update_entity_chunk_status(ChunkPos::ZERO, Visibility::Hidden)
            .unwrap();
        let cached = reference.resolve(&level).unwrap() == Some(replacement);
        let wrong = level.next_entity_ref();
        let mut tnt = PrimedTnt::new(level.next_entity_id(), 123);
        let wrong_uuid = tnt.parent.uuid;
        tnt.parent.set_pos(Vec3::new(16.0, 64.0, 0.0));
        level
            .add_entity(wrong, EntityState::PrimedTnt(Box::new(tnt)))
            .unwrap();
        level
            .update_entity_chunk_status(ChunkPos::new(1, 0), Visibility::Tracked)
            .unwrap();
        let mut unresolved = EntityReference::<LivingEntity>::unresolved(wrong_uuid);
        let wrong_type = unresolved.resolve(&level).unwrap().is_none();
        assert_eq!(unresolved.cached_entity(), None);
        let actual = [current_uuid, hidden, replaced, cached, wrong_type]
            .map(|value| value.to_string())
            .join(",");
        let expected = include_str!("../../tests/java/entity/vectors.tsv")
            .lines()
            .find_map(|row| row.strip_prefix("entity-reference\t"))
            .unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn tnt_retains_hidden_owner_until_owner_lookup_observes_removal() {
        let mut level = level();
        let owner = level.next_entity_ref();
        level
            .add_entity(owner, EntityState::Cow(Box::default()))
            .unwrap();
        let uuid = level.entity::<EntityData>(owner).unwrap().uuid;
        let tnt = PrimedTnt::new_at(&mut level, Vec3::ZERO, Some(owner)).unwrap();
        let entity = level.next_entity_ref();
        level
            .add_entity(entity, EntityState::PrimedTnt(Box::new(tnt)))
            .unwrap();
        assert_eq!(level.entity_by_uuid(uuid), None);
        assert_eq!(
            primed_tnt::get_owner(entity, &mut level).unwrap(),
            Some(owner)
        );
        level
            .remove_entity_with_reason(owner, RemovalReason::Discarded)
            .unwrap();
        level.finish_entity_management();
        assert!(level.entity_state(owner).is_ok());
        assert_eq!(primed_tnt::get_owner(entity, &mut level).unwrap(), None);
        level.finish_entity_management();
        assert!(matches!(
            level.entity_state(owner),
            Err(LevelError::MissingEntity(_))
        ));
        let saved = level
            .entity::<PrimedTnt>(entity)
            .unwrap()
            .save_without_id(&level)
            .unwrap();
        assert_eq!(saved.get_int_array("owner").unwrap(), uuid.to_int_array());
    }

    #[test]
    fn dropped_item_retains_disconnected_thrower_until_owner_lookup() {
        let mut level = level();
        level.install_shared(Box::default());
        let player = level.next_entity_ref();
        let uuid = Uuid::new(1, 2);
        let state = ServerPlayer::new(
            uuid,
            "thrower".into(),
            level.next_entity_id(),
            Vec3::ZERO,
            GameType::Survival,
        );
        level
            .add_entity(player, EntityState::Player(Box::new(state)))
            .unwrap();
        let entity = drop_item(
            player,
            ItemStack::new(Item::STONE, 1, Default::default()),
            true,
            &mut level,
        )
        .unwrap()
        .unwrap();
        level
            .remove_entity_with_reason(player, RemovalReason::Discarded)
            .unwrap();
        level.finish_entity_management();
        assert_eq!(level.entity::<EntityData>(player).unwrap().uuid, uuid);
        assert_eq!(item_entity::get_owner(entity, &mut level).unwrap(), None);
        level.finish_entity_management();
        assert!(matches!(
            level.entity_state(player),
            Err(LevelError::MissingEntity(_))
        ));
        let saved = level
            .entity::<ItemEntity>(entity)
            .unwrap()
            .save_without_id(&level)
            .unwrap();
        assert_eq!(saved.get_int_array("Thrower").unwrap(), uuid.to_int_array());
    }
}
