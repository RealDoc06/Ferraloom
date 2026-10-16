//! Actual query contracts. R27 changes are UNRUN until ROOT's native receipt.

use super::{query::*, *};
use crate::chunk::{ChunkData, poi_binding::TestRuntime};
use pumpkin_data::{Block, BlockStateId};

fn area() -> PoiArea {
    PoiArea::new(BlockPos::new(0, 0, 0), 48).unwrap()
}

fn publish(runtime: &TestRuntime, x: i32) -> crate::level::SyncChunk {
    let chunk = ChunkData::empty_sync(x, 0);
    chunk.bind_poi(&runtime.runtime).unwrap();
    let _lifecycle = runtime.runtime.lifecycle.write().unwrap();
    runtime.chunks.insert(ChunkPos::new(x, 0), chunk.clone());
    chunk
}

#[test]
fn cache_query_actual_writers_preserve_tickets_and_reject_obsolete_preparation() {
    let directory = tempfile::tempdir().unwrap();
    let runtime = TestRuntime::new(directory.path().to_owned());
    let first = publish(&runtime, 0);
    let second = publish(&runtime, 1);
    let homes: Vec<_> = Block::WHITE_BED
        .states
        .iter()
        .map(|s| s.id)
        .filter(|state| PoiType::from_state(*state) == Some(PoiType::Home))
        .collect();
    first.set_block_absolute_y(1, 0, 1, homes[0]);
    second.set_blocks_batch([(1, 0, 1, Block::BELL.default_state.id)]);
    assert_eq!(runtime.runtime.snapshot(area()).unwrap().len(), 2);
    // A query no longer establishes the reconciled ticket baseline.
    let baseline = first.prepare_poi().unwrap();
    runtime
        .runtime
        .storage
        .lock()
        .unwrap()
        .add_with_free_tickets(BlockPos::new(1, 0, 1), PoiType::Home.identifier(), -7)
        .unwrap();
    first.set_block_absolute_y(1, 0, 1, homes[1]);
    let entries = runtime.runtime.snapshot(area()).unwrap();
    assert_eq!(entries.iter().find(|e| e.x == 1).unwrap().free_tickets, -7);
    assert!(!first.read_for_poi().is_current(&baseline));
    first.set_blocks_batch([(1, 0, 1, BlockStateId::AIR), (1, 0, 1, homes[0])]);
    assert_eq!(
        runtime
            .runtime
            .snapshot(area())
            .unwrap()
            .iter()
            .find(|e| e.x == 1)
            .unwrap()
            .free_tickets,
        1
    );

    // Even changed observations are independent of the terrain encoder.
    first.set_custom_data("test", "end", NbtTag::End);
    assert_eq!(runtime.runtime.snapshot(area()).unwrap().len(), 2);
    first.set_block_absolute_y(2, 0, 1, Block::BELL.default_state.id);
    assert_eq!(runtime.runtime.capture_area(area()).unwrap().len(), 3);
    assert_eq!(
        runtime.runtime.storage.lock().unwrap().regions[&(0, 0)].entries[&(1, 0, 1)].free_tickets,
        -7
    );
    let replacement = publish(&runtime, 0);
    replacement.set_block_absolute_y(3, 0, 1, Block::LECTERN.default_state.id);
    assert!(matches!(
        runtime.runtime.capture_area_with_overlays(
            area(),
            &[],
            &[(ChunkPos::new(0, 0), first.clone())],
        ),
        Err(PoiQueryError::ChangedDuringQuery)
    ));
    assert!(matches!(
        first.prepare_poi(),
        Err(PoiReconcileError::ObsoletePublication)
    ));
    let entries = runtime.runtime.snapshot(area()).unwrap();
    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .any(|e| e.x == 3 && e.poi_type == PoiType::Librarian.identifier())
    );
    assert!(!entries.iter().any(|e| e.x == 1));
}

