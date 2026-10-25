use super::*;
use crate::{
    Behaviors,
    entity::{EntityState, experience_orb::ExperienceOrb},
    level::{Region, entity::EntityManagementEvent},
};
use ferraloom_nbt::{ListTag, Tag};
use ferraloom_util::{Uuid, Vec3};

fn level() -> Level {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    Level::new(
        Region::new(&BEHAVIORS),
        crate::level::LevelContext::fixture(0, 63),
    )
}
fn orb_tag(number: i64, position: Vec3) -> CompoundTag {
    let mut orb = ExperienceOrb::new(1, number);
    orb.parent.uuid = Uuid::new(0, number);
    orb.parent.set_pos(position);
    let mut tag = orb.save_without_id();
    tag.insert("id", Tag::String("minecraft:experience_orb".into()));
    tag
}
fn chunk(pos: ChunkPos, entities: Vec<CompoundTag>) -> CompoundTag {
    EntityChunk {
        data_version: Some(ferraloom_storage::DATA_VERSION),
        position: Some(IntArray(vec![pos.x, pos.z])),
        entities: Some(entities),
        ..EntityChunk::default()
    }
    .write()
    .unwrap()
}
fn load_request(level: &mut Level) -> (EntityLoadRequestId, ChunkPos) {
    let requests = level.take_entity_storage_requests();
    assert_eq!(requests.len(), 1);
    let EntityStorageRequest::Load { id, pos } = requests[0] else {
        panic!("expected a load request");
    };
    (id, pos)
}

#[test]
fn mutable_status_and_io_queues_preserve_native_events_and_canonical_state() {
    use crate::entity::player::{GameType, ServerPlayer};
    let mut level = level();
    let pos = ChunkPos::ZERO;
    level
        .add_worldgen_chunk_entities_shared(
            pos,
            vec![
                orb_tag(1, Vec3::new(1.5, 64.0, 1.5)),
                orb_tag(2, Vec3::new(1.5, 64.0, 1.5)),
            ],
        )
        .unwrap();
    let player = level.next_entity_ref();
    let state = ServerPlayer::new(
        Uuid::new(0, 3),
        "visibility".into(),
        level.next_entity_id(),
        Vec3::new(1.5, 64.0, 1.5),
        GameType::Survival,
    );
    level
        .add_entity(player, EntityState::Player(Box::new(state)))
        .unwrap();
    let shared = &mut level;
    let captured_address = std::ptr::from_ref(shared.entity::<EntityData>(EntityRef(1)).unwrap());
    shared.take_entity_management_events();
    let mut count = 0;
    for row in include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|row| row.starts_with("entity-visibility\t"))
    {
        let values: Vec<_> = row.split('\t').collect();
        let status = match values[1] {
            "TRACKED" => Visibility::Tracked,
            "TICKING" => Visibility::Ticking,
            "HIDDEN" => Visibility::Hidden,
            _ => panic!("unknown visibility"),
        };
        shared.update_entity_chunk_status(pos, status).unwrap();
        let events: Vec<_> = shared
            .take_entity_management_events()
            .into_iter()
            .map(|event| match event {
                EntityManagementEvent::TrackingStart(id) => format!("TrackingStart:{}", id.0),
                EntityManagementEvent::TrackingEnd(id) => format!("TrackingEnd:{}", id.0),
                EntityManagementEvent::TickingStart(id) => format!("TickingStart:{}", id.0),
                EntityManagementEvent::TickingEnd(id) => format!("TickingEnd:{}", id.0),
                _ => panic!("unexpected status event {event:?}"),
            })
            .collect();
        assert_eq!(events.join(","), values[2]);
        assert_eq!(
            shared
                .all_entities()
                .iter()
                .map(|id| id.0.to_string())
                .collect::<Vec<_>>()
                .join(","),
            values[3]
        );
        assert_eq!(
            shared.entity::<EntityData>(EntityRef(1)).unwrap().uuid,
            Uuid::new(0, 1)
        );
        assert_eq!(
            shared
                .entity::<EntityData>(EntityRef(1))
                .unwrap()
                .tick_count,
            0
        );
        count += 1;
    }
    assert_eq!(count, 5);
    assert_eq!(
        captured_address,
        std::ptr::from_ref(shared.entity::<EntityData>(EntityRef(1)).unwrap())
    );
    let requests = shared.take_entity_storage_requests();
    assert_eq!(requests.len(), 1);
    let EntityStorageRequest::Load { id, pos: loaded } = requests[0] else {
        panic!("expected load")
    };
    assert_eq!(loaded, pos);
    shared.complete_entity_chunk_load(id, Ok(None)).unwrap();
    assert!(matches!(
        shared.complete_entity_chunk_load(id, Ok(None)),
        Err(EntityStorageError::DuplicateLoadResponse(_))
    ));
    assert_eq!(
        shared.entity_chunk_load_status(pos),
        ChunkLoadStatus::Pending
    );
    assert!(shared.take_entity_storage_requests().is_empty());
    shared.process_pending_entity_loads().unwrap();
    assert_eq!(
        shared.entity_chunk_load_status(pos),
        ChunkLoadStatus::Loaded
    );
    shared
        .update_entity_chunk_status(pos, Visibility::Hidden)
        .unwrap();
    shared.process_pending_entity_loads().unwrap();
    assert!(
        !shared.entity::<EntityData>(EntityRef(1)).unwrap().removed,
        "load integration does not run unload management"
    );
    assert_eq!(
        shared
            .entity::<EntityData>(EntityRef(1))
            .unwrap()
            .tick_count,
        0
    );
}

