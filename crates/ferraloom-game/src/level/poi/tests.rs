use super::*;
use ferraloom_storage::{
    IntArray, PoiChunk, PoiRecord as PackedRecord, PoiSection as PackedSection,
};
use ferraloom_util::{BlockPos, ChunkPos};

fn loaded() -> PoiManager {
    let mut manager = PoiManager::new(-1, 0);
    for pos in PoiManager::chunks_in_square(BlockPos::ZERO, 16) {
        manager.complete_chunk_load(pos, None).unwrap();
    }
    manager
}
fn packed(pos: BlockPos, kind: PoiTypeId, free: i32, valid: bool) -> PoiChunk {
    PoiChunk {
        data_version: Some(ferraloom_storage::DATA_VERSION),
        sections: Some(
            [(
                (pos.y >> 4).to_string().into(),
                PackedSection {
                    valid: Some(valid),
                    records: Some(vec![PackedRecord {
                        position: Some(IntArray(vec![pos.x, pos.y, pos.z])),
                        poi_type: Some(kind.name().into()),
                        free_tickets: Some(free),
                        ..Default::default()
                    }]),
                    ..Default::default()
                },
            )]
            .into_iter()
            .collect(),
        ),
        ..Default::default()
    }
}
#[test]
fn unknown_absent_and_late_prefetch_are_distinct_and_do_not_replace_mutations() {
    let pos = BlockPos::new(1, 0, 1);
    let chunk = ChunkPos::ZERO;
    let mut manager = PoiManager::new(-1, 0);
    assert_eq!(manager.get_type(pos), Err(PoiError::UnloadedChunk(chunk)));
    manager.complete_chunk_load(chunk, None).unwrap();
    assert_eq!(manager.get_type(pos).unwrap(), None);
    manager
        .complete_chunk_load(chunk, Some(packed(pos, PoiTypeId::MEETING, 32, true)))
        .unwrap();
    assert_eq!(manager.get_type(pos).unwrap(), None);
    manager.add(pos, PoiTypeId::HOME).unwrap();
    manager
        .complete_chunk_load(chunk, Some(packed(pos, PoiTypeId::MEETING, 32, true)))
        .unwrap();
    assert_eq!(manager.get_type(pos).unwrap(), Some(PoiTypeId::HOME));
}
#[test]
fn ticket_capacity_occupancy_and_revision_acknowledgement_remain_canonical() {
    let mut manager = loaded();
    let pos = BlockPos::new(1, 0, 1);
    manager.add(pos, PoiTypeId::HOME).unwrap();
    let before = manager
        .capture_chunk_for_save(ChunkPos::ZERO)
        .unwrap()
        .unwrap();
    assert_eq!(
        manager
            .take(|kind| kind == PoiTypeId::HOME, |_, _| true, pos, 1)
            .unwrap(),
        Some(pos)
    );
    assert_eq!(manager.get_free_tickets(pos).unwrap(), 0);
    assert_eq!(
        manager
            .get_count_in_range(|_| true, pos, 1, Occupancy::IsOccupied)
            .unwrap(),
        1
    );
    assert_eq!(manager.take(|_| true, |_, _| true, pos, 1).unwrap(), None);
    manager
        .acknowledge_chunk_saved(ChunkPos::ZERO, before.revision)
        .unwrap();
    let claimed = manager
        .capture_chunk_for_save(ChunkPos::ZERO)
        .unwrap()
        .unwrap();
    assert!(claimed.revision > before.revision);
    manager
        .acknowledge_chunk_saved(ChunkPos::ZERO, claimed.revision)
        .unwrap();
    assert!(
        manager
            .capture_chunk_for_save(ChunkPos::ZERO)
            .unwrap()
            .is_none()
    );
    assert!(manager.release(pos).unwrap());
    assert!(!manager.release(pos).unwrap());
    assert!(
        manager
            .capture_chunk_for_save(ChunkPos::ZERO)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        manager.release(BlockPos::new(2, 0, 1)),
        Err(PoiError::MissingRecord(BlockPos::new(2, 0, 1)))
    );
}
#[test]
fn invalid_section_refresh_preserves_old_ticket_objects_by_position() {
    let pos = BlockPos::new(1, 0, 1);
    let extra = BlockPos::new(2, 0, 1);
    let mut manager = PoiManager::new(-1, 0);
    manager
        .complete_chunk_load(ChunkPos::ZERO, Some(packed(pos, PoiTypeId::HOME, 0, false)))
        .unwrap();
    manager
        .check_consistency(
            ChunkPos::ZERO,
            0,
            vec![(pos, PoiTypeId::MEETING), (extra, PoiTypeId::HOME)],
        )
        .unwrap();
    // Native refresh reuses the old record by position, including its old type.
    assert_eq!(manager.get_type(pos).unwrap(), Some(PoiTypeId::HOME));
    assert_eq!(manager.get_free_tickets(pos).unwrap(), 0);
    assert_eq!(manager.get_free_tickets(extra).unwrap(), 1);
    manager
        .check_consistency(ChunkPos::ZERO, 0, Vec::new())
        .unwrap();
    assert_eq!(manager.get_type(extra).unwrap(), Some(PoiTypeId::HOME));
    let saved = manager
        .capture_chunk_for_save(ChunkPos::ZERO)
        .unwrap()
        .unwrap();
    assert_eq!(
        saved
            .chunk
            .sections
            .as_ref()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .valid,
        Some(true)
    );
}
#[test]
fn zero_ticket_types_are_neither_claimable_nor_occupied() {
    let mut manager = loaded();
    let pos = BlockPos::new(1, 0, 1);
    manager.add(pos, PoiTypeId::LIGHTNING_ROD).unwrap();
    assert_eq!(
        manager
            .get_count_in_range(|_| true, pos, 1, Occupancy::Any)
            .unwrap(),
        1
    );
    assert_eq!(
        manager
            .get_count_in_range(|_| true, pos, 1, Occupancy::IsOccupied)
            .unwrap(),
        0
    );
    assert_eq!(manager.take(|_| true, |_, _| true, pos, 1).unwrap(), None);
    assert!(!manager.release(pos).unwrap());
}
#[test]
fn storage_errors_keep_the_original_owned_cause() {
    use std::error::Error;
    use std::sync::Arc;
    let source = Arc::new(ferraloom_storage::Error::Invalid("POI fixture".into()));
    let error = PoiError::Storage(source.clone());
    assert_eq!(error, error.clone());
    assert!(std::ptr::eq(
        error
            .source()
            .unwrap()
            .downcast_ref::<ferraloom_storage::Error>()
            .unwrap(),
        source.as_ref()
    ));
    assert_ne!(
        error,
        PoiError::Storage(Arc::new(ferraloom_storage::Error::Invalid(
            "POI fixture".into()
        )))
    );
}

#[test]
fn village_distance_tracks_occupied_sources_and_caps_at_seven_without_loading() {
    use ferraloom_util::SectionPos;
    let mut manager = loaded();
    let pos = BlockPos::new(1, 0, 1);
    manager.add(pos, PoiTypeId::HOME).unwrap();
    let village = |kind| kind == PoiTypeId::HOME;
    assert_eq!(
        manager.sections_to_village(SectionPos::new(0, 0, 0), village),
        7
    );
    manager.take(village, |_, _| true, pos, 1).unwrap();
    for distance in 0..=8 {
        assert_eq!(
            manager.sections_to_village(SectionPos::new(distance, distance, -distance), village),
            distance.min(7)
        );
    }
    manager.release(pos).unwrap();
    assert_eq!(
        manager.sections_to_village(SectionPos::new(0, 0, 0), village),
        7
    );
}