#[test]
fn cache_query_known_missing_and_valid_zero_are_observations() {
    use super::persistence_tests::{chunk, compress_fixture, record, section, write_mca};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("r.0.0.mca");
    let payload = compress_fixture(chunk(&[(
        "0",
        section(
            Some(0),
            vec![record([1, 0, 1], PoiType::Home.identifier(), Some(0))],
        ),
    )]));
    write_mca(&path, &[(0, &payload)]);
    let before = std::fs::read(&path).unwrap();
    let runtime = TestRuntime::new(directory.path().to_owned());
    let entries = runtime.runtime.snapshot(area()).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].free_tickets, 0);
    let mut storage = runtime.runtime.storage.lock().unwrap();
    assert_eq!(storage.regions[&(0, 0)].section_validity[&(0, 0, 0)], 0);
    assert!(
        !storage.regions[&(0, 0)]
            .section_validity
            .contains_key(&(1, 0, 0))
    );
    storage.save_all().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

async fn private_query_sibling<S>(config: S::ChunkConfig)
where
    S: crate::chunk::io::ChunkSerializer<Data = ChunkData, WriteBackend = PathBuf>,
    S::ChunkConfig: Send + Sync,
{
    use crate::chunk::io::{Dirtiable, FileIO, file_manager::ChunkFileManager};
    use crate::level::LevelFolder;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().to_owned();
    let folder = LevelFolder {
        root_folder: root.clone(),
        dim_folder: root.clone(),
        region_folder: root.join("region"),
        entities_folder: root.join("entities"),
        poi_folder: root.join("poi"),
    };
    std::fs::create_dir_all(&folder.region_folder).unwrap();
    let runtime = TestRuntime::new(folder.poi_folder.clone());
    let first = publish(&runtime, 0);
    let second = publish(&runtime, 1);
    let writer = ChunkFileManager::<S>::new(config);
    first.set_block_absolute_y(1, 0, 1, Block::LECTERN.default_state.id);
    writer
        .save_chunks(&folder, vec![(ChunkPos::new(0, 0), first.clone())])
        .await
        .unwrap();
    writer.flush().await.unwrap();
    first.set_block_absolute_y(1, 0, 1, Block::BELL.default_state.id);
    first.set_custom_data("test", "end", NbtTag::End);
    assert!(
        runtime
            .runtime
            .snapshot(area())
            .unwrap()
            .iter()
            .any(|entry| entry.x == 1 && entry.poi_type == PoiType::Meeting.identifier())
    );
    assert!(
        writer
            .save_chunks(&folder, vec![(ChunkPos::new(0, 0), first.clone())])
            .await
            .is_err()
    );
    assert!(first.is_dirty());
    second.set_block_absolute_y(1, 0, 1, Block::COMPOSTER.default_state.id);
    // Pending A still refuses, while the real manager writes healthy sibling B.
    assert!(
        writer
            .save_chunks(&folder, vec![(ChunkPos::new(1, 0), second)])
            .await
            .is_err()
    );
    assert!(writer.flush().await.is_err());
    runtime.runtime.storage.lock().unwrap().save_all().unwrap();
    let path = folder.poi_folder.join("r.0.0.mca");
    let disk = PoiRegion::load(&path).unwrap();
    assert_eq!(
        disk.entries[&(1, 0, 1)].poi_type,
        PoiType::Librarian.identifier()
    );
    assert_eq!(
        disk.entries[&(17, 0, 1)].poi_type,
        PoiType::Farmer.identifier()
    );
    assert_eq!(
        first.section.get_block_absolute_y(1, 0, 1),
        Some(Block::BELL.default_state.id)
    );
    first.set_custom_data("test", "end", NbtTag::Int(1));
    let entries = runtime.runtime.snapshot(area()).unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.x == 1 && e.poi_type == PoiType::Meeting.identifier())
    );
    runtime.runtime.storage.lock().unwrap().save_all().unwrap();
    assert_eq!(
        PoiRegion::load(&path).unwrap().entries[&(1, 0, 1)].poi_type,
        PoiType::Librarian.identifier()
    );
    // Only successful retry of A's retained actual save may publish its POI.
    writer.flush().await.unwrap();
    assert!(!first.is_dirty());
    assert_eq!(
        PoiRegion::load(&path).unwrap().entries[&(1, 0, 1)].free_tickets,
        32
    );
}