#[test]
fn load_completion_order_does_not_change_entity_creation_or_callbacks() {
    let mut level = level();
    let a = ChunkPos::new(0, 0);
    let b = ChunkPos::new(1, 0);
    assert!(!level.are_entities_loaded(a));
    level
        .update_entity_chunk_status(a, Visibility::Ticking)
        .unwrap();
    let (first, _) = load_request(&mut level);
    assert!(!level.are_entities_loaded(a));
    level
        .update_entity_chunk_status(b, Visibility::Ticking)
        .unwrap();
    let (second, _) = load_request(&mut level);
    level
        .complete_entity_chunk_load(
            second,
            Ok(Some(chunk(b, vec![orb_tag(2, Vec3::new(17.0, 64.0, 1.0))]))),
        )
        .unwrap();
    level.process_pending_entity_loads().unwrap();
    assert!(level.all_entities().is_empty());
    assert_eq!(level.entity_chunk_load_status(b), ChunkLoadStatus::Pending);
    assert!(!level.are_entities_loaded(b));
    level
        .complete_entity_chunk_load(
            first,
            Ok(Some(chunk(a, vec![orb_tag(1, Vec3::new(1.0, 64.0, 1.0))]))),
        )
        .unwrap();
    level.process_pending_entity_loads().unwrap();
    let entities = level.all_entities();
    assert!(level.are_entities_loaded(a));
    assert!(level.are_entities_loaded(b));
    assert_eq!(entities.len(), 2);
    for (index, id) in entities.iter().enumerate() {
        let base = level.entity::<EntityData>(*id).unwrap();
        assert_eq!(base.network_id, index as i32 + 1);
        assert_eq!(base.uuid, Uuid::new(0, index as i64 + 1));
    }
    assert_eq!(
        level.take_entity_management_events(),
        vec![
            EntityManagementEvent::TrackingStart(entities[0]),
            EntityManagementEvent::TickingStart(entities[0]),
            EntityManagementEvent::TrackingStart(entities[1]),
            EntityManagementEvent::TickingStart(entities[1]),
        ]
    );
}

#[test]
fn fresh_chunk_loads_before_saving_new_and_existing_entities() {
    let mut level = level();
    let mut orb = ExperienceOrb::new(level.next_entity_id(), 1);
    orb.parent.uuid = Uuid::new(0, 1);
    orb.parent.set_pos(Vec3::new(1.0, 64.0, 1.0));
    let new = level.next_entity_ref();
    level
        .add_entity(new, EntityState::ExperienceOrb(Box::new(orb)))
        .unwrap();
    level.auto_save_entities().unwrap();
    let (request, pos) = load_request(&mut level);
    assert_eq!(
        level.entity_chunk_load_status(pos),
        ChunkLoadStatus::Pending
    );
    level.auto_save_entities().unwrap();
    assert!(level.take_entity_storage_requests().is_empty());
    let mut saved = chunk(pos, vec![orb_tag(2, Vec3::new(2.0, 64.0, 1.0))]);
    saved.insert("future_chunk_field", Tag::Int(42));
    level
        .complete_entity_chunk_load(request, Ok(Some(saved)))
        .unwrap();
    level.process_pending_entity_loads().unwrap();
    level.auto_save_entities().unwrap();
    let requests = level.take_entity_storage_requests();
    let [
        EntityStorageRequest::Store {
            chunk: Some(saved), ..
        },
    ] = requests.as_slice()
    else {
        panic!("expected one complete save");
    };
    let tags = saved.entities.as_ref().unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(
        tags[0].get_int_array("UUID").unwrap(),
        Uuid::new(0, 1).to_int_array()
    );
    assert_eq!(
        tags[1].get_int_array("UUID").unwrap(),
        Uuid::new(0, 2).to_int_array()
    );
    assert_eq!(saved.extra.get_int("future_chunk_field"), Some(42));
    assert!(level.entity::<EntityData>(new).unwrap().removed);
    assert_eq!(level.entity_chunk_load_status(pos), ChunkLoadStatus::Fresh);
    level.finish_entity_management();
    assert!(level.entity::<EntityData>(new).is_err());
}

