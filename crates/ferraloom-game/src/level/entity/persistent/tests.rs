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
        level.entity::<EntityData>(passenger).unwrap().vehicle,
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
    assert!(matches!(
        level.poll_entity_save_all(),
        Err(EntityStorageError::Entity(
            EntityPersistenceError::UnsupportedType(_)
        ))
    ));
    assert_eq!(
        level.entity_chunk_load_status(pos),
        ChunkLoadStatus::Pending
    );
    level.auto_save_entities().unwrap();
    assert!(level.take_entity_storage_requests().is_empty());
    assert!(!level.entity::<EntityData>(id).unwrap().removed);
}
