//! Proto-chunk entity insertion at FULL promotion, through canonical factories.
use super::Level;
use crate::entity::{
    EntityRef,
    persistence::{self, EntityPersistenceError},
};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::ChunkPos;

/// A failed promotion retains all owned input NBT. Unsupported factories are
/// checked before construction, so that error leaves IDs and canonical state
/// untouched and the pipeline can retain the proto chunk for a later retry.
#[derive(Debug)]
pub struct WorldgenEntityLoadError {
    pub pos: ChunkPos,
    pub entities: Vec<CompoundTag>,
    pub source: EntityPersistenceError,
}
impl std::fmt::Display for WorldgenEntityLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "proto-chunk entity load at {:?}: {}",
            self.pos, self.source
        )
    }
}
impl std::error::Error for WorldgenEntityLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

fn entity_type(tag: &CompoundTag) -> Option<ferraloom_data::EntityType> {
    let name = tag.get_string("id")?.to_string_lossy();
    let name = ferraloom_protocol::Identifier::parse(&name).ok()?;
    ferraloom_data::EntityType::from_name(name.as_str())
}
fn check_factories(tag: &CompoundTag) -> Result<(), EntityPersistenceError> {
    let Some(kind) = entity_type(tag) else {
        return Ok(());
    };
    if !persistence::has_entity_factory(kind) {
        return Err(EntityPersistenceError::UnsupportedType(kind.name().into()));
    }
    if let Some(passengers) = tag.get_list("Passengers") {
        for passenger in passengers.iter().filter_map(Tag::as_compound) {
            check_factories(passenger)?;
        }
    }
    Ok(())
}

impl Level {
    /// Port of ChunkStatusTasks#postLoadProtoChunk and
    /// PersistentEntitySectionManager#addWorldGenChunkEntities. Registration is
    /// pre-order and emits onCreated before passenger construction and riding.
    /// This does not mark persistent entity IO loaded or promote visibility.
    pub fn add_worldgen_chunk_entities(
        &mut self,
        pos: ChunkPos,
        entities: Vec<CompoundTag>,
    ) -> Result<(), WorldgenEntityLoadError> {
        if let Some(source) = entities.iter().find_map(|tag| check_factories(tag).err()) {
            return Err(WorldgenEntityLoadError {
                pos,
                entities,
                source,
            });
        }
        for tag in &entities {
            if let Err(source) = self.load_worldgen_entity_tree(tag.clone()) {
                return Err(WorldgenEntityLoadError {
                    pos,
                    entities,
                    source,
                });
            }
        }
        Ok(())
    }