#[tokio::test]
async fn private_query_preserves_unsavable_source_across_three_format_sibling_saves() {
    use crate::chunk::format::{anvil::AnvilChunkFile, linear::LinearV2File, pump::PumpFile};
    Box::pin(private_query_sibling::<AnvilChunkFile<ChunkData>>(
        pumpkin_config::chunk::AnvilChunkConfig::default(),
    ))
    .await;
    Box::pin(private_query_sibling::<LinearV2File<ChunkData>>(())).await;
    Box::pin(private_query_sibling::<PumpFile<ChunkData>>(())).await;
}

// Shared source fixture for the actual decoder and scheduler conversion tests.
pub fn sparse_read_fixture(folder: &Path) -> ChunkData {
    use super::persistence_tests::{chunk, compress_fixture, record, section, write_mca};
    std::fs::create_dir_all(folder).unwrap();
    let records = [
        (
            "0",
            section(
                Some(0),
                vec![record([1, 0, 1], PoiType::Home.identifier(), Some(0))],
            ),
        ),
        (
            "1",
            section(
                Some(0),
                vec![record([1, 16, 1], PoiType::Home.identifier(), Some(-7))],
            ),
        ),
        (
            "2",
            section(
                Some(1),
                vec![record([1, 32, 1], PoiType::Meeting.identifier(), Some(3))],
            ),
        ),
    ];
    let payload = compress_fixture(chunk(&records));
    write_mca(&folder.join("r.0.0.mca"), &[(0, &payload)]);
    let mut root = NbtCompound::new();
    root.put_int("DataVersion", DATA_VERSION);
    root.put_int("xPos", 0);
    root.put_int("zPos", 0);
    root.put_int("yPos", -4);
    root.put_string("Status", "minecraft:empty".to_owned());
    root.put_list(
        "sections",
        [0, 2]
            .into_iter()
            .map(|y| {
                let mut section = NbtCompound::new();
                section.put_byte("Y", y);
                // Listed without block_states is a default section, not unvisited.
                NbtTag::Compound(section)
            })
            .collect(),
    );
    let bytes = crate::nbt_io::encode_compound(root).unwrap();
    ChunkData::internal_from_bytes(&bytes, ChunkPos::new(0, 0)).unwrap()
}