#[test]
fn recursive_passengers_load_before_callbacks_and_save_vehicle_horizontal_position() {
    let mut level = level();
    let pos = ChunkPos::new(0, 0);
    level
        .update_entity_chunk_status(pos, Visibility::Ticking)
        .unwrap();
    let (request, _) = load_request(&mut level);
    let mut root = orb_tag(1, Vec3::new(1.0, 64.0, 2.0));
    let mut rider = orb_tag(2, Vec3::new(7.0, 65.0, 8.0));
    rider.insert("future_entity_field", Tag::String("kept".into()));
    root.insert("Passengers", Tag::List(ListTag(vec![Tag::Compound(rider)])));
    level
        .complete_entity_chunk_load(request, Ok(Some(chunk(pos, vec![root]))))
        .unwrap();
    level.process_pending_entity_loads().unwrap();
    let entities = level.all_entities();
    let (root, passenger) = (entities[0], entities[1]);
    assert_eq!(
        level.entity::<EntityData>(root).unwrap().passengers,
        vec![passenger]
    );
    assert_eq!(
        level.entity::<EntityData>(passenger).unwrap().vehicle.get(),
        Some(root)
    );
    assert!(level.save_entity(passenger).unwrap().is_none());
    assert!(!level.entity_should_be_saved(passenger).unwrap());
    let saved = level.save_entity(root).unwrap().unwrap();
    let passenger_tag = saved.get_list("Passengers").unwrap()[0]
        .as_compound()
        .unwrap();
    let position = passenger_tag.get_list("Pos").unwrap();
    assert_eq!(position[0].as_double(), Some(1.0));
    assert_eq!(position[1].as_double(), Some(65.0));
    assert_eq!(position[2].as_double(), Some(2.0));
    assert_eq!(
        passenger_tag
            .get_string("future_entity_field")
            .unwrap()
            .to_string_lossy(),
        "kept"
    );
    level
        .update_entity_chunk_status(pos, Visibility::Hidden)
        .unwrap();
    level.tick_entity_management().unwrap();
    assert!(level.all_entities().is_empty());
    assert!(level.entity::<EntityData>(root).is_err());
    assert!(level.entity::<EntityData>(passenger).is_err());
    let requests = level.take_entity_storage_requests();
    let [
        EntityStorageRequest::Store {
            chunk: Some(chunk), ..
        },
    ] = requests.as_slice()
    else {
        panic!("expected the saved root tree");
    };
    assert_eq!(chunk.entities.as_ref().unwrap().len(), 1);
}

#[test]
fn known_empty_chunks_avoid_redundant_writes_and_reads() {
    let mut level = level();
    let pos = ChunkPos::new(0, 0);
    level
        .update_entity_chunk_status(pos, Visibility::Tracked)
        .unwrap();
    let (request, _) = load_request(&mut level);
    level.complete_entity_chunk_load(request, Ok(None)).unwrap();
    level.tick_entity_management().unwrap();
    level.auto_save_entities().unwrap();
    assert!(level.take_entity_storage_requests().is_empty());
    level
        .update_entity_chunk_status(pos, Visibility::Hidden)
        .unwrap();
    level.tick_entity_management().unwrap();
    level
        .update_entity_chunk_status(pos, Visibility::Tracked)
        .unwrap();
    assert!(level.take_entity_storage_requests().is_empty());
    level.tick_entity_management().unwrap();
    assert_eq!(level.entity_chunk_load_status(pos), ChunkLoadStatus::Loaded);
}

