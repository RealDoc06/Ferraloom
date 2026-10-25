//! Proto-chunk entity insertion at FULL promotion, through canonical factories.
use super::{EntityPublication, Level};
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
    pub stage: WorldgenEntityLoadStage,
}
/// Only a preflight failure is safe to retry from the original proto-chunk NBT.
/// Once constructors start, IDs or registered roots may already be observable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldgenEntityLoadStage {
    Preflight,
    Publishing,
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

pub(super) fn entity_type(tag: &CompoundTag) -> Option<ferraloom_data::EntityType> {
    let name = tag.get_string("id")?.to_string_lossy();
    let name = ferraloom_protocol::Identifier::parse(&name).ok()?;
    ferraloom_data::EntityType::from_name(name.as_str())
}
pub(super) fn check_factories(
    tag: &CompoundTag,
    level: &Level,
) -> Result<(), EntityPersistenceError> {
    let Some(kind) = entity_type(tag) else {
        return Ok(());
    };
    if !persistence::has_entity_factory(kind) {
        return Err(EntityPersistenceError::UnsupportedType(kind.name().into()));
    }
    // This constructor's NBT default reads the live ANVIL tag. Validate the
    // required context before allocating or publishing any part of the stream.
    if kind == ferraloom_data::EntityType::FALLING_BLOCK {
        level.block_tags()?;
    }
    if let Some(passengers) = tag.get_list("Passengers") {
        for passenger in passengers.iter().filter_map(Tag::as_compound) {
            check_factories(passenger, level)?;
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
        EntityPublication::Mutable(self).publish_worldgen_entities(pos, entities)
    }
    /// Shared postLoad can publish hidden entities; a required tracking callback
    /// needs the pipeline's main-thread mutable continuation.
    pub fn add_worldgen_chunk_entities_shared(
        &self,
        pos: ChunkPos,
        entities: Vec<CompoundTag>,
    ) -> Result<(), WorldgenEntityLoadError> {
        EntityPublication::Shared(self).publish_worldgen_entities(pos, entities)
    }
}
impl EntityPublication<'_> {
    fn publish_worldgen_entities(
        &mut self,
        pos: ChunkPos,
        entities: Vec<CompoundTag>,
    ) -> Result<(), WorldgenEntityLoadError> {
        if let Some(source) = entities
            .iter()
            .find_map(|tag| check_factories(tag, self).err())
        {
            return Err(WorldgenEntityLoadError {
                pos,
                entities,
                source,
                stage: WorldgenEntityLoadStage::Preflight,
            });
        }
        for tag in &entities {
            if let Err(source) = self.load_worldgen_entity_tree(tag.clone()) {
                return Err(WorldgenEntityLoadError {
                    pos,
                    entities,
                    source,
                    stage: WorldgenEntityLoadStage::Publishing,
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
            .registration
            .knows_uuid(state.as_entity().uuid)
        {
            self.retain_unmanaged_entity(entity, state);
        } else {
            self.publish(entity, state, false, "add_worldgen_chunk_entities_shared")?;
        }
        for passenger in passengers.iter().filter_map(Tag::as_compound) {
            if let Some(passenger) = self.load_worldgen_entity_tree(passenger.clone())? {
                crate::entity::start_riding_loaded_shared(passenger, entity, self)?;
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
        Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        )
    }
    fn oracle(case: &str) -> Vec<&'static str> {
        include_str!("../../../tests/java/entity/vectors.tsv")
            .lines()
            .find(|line| line.starts_with(&format!("worldgen-entities\t{case}\t")))
            .unwrap()
            .split('\t')
            .collect()
    }
    fn created_ids(level: &Level, events: &[EntityManagementEvent]) -> Vec<String> {
        events
            .iter()
            .map(|event| {
                let EntityManagementEvent::Created(entity) = event else {
                    panic!("hidden worldgen entities only emit Created");
                };
                level
                    .entity::<EntityData>(*entity)
                    .unwrap()
                    .uuid
                    .least
                    .to_string()
            })
            .collect()
    }
    fn expected_created(fields: &[&str]) -> Vec<String> {
        fields[2]
            .split(',')
            .map(|event| event.split(':').next().unwrap().to_owned())
            .collect()
    }
    fn shared_oracle(case: &str) -> Vec<&'static str> {
        include_str!("../../../tests/java/entity/vectors.tsv")
            .lines()
            .find(|line| line.starts_with(&format!("worldgen-shared\t{case}\t")))
            .unwrap()
            .split('\t')
            .collect()
    }
    fn links(level: &Level) -> String {
        let mut states: Vec<_> = level
            .region()
            .entities
            .states
            .values()
            .map(|state| state.as_entity())
            .collect();
        states.sort_by_key(|base| base.network_id);
        states
            .into_iter()
            .map(|base| {
                let vehicle = base.vehicle.get().map_or_else(
                    || "none".to_owned(),
                    |entity| {
                        level
                            .entity::<EntityData>(entity)
                            .unwrap()
                            .network_id
                            .to_string()
                    },
                );
                let passengers = base
                    .passengers
                    .iter()
                    .map(|&entity| {
                        level
                            .entity::<EntityData>(entity)
                            .unwrap()
                            .network_id
                            .to_string()
                    })
                    .collect::<Vec<_>>()
                    .join("/");
                format!(
                    "{}:{}:{vehicle}:{passengers}:{}",
                    base.network_id, base.uuid.least, base.removed
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    }
    #[test]
    fn shared_worldgen_load_keeps_canonical_borrows_and_native_nested_duplicate_links() {
        let mut level = level();
        let pos = ChunkPos::new(0, 0);
        level
            .add_worldgen_chunk_entities_shared(pos, vec![tag("minecraft:tnt", Uuid::new(0, 99))])
            .unwrap();
        let anchor = level
            .entity::<EntityData>(EntityRef::fixture(level.region_id(), 1))
            .unwrap();
        let captured = anchor.passengers.as_slice();
        let mut first = tag("minecraft:tnt", Uuid::new(0, 2));
        first.insert(
            "Passengers",
            Tag::List(ListTag(vec![
                Tag::Compound(tag("minecraft:tnt", Uuid::new(0, 3))),
                Tag::Compound(tag("minecraft:tnt", Uuid::new(0, 4))),
            ])),
        );
        let mut duplicate = tag("minecraft:tnt", Uuid::new(0, 1));
        duplicate.insert(
            "Passengers",
            Tag::List(ListTag(vec![Tag::Compound(tag(
                "minecraft:tnt",
                Uuid::new(0, 5),
            ))])),
        );
        let mut root = tag("minecraft:tnt", Uuid::new(0, 1));
        root.insert(
            "Passengers",
            Tag::List(ListTag(vec![
                Tag::Compound(first),
                Tag::Compound(duplicate),
                Tag::Compound(tag("missing:unknown", Uuid::new(0, 7))),
            ])),
        );
        level
            .add_worldgen_chunk_entities_shared(
                pos,
                vec![root, tag("minecraft:tnt", Uuid::new(0, 6))],
            )
            .unwrap();
        let native = shared_oracle("nested");
        assert_eq!(links(&level), native[3]);
        assert_eq!(captured.len().to_string(), native[4]);
        assert!(std::ptr::eq(
            anchor,
            level
                .entity::<EntityData>(EntityRef::fixture(level.region_id(), 1))
                .unwrap()
        ));
        let events = level.take_entity_management_events();
        assert_eq!(created_ids(&level, &events), expected_created(&native));
        level.finish_entity_management();
        assert!(
            level
                .entity::<EntityData>(EntityRef::fixture(level.region_id(), 6))
                .is_ok()
        );
    }
    #[test]
    fn shared_worldgen_removed_item_mount_keeps_native_riding_links() {
        let level = level();
        let mut root = tag("minecraft:item", Uuid::new(0, 1));
        root.insert(
            "Passengers",
            Tag::List(ListTag(vec![Tag::Compound(tag(
                "minecraft:tnt",
                Uuid::new(0, 2),
            ))])),
        );
        level
            .add_worldgen_chunk_entities_shared(ChunkPos::new(0, 0), vec![root])
            .unwrap();
        let native = shared_oracle("removed-vehicle");
        assert_eq!(links(&level), native[3]);
        let events = level.take_entity_management_events();
        assert_eq!(created_ids(&level, &events), expected_created(&native));
    }
    #[test]
    fn mutable_worldgen_registration_publishes_tick_and_visible_indexes_immediately() {
        let mut level = level();
        let pos = ChunkPos::new(0, 0);
        level
            .update_entity_chunk_status(pos, Visibility::Ticking)
            .unwrap();
        let mut root = tag("minecraft:tnt", Uuid::new(1, 1));
        root.insert(
            "Passengers",
            Tag::List(ListTag(vec![Tag::Compound(tag(
                "minecraft:tnt",
                Uuid::new(1, 2),
            ))])),
        );
        level.add_worldgen_chunk_entities(pos, vec![root]).unwrap();
        assert_eq!(
            level.all_entities(),
            [
                EntityRef::fixture(level.region_id(), 1),
                EntityRef::fixture(level.region_id(), 2)
            ]
        );
        assert_eq!(
            level.region().entity_tick_list.snapshot(),
            [
                EntityRef::fixture(level.region_id(), 1),
                EntityRef::fixture(level.region_id(), 2)
            ]
        );
        assert_eq!(
            level.take_entity_management_events(),
            [
                EntityManagementEvent::Created(EntityRef::fixture(level.region_id(), 1)),
                EntityManagementEvent::TrackingStart(EntityRef::fixture(level.region_id(), 1)),
                EntityManagementEvent::TickingStart(EntityRef::fixture(level.region_id(), 1)),
                EntityManagementEvent::Created(EntityRef::fixture(level.region_id(), 2)),
                EntityManagementEvent::TrackingStart(EntityRef::fixture(level.region_id(), 2)),
                EntityManagementEvent::TickingStart(EntityRef::fixture(level.region_id(), 2)),
            ]
        );
        assert_eq!(
            level.entity_chunk_load_status(pos),
            ChunkLoadStatus::Pending
        );
    }
    #[test]
    fn shared_visible_publication_requires_the_mutable_tracker_continuation() {
        let mut level = level();
        let pos = ChunkPos::ZERO;
        level
            .update_entity_chunk_status(pos, Visibility::Tracked)
            .unwrap();
        let input = vec![tag("minecraft:tnt", Uuid::new(4, 5))];
        let error = level
            .add_worldgen_chunk_entities_shared(pos, input.clone())
            .unwrap_err();
        assert!(matches!(
            error.source,
            EntityPersistenceError::Level(
                crate::level::LevelError::UnsupportedSharedEntityTracking(
                    "add_worldgen_chunk_entities_shared"
                )
            )
        ));
        assert_eq!(error.entities, input);
        assert!(level.all_entities().is_empty());
        assert!(level.take_entity_management_events().is_empty());
    }
    #[test]
    fn shared_worldgen_required_context_fails_before_any_constructor() {
        let level = level();
        let pos = ChunkPos::new(0, 0);
        let entities = vec![
            tag("minecraft:tnt", Uuid::new(1, 1)),
            tag("minecraft:falling_block", Uuid::new(1, 2)),
        ];
        let error = level
            .add_worldgen_chunk_entities_shared(pos, entities.clone())
            .unwrap_err();
        assert!(matches!(
            error.source,
            EntityPersistenceError::Level(crate::level::LevelError::MissingBlockTags)
        ));
        assert_eq!(error.entities, entities);
        assert_eq!(error.stage, WorldgenEntityLoadStage::Preflight);
        assert_eq!(level.next_entity_id(), 1);
        assert_eq!(
            level.next_entity_ref(),
            EntityRef::fixture(level.region_id(), 1)
        );
        assert!(level.region().entities.states.is_empty());
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
        let events = level.take_entity_management_events();
        let native = oracle("preorder");
        assert_eq!(created_ids(&level, &events), expected_created(&native));
        let links = [
            EntityRef::fixture(level.region_id(), 1),
            EntityRef::fixture(level.region_id(), 2),
            EntityRef::fixture(level.region_id(), 3),
        ]
        .map(|id| {
            let base = level.entity::<EntityData>(id).unwrap();
            let vehicle = base.vehicle.get().map_or_else(
                || "none".to_owned(),
                |vehicle| {
                    level
                        .entity::<EntityData>(vehicle)
                        .unwrap()
                        .uuid
                        .least
                        .to_string()
                },
            );
            format!("{}:{}:{vehicle}", base.uuid.least, base.passengers.len())
        })
        .join(",");
        assert_eq!(links, native[3]);
        assert_eq!(level.are_entities_loaded(pos).to_string(), native[4]);
        assert_eq!(
            level
                .entity::<EntityData>(EntityRef::fixture(level.region_id(), 1))
                .unwrap()
                .passengers,
            [EntityRef::fixture(level.region_id(), 2)]
        );
        assert_eq!(
            level
                .entity::<EntityData>(EntityRef::fixture(level.region_id(), 2))
                .unwrap()
                .vehicle
                .get(),
            Some(EntityRef::fixture(level.region_id(), 1))
        );
        assert_eq!(
            level
                .entity::<EntityData>(EntityRef::fixture(level.region_id(), 3))
                .unwrap()
                .network_id,
            3
        );
        assert_eq!(
            level
                .entity::<PrimedTnt>(EntityRef::fixture(level.region_id(), 1))
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
    fn unknown_worldgen_roots_skip_their_entire_passenger_tree() {
        let mut level = level();
        let mut root = tag("missing:unknown", Uuid::new(1, 1));
        root.insert(
            "Passengers",
            Tag::List(ListTag(vec![Tag::Compound(tag(
                "minecraft:tnt",
                Uuid::new(1, 2),
            ))])),
        );
        level
            .add_worldgen_chunk_entities(ChunkPos::new(0, 0), vec![root])
            .unwrap();
        assert_eq!(
            level.take_entity_management_events().len().to_string(),
            oracle("unknown-root")[2]
        );
        assert_eq!(level.next_entity_id(), 1);
        assert_eq!(
            level.next_entity_ref(),
            EntityRef::fixture(level.region_id(), 1)
        );
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
        assert_eq!(error.stage, WorldgenEntityLoadStage::Preflight);
        assert!(
            matches!(error.source, EntityPersistenceError::UnsupportedType(name) if name == "minecraft:zombie")
        );
        assert!(level.take_entity_management_events().is_empty());
        assert_eq!(level.next_entity_id(), 1);
        assert_eq!(
            level.next_entity_ref(),
            EntityRef::fixture(level.region_id(), 1)
        );
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
            level
                .entity::<EntityData>(EntityRef::fixture(level.region_id(), 3))
                .unwrap()
                .vehicle
                .get(),
            Some(EntityRef::fixture(level.region_id(), 2))
        );
        assert_eq!(
            level
                .entity::<EntityData>(EntityRef::fixture(level.region_id(), 2))
                .unwrap()
                .position,
            Vec3::new(1.5, 64.0, 1.5)
        );
        assert_eq!(level.region().entities.registration.uuid_count(), 2);
        let events = level.take_entity_management_events();
        let native = oracle("duplicate");
        assert_eq!(created_ids(&level, &events), expected_created(&native));
        let child = level
            .entity::<EntityData>(EntityRef::fixture(level.region_id(), 3))
            .unwrap();
        let vehicle = child.vehicle.get().unwrap();
        assert_eq!(
            format!(
                "{}:{}",
                level.entity::<EntityData>(vehicle).unwrap().uuid.least,
                vehicle != EntityRef::fixture(level.region_id(), 1)
            ),
            native[3]
        );
    }
}