#[test]
fn empty_read_overlay_visits_only_listed_sections_without_publishing() {
    let directory = tempfile::tempdir().unwrap();
    let decoded = sparse_read_fixture(directory.path());
    let runtime = TestRuntime::new(directory.path().to_owned());
    let path = directory.path().join("r.0.0.mca");
    let before = std::fs::read(&path).unwrap();
    let overlay = PoiTerrainOverlay::classify_decoded(&decoded).unwrap();
    assert_eq!(
        overlay
            .sections
            .iter()
            .map(|section| section.section_y)
            .collect::<Vec<_>>(),
        vec![0, 2]
    );
    runtime
        .runtime
        .storage
        .lock()
        .unwrap()
        .preload_area(area())
        .unwrap();
    let entries = runtime
        .runtime
        .capture_area_with_overlays(area(), &[overlay], &[])
        .unwrap();
    assert!(!entries.iter().any(|entry| entry.y == 0));
    assert_eq!(
        entries
            .iter()
            .find(|entry| entry.y == 16)
            .unwrap()
            .free_tickets,
        -7
    );
    assert_eq!(
        entries
            .iter()
            .find(|entry| entry.y == 32)
            .unwrap()
            .free_tickets,
        3
    );
    let mut storage = runtime.runtime.storage.lock().unwrap();
    assert_eq!(storage.regions[&(0, 0)].section_validity[&(0, 0, 0)], 0);
    assert!(storage.regions[&(0, 0)].entries.contains_key(&(1, 0, 1)));
    storage.save_all().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn cache_query_holds_both_terrain_sources_in_one_common_read_window() {
    use std::sync::{TryLockError, mpsc};
    use std::time::{Duration, Instant};
    let directory = tempfile::tempdir().unwrap();
    let runtime = TestRuntime::new(directory.path().to_owned());
    let first = publish(&runtime, 0);
    let second = publish(&runtime, 1);
    first.set_block_absolute_y(1, 0, 1, Block::LECTERN.default_state.id);
    second.set_blocks_batch([(1, 0, 1, Block::COMPOSTER.default_state.id)]);
    first.prepare_poi().unwrap();
    second.prepare_poi().unwrap();
    // Both FULL sources now require private classification in the same window.
    first.set_block_absolute_y(2, 0, 1, Block::STONE.default_state.id);
    second.set_blocks_batch([(2, 0, 1, Block::STONE.default_state.id)]);
    first.set_custom_data("test", "end", NbtTag::End);
    std::thread::scope(|scope| {
        // A sorts before B. Only the real query can hold A while B is blocked.
        let held_second = second.section.block_sections.write().unwrap();
        let (captured, capture) = mpsc::channel();
        let runtime = &runtime.runtime;
        scope.spawn(move || {
            captured.send(runtime.snapshot(area())).unwrap();
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match first.section.block_sections.try_write() {
                Err(TryLockError::WouldBlock) => break,
                Err(TryLockError::Poisoned(_)) => panic!("unexpected terrain poison"),
                Ok(available) => drop(available),
            }
            assert!(Instant::now() < deadline, "query did not acquire A");
            std::thread::yield_now();
        }
        let (starting, started) = mpsc::channel();
        let (finished, finish) = mpsc::channel();
        let first = &first;
        scope.spawn(move || {
            starting.send(()).unwrap();
            first.set_block_absolute_y(1, 0, 1, Block::BELL.default_state.id);
            finished.send(()).unwrap();
        });
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(
            finish.recv_timeout(Duration::from_millis(50)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        drop(held_second);
        let entries = capture
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries.iter().find(|entry| entry.x == 1).unwrap().poi_type,
            PoiType::Librarian.identifier()
        );
        assert_eq!(
            entries.iter().find(|entry| entry.x == 17).unwrap().poi_type,
            PoiType::Farmer.identifier()
        );
        finish.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let entries = runtime.runtime.snapshot(area()).unwrap();
    assert_eq!(
        entries.iter().find(|entry| entry.x == 1).unwrap().poi_type,
        PoiType::Meeting.identifier()
    );
    assert_eq!(
        entries.iter().find(|entry| entry.x == 17).unwrap().poi_type,
        PoiType::Farmer.identifier()
    );
}

// R27 F1 production-path control, UNRUN.
#[test]
fn cache_query_repeated_single_and_batch_writes_keep_aba_and_empty_results_private() {
    let directory = tempfile::tempdir().unwrap();
    let runtime = TestRuntime::new(directory.path().to_owned());
    let first = publish(&runtime, 0);
    let second = publish(&runtime, 1);
    let home = Block::WHITE_BED
        .states
        .iter()
        .map(|state| state.id)
        .find(|state| PoiType::from_state(*state) == Some(PoiType::Home))
        .unwrap();
    first.set_block_absolute_y(1, 0, 1, home);
    second.set_blocks_batch([(1, 0, 1, Block::BELL.default_state.id)]);
    let baseline = first.prepare_poi().unwrap();
    second.prepare_poi().unwrap();
    {
        let mut storage = runtime.runtime.storage.lock().unwrap();
        storage
            .add_with_free_tickets(BlockPos::new(1, 0, 1), PoiType::Home.identifier(), 0)
            .unwrap();
        storage.save_all().unwrap();
    };
    let path = directory.path().join("r.0.0.mca");
    let before = std::fs::read(&path).unwrap();
    first.set_custom_data("test", "end", NbtTag::End);
    for round in 0..16 {
        let occupied = round % 2 == 0;
        let state = if occupied { Block::STONE } else { Block::DIRT };
        first.set_block_absolute_y(2, 0, 1, state.default_state.id);
        first.set_blocks_batch([(1, 0, 1, BlockStateId::AIR), (1, 0, 1, home)]);
        second.set_blocks_batch([(
            1,
            0,
            1,
            if occupied {
                Block::BELL.default_state.id
            } else {
                BlockStateId::AIR
            },
        )]);
        for _ in 0..2 {
            let entries = runtime.runtime.snapshot(area()).unwrap();
            assert_eq!(entries.len(), 1 + usize::from(occupied));
            assert_eq!(
                entries
                    .iter()
                    .find(|entry| entry.x == 1)
                    .unwrap()
                    .free_tickets,
                1
            );
            assert_eq!(entries.iter().any(|entry| entry.x == 17), occupied);
            assert!(!first.read_for_poi().is_current(&baseline));
        }
    }
    first.set_block_absolute_y(1, 0, 1, BlockStateId::AIR);
    assert!(runtime.runtime.snapshot(area()).unwrap().is_empty());
    let mut storage = runtime.runtime.storage.lock().unwrap();
    assert_eq!(storage.regions[&(0, 0)].entries[&(1, 0, 1)].free_tickets, 0);
    assert!(storage.regions[&(0, 0)].entries.contains_key(&(17, 0, 1)));
    storage.save_all().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

// R27 F5 production-path control, UNRUN.
#[test]
fn classified_cold_sections_merge_current_tickets_validity_and_unknown_refusals() {
    let directory = tempfile::tempdir().unwrap();
    let decoded = sparse_read_fixture(directory.path());
    let home = Block::WHITE_BED
        .states
        .iter()
        .map(|state| state.id)
        .find(|state| PoiType::from_state(*state) == Some(PoiType::Home))
        .unwrap();
    decoded.set_blocks_batch([(1, 0, 1, home), (1, 32, 1, home)]);
    let overlay = PoiTerrainOverlay::classify_decoded(&decoded).unwrap();
    let runtime = TestRuntime::new(directory.path().to_owned());
    {
        let mut storage = runtime.runtime.storage.lock().unwrap();
        storage.preload_area(area()).unwrap();
        // Classification predates this ticket mutation and knows no cached Valid.
        storage
            .add_with_free_tickets(BlockPos::new(1, 0, 1), PoiType::Home.identifier(), -11)
            .unwrap();
        storage.save_all().unwrap();
    };
    let path = directory.path().join("r.0.0.mca");
    let before = std::fs::read(&path).unwrap();
    let entries = runtime
        .runtime
        .capture_area_with_overlays(area(), std::slice::from_ref(&overlay), &[])
        .unwrap();
    assert_eq!(
        entries
            .iter()
            .find(|entry| entry.y == 0)
            .unwrap()
            .free_tickets,
        -11
    );
    assert_eq!(
        entries
            .iter()
            .find(|entry| entry.y == 16)
            .unwrap()
            .free_tickets,
        -7
    );
    let valid = entries.iter().find(|entry| entry.y == 32).unwrap();
    assert_eq!(valid.poi_type, PoiType::Meeting.identifier());
    assert_eq!(valid.free_tickets, 3);
    {
        let mut storage = runtime.runtime.storage.lock().unwrap();
        assert_eq!(storage.regions[&(0, 0)].section_validity[&(0, 0, 0)], 0);
        storage.save_all().unwrap();
    };
    assert_eq!(std::fs::read(&path).unwrap(), before);

    // Even a position absent from the sparse classified POIs must be checked.
    let unknown = BlockPos::new(2, 0, 1);
    {
        let mut storage = runtime.runtime.storage.lock().unwrap();
        storage
            .add_with_free_tickets(unknown, "future:poi", -9)
            .unwrap();
        storage.save_all().unwrap();
    };
    let refused_bytes = std::fs::read(&path).unwrap();
    assert!(matches!(
        runtime.runtime.capture_area_with_overlays(area(), &[overlay], &[]),
        Err(PoiQueryError::Reconciliation(PoiReconcileError::UnknownType { pos, .. }))
            if pos == unknown
    ));
    let mut storage = runtime.runtime.storage.lock().unwrap();
    assert_eq!(
        storage.regions[&(0, 0)].entries[&(1, 0, 1)].free_tickets,
        -11
    );
    assert_eq!(
        storage.regions[&(0, 0)].entries[&(2, 0, 1)].free_tickets,
        -9
    );
    storage.save_all().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), refused_bytes);
}