#[test]
fn save_all_waits_for_pending_load_and_keeps_unported_chunks_pending() {
    let mut level = level();
    let pos = ChunkPos::new(0, 0);
    level
        .update_entity_chunk_status(pos, Visibility::Ticking)
        .unwrap();
    let (request, _) = load_request(&mut level);
    let mut orb = ExperienceOrb::new(level.next_entity_id(), 1);
    orb.parent.set_pos(Vec3::new(1.0, 64.0, 1.0));
    let id = level.next_entity_ref();
    level
        .add_entity(id, EntityState::ExperienceOrb(Box::new(orb)))
        .unwrap();
    level.begin_entity_save_all().unwrap();
    assert!(!level.poll_entity_save_all().unwrap());
    assert!(level.take_entity_storage_requests().is_empty());
    let mut unported = orb_tag(2, Vec3::ZERO);
    unported.insert("id", Tag::String("minecraft:pig".into()));
    level
        .complete_entity_chunk_load(request, Ok(Some(chunk(pos, vec![unported]))))
        .unwrap();
    let Err(EntityStorageError::LoadIntegration(failure)) = level.poll_entity_save_all() else {
        panic!("expected explicit factory failure");
    };
    assert!(matches!(
        failure.source,
        EntityStorageError::Entity(EntityPersistenceError::UnsupportedType(_))
    ));
    assert_eq!(
        failure.stage,
        super::super::WorldgenEntityLoadStage::Preflight
    );
    assert!(
        failure.chunk.is_some(),
        "the failed owned response is retained"
    );
    assert_eq!(
        level.entity_chunk_load_status(pos),
        ChunkLoadStatus::Pending
    );
    level.auto_save_entities().unwrap();
    assert!(level.take_entity_storage_requests().is_empty());
    assert!(!level.entity::<EntityData>(id).unwrap().removed);
}

#[test]
fn shared_entity_io_preflight_keeps_owned_nbt_and_never_replays_it() {
    let level = level();
    let shared = &level;
    let pos = ChunkPos::ZERO;
    shared
        .update_entity_chunk_status_shared(pos, Visibility::Ticking)
        .unwrap();
    let requests = shared.take_entity_storage_requests();
    let EntityStorageRequest::Load { id, .. } = requests[0] else {
        panic!("expected load")
    };
    let mut unported = orb_tag(17, Vec3::new(1.0, 64.0, 1.0));
    unported.insert("id", Tag::String("minecraft:pig".into()));
    let original = chunk(pos, vec![unported]);
    shared
        .complete_entity_chunk_load(id, Ok(Some(original.clone())))
        .unwrap();
    let Err(EntityStorageError::LoadIntegration(failure)) =
        shared.process_pending_entity_loads_shared()
    else {
        panic!("expected preflight failure")
    };
    assert_eq!(failure.chunk, Some(original));
    assert_eq!(failure.id, id);
    assert_eq!(failure.pos, pos);
    assert_eq!(
        failure.stage,
        super::super::WorldgenEntityLoadStage::Preflight
    );
    assert!(matches!(
        failure.source,
        EntityStorageError::Entity(EntityPersistenceError::UnsupportedType(_))
    ));
    shared.process_pending_entity_loads_shared().unwrap();
    assert_eq!(
        shared.entity_chunk_load_status(pos),
        ChunkLoadStatus::Pending
    );
    assert_eq!(shared.next_entity_id(), 1);
    assert_eq!(shared.next_entity_ref(), EntityRef(1));
    assert!(shared.all_entities().is_empty());
}

#[test]
fn mutable_legacy_load_skips_unknown_trees_and_keeps_registered_passenger_order() {
    let mut level = level();
    let shared = &mut level;
    let pos = ChunkPos::ZERO;
    shared
        .update_entity_chunk_status(pos, Visibility::Ticking)
        .unwrap();
    let requests = shared.take_entity_storage_requests();
    let EntityStorageRequest::Load { id, .. } = requests[0] else {
        panic!("expected load")
    };
    let mut unknown = CompoundTag::new();
    unknown.insert("id", Tag::String("missing:entity".into()));
    unknown.insert(
        "Passengers",
        Tag::List(ListTag(vec![Tag::Compound(orb_tag(99, Vec3::ZERO))])),
    );
    let mut root = orb_tag(1, Vec3::new(1.0, 64.0, 1.0));
    root.insert(
        "Passengers",
        Tag::List(ListTag(vec![
            Tag::Compound(unknown.clone()),
            Tag::Compound(orb_tag(2, Vec3::new(1.0, 64.0, 1.0))),
        ])),
    );
    shared
        .complete_entity_chunk_load(id, Ok(Some(chunk(pos, vec![unknown, root]))))
        .unwrap();
    shared.process_pending_entity_loads().unwrap();
    assert_eq!(shared.all_entities(), [EntityRef(1), EntityRef(2)]);
    let parent = shared.entity::<EntityData>(EntityRef(1)).unwrap();
    assert_eq!(parent.passengers.as_slice(), &[EntityRef(2)]);
    assert_eq!(
        shared
            .entity::<EntityData>(EntityRef(2))
            .unwrap()
            .vehicle
            .get(),
        Some(EntityRef(1))
    );
    assert_eq!(
        shared.take_entity_management_events(),
        [
            EntityManagementEvent::TrackingStart(EntityRef(1)),
            EntityManagementEvent::TickingStart(EntityRef(1)),
            EntityManagementEvent::TrackingStart(EntityRef(2)),
            EntityManagementEvent::TickingStart(EntityRef(2))
        ]
    );
    assert!(shared.are_entities_loaded(pos));
    assert_eq!(parent.tick_count, 0);
    assert_eq!(shared.next_entity_id(), 3);
    let expected = include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .find(|row| row.starts_with("entity-legacy\tunknown-trees\t"))
        .unwrap();
    let fields: Vec<_> = expected.split('\t').collect();
    let links: Vec<_> = shared
        .all_entities()
        .iter()
        .map(|&id| {
            let base = shared.entity::<EntityData>(id).unwrap();
            format!(
                "{}:{}:{}:{}:{}",
                base.network_id,
                base.uuid.least,
                base.vehicle.get().map_or_else(
                    || "none".into(),
                    |vehicle| shared
                        .entity::<EntityData>(vehicle)
                        .unwrap()
                        .network_id
                        .to_string()
                ),
                base.passengers
                    .iter()
                    .map(|&child| shared
                        .entity::<EntityData>(child)
                        .unwrap()
                        .network_id
                        .to_string())
                    .collect::<Vec<_>>()
                    .join("/"),
                base.removed
            )
        })
        .collect();
    assert_eq!(links.join(","), fields[2]);
    assert_eq!(fields[3], "0", "legacy load does not emit onCreated");
    assert_eq!(fields[4], "2", "unknown trees consume no constructor IDs");
}