    fn load_worldgen_entity_tree(
        &mut self,
        tag: CompoundTag,
    ) -> Result<Option<EntityRef>, EntityPersistenceError> {
        if entity_type(&tag).is_none() {
            eprintln!("Skipping entity with unknown or missing id while loading proto chunk");
            return Ok(None);
        }
        let passengers = tag.get_list("Passengers").cloned().unwrap_or_default();
        let (entity, state) = match persistence::load_entity_state(self, tag) {
            Ok(entity) => entity,
            Err(
                error @ (EntityPersistenceError::MissingType | EntityPersistenceError::Base(_)),
            ) => {
                eprintln!("Skipping entity while loading proto chunk: {error}");
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        if self
            .region()
            .entities
            .known_uuids
            .contains(&state.as_entity().uuid)
        {
            self.retain_unmanaged_entity(entity, state);
        } else {
            self.add_entity_with_origin(entity, state, false)?;
        }
        for passenger in passengers.iter().filter_map(Tag::as_compound) {
            if let Some(passenger) = self.load_worldgen_entity_tree(passenger.clone())? {
                crate::entity::start_riding(passenger, entity, true, self)?;
            }
        }
        Ok(Some(entity))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        entity::{EntityData, item::PrimedTnt},
        level::{
            Region,
            entity::{ChunkLoadStatus, EntityManagementEvent, Visibility},
        },
    };
    use ferraloom_nbt::ListTag;
    use ferraloom_util::{Uuid, Vec3};

    fn tag(kind: &str, uuid: Uuid) -> CompoundTag {
        let mut tag = CompoundTag::new();
        tag.insert("id", Tag::String(kind.into()));
        tag.insert("UUID", Tag::IntArray(uuid.to_int_array().to_vec()));
        tag.insert(
            "Pos",
            Tag::List(ListTag(vec![
                Tag::Double(1.5),
                Tag::Double(64.0),
                Tag::Double(1.5),
            ])),
        );
        tag
    }
    fn level() -> Level {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        Level::new(Region::new(&BEHAVIORS))
    }
    #[test]
    fn worldgen_registers_preorder_as_created_and_does_not_complete_entity_io() {
        let mut level = level();
        let pos = ChunkPos::new(0, 0);
        let mut root = tag("minecraft:tnt", Uuid::new(1, 1));
        root.insert("future", Tag::Int(7));
        root.insert(
            "Passengers",
            Tag::List(ListTag(vec![
                Tag::Compound(tag(":tnt", Uuid::new(1, 2))),
                Tag::Compound(tag("missing:unknown", Uuid::new(1, 3))),
            ])),
        );
        level
            .add_worldgen_chunk_entities(pos, vec![root, tag("minecraft:tnt", Uuid::new(1, 4))])
            .unwrap();
        assert_eq!(
            level.take_entity_management_events(),
            vec![
                EntityManagementEvent::Created(EntityRef(1)),
                EntityManagementEvent::Created(EntityRef(2)),
                EntityManagementEvent::Created(EntityRef(3))
            ]
        );
        assert_eq!(
            level.entity::<EntityData>(EntityRef(1)).unwrap().passengers,
            [EntityRef(2)]
        );
        assert_eq!(
            level.entity::<EntityData>(EntityRef(2)).unwrap().vehicle,
            Some(EntityRef(1))
        );
        assert_eq!(
            level.entity::<EntityData>(EntityRef(3)).unwrap().network_id,
            3
        );
        assert_eq!(
            level
                .entity::<PrimedTnt>(EntityRef(1))
                .unwrap()
                .parent
                .saved
                .get_int("future"),
            Some(7)
        );
        assert_eq!(level.entity_chunk_visibility(pos), Visibility::Hidden);
        assert_eq!(level.entity_chunk_load_status(pos), ChunkLoadStatus::Fresh);
        assert!(level.take_entity_storage_requests().is_empty());
    }
    #[test]
    fn unported_worldgen_factories_retain_owned_nbt_before_allocating() {
        let mut level = level();
        let pos = ChunkPos::new(2, -3);
        let mut root = tag("minecraft:tnt", Uuid::new(1, 1));
        root.insert(
            "Passengers",
            Tag::List(ListTag(vec![Tag::Compound(tag(
                "minecraft:zombie",
                Uuid::new(1, 2),
            ))])),
        );
        let entities = vec![root];
        let error = level
            .add_worldgen_chunk_entities(pos, entities.clone())
            .unwrap_err();
        assert_eq!(error.pos, pos);
        assert_eq!(error.entities, entities);
        assert!(
            matches!(error.source, EntityPersistenceError::UnsupportedType(name) if name == "minecraft:zombie")
        );
        assert!(level.take_entity_management_events().is_empty());
        assert_eq!(level.next_entity_id(), 1);
        assert_eq!(level.next_entity_ref(), EntityRef(1));
    }
    #[test]
    fn duplicate_worldgen_vehicle_retains_the_object_held_by_registered_passengers() {
        let mut level = level();
        let uuid = Uuid::new(1, 1);
        let pos = ChunkPos::new(0, 0);
        level
            .add_worldgen_chunk_entities(pos, vec![tag("minecraft:tnt", uuid)])
            .unwrap();
        let mut duplicate = tag("minecraft:tnt", uuid);
        duplicate.insert(
            "Passengers",
            Tag::List(ListTag(vec![Tag::Compound(tag(
                "minecraft:tnt",
                Uuid::new(1, 2),
            ))])),
        );
        level
            .add_worldgen_chunk_entities(pos, vec![duplicate])
            .unwrap();
        assert_eq!(
            level.entity::<EntityData>(EntityRef(3)).unwrap().vehicle,
            Some(EntityRef(2))
        );
        assert_eq!(
            level.entity::<EntityData>(EntityRef(2)).unwrap().position,
            Vec3::new(1.5, 64.0, 1.5)
        );
        assert_eq!(level.region().entities.known_uuids.len(), 2);
        assert_eq!(
            level.take_entity_management_events(),
            vec![
                EntityManagementEvent::Created(EntityRef(1)),
                EntityManagementEvent::Created(EntityRef(3))
            ]
        );
    }
}