#[test]
fn duplicate_vehicle_uuid_keeps_held_object_without_stealing_the_visible_identity() {
    let mut level = level();
    let pos = ChunkPos::ZERO;
    level
        .update_entity_chunk_status(pos, Visibility::Ticking)
        .unwrap();
    let (request, _) = load_request(&mut level);
    let original = orb_tag(1, Vec3::new(1.0, 64.0, 1.0));
    let mut duplicate = original.clone();
    duplicate.insert(
        "Passengers",
        Tag::List(ListTag(vec![Tag::Compound(orb_tag(
            2,
            Vec3::new(2.0, 65.0, 2.0),
        ))])),
    );
    level
        .complete_entity_chunk_load(request, Ok(Some(chunk(pos, vec![original, duplicate]))))
        .unwrap();
    level.tick_entity_management().unwrap();
    let original = level.entity_by_uuid(Uuid::new(0, 1)).unwrap();
    let passenger = level.entity_by_uuid(Uuid::new(0, 2)).unwrap();
    let duplicate = level
        .entity::<EntityData>(passenger)
        .unwrap()
        .vehicle
        .get()
        .unwrap();
    assert_ne!(original, duplicate);
    assert_eq!(level.all_entities(), vec![original, passenger]);
    assert!(!level.entity::<EntityData>(duplicate).unwrap().removed);
    level
        .set_entity_pos(duplicate, Vec3::new(48.0, 64.0, 1.0))
        .unwrap();
    assert_eq!(level.all_entities(), vec![original, passenger]);
    assert_eq!(level.entity_by_uuid(Uuid::new(0, 1)), Some(original));
    crate::entity::stop_riding(passenger, &mut level).unwrap();
    level.finish_entity_management();
    assert!(level.entity_state(duplicate).is_err());
    assert_eq!(level.entity_by_uuid(Uuid::new(0, 1)), Some(original));
    assert_eq!(level.entity_chunk_load_status(pos), ChunkLoadStatus::Loaded);
}

#[test]
fn shared_visibility_requires_mutable_tracking_without_losing_the_transition() {
    let mut level = level();
    let pos = ChunkPos::ZERO;
    level
        .add_worldgen_chunk_entities_shared(pos, vec![orb_tag(1, Vec3::new(1.0, 64.0, 1.0))])
        .unwrap();
    level.take_entity_management_events();
    assert_eq!(
        level.update_entity_chunk_status_shared(pos, Visibility::Tracked),
        Err(LevelError::UnsupportedSharedEntityTracking(
            "update_entity_chunk_status_shared"
        ))
    );
    assert_eq!(level.entity_chunk_visibility(pos), Visibility::Hidden);
    assert!(level.all_entities().is_empty());
    assert!(level.take_entity_management_events().is_empty());
    level
        .update_entity_chunk_status(pos, Visibility::Tracked)
        .unwrap();
    assert_eq!(level.all_entities(), [EntityRef(1)]);
    assert_eq!(
        level.take_entity_management_events(),
        [EntityManagementEvent::TrackingStart(EntityRef(1))]
    );
}
