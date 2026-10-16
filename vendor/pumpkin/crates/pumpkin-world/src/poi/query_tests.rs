//! Stage C actual query contracts. UNRUN until root's exclusive native receipt.

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

    // Clean observations do not run an encoder on every villager query.
    first.set_custom_data("test", "end", NbtTag::End);
    assert_eq!(runtime.runtime.snapshot(area()).unwrap().len(), 2);
    first.set_block_absolute_y(2, 0, 1, Block::BELL.default_state.id);
    let AreaCapture::NeedsPreparation(pending) = runtime.runtime.capture_area(area()).unwrap()
    else {
        panic!("actual write must invalidate the area");
    };
    assert!(std::sync::Arc::ptr_eq(&pending[0], &first));
    let replacement = publish(&runtime, 0);
    replacement.set_block_absolute_y(3, 0, 1, Block::LECTERN.default_state.id);
    assert!(matches!(
        pending[0].prepare_poi(),
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

async fn refused_query_sibling<S>(config: S::ChunkConfig)
where
    S: crate::chunk::io::ChunkSerializer<Data = ChunkData, WriteBackend = PathBuf>,
    S::ChunkConfig: Send + Sync,
{
    use crate::chunk::io::{FileIO, file_manager::ChunkFileManager};
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
    assert!(matches!(
        runtime.runtime.snapshot(area()),
        Err(PoiQueryError::Reconciliation(
            PoiReconcileError::TerrainEncoding(_)
        ))
    ));
    second.set_block_absolute_y(1, 0, 1, Block::COMPOSTER.default_state.id);
    writer
        .save_chunks(&folder, vec![(ChunkPos::new(1, 0), second)])
        .await
        .unwrap();
    writer.flush().await.unwrap();
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
        PoiRegion::load(&path).unwrap().entries[&(1, 0, 1)].free_tickets,
        32
    );
}

#[tokio::test]
async fn refused_query_cannot_leak_through_same_region_sibling_in_all_three_formats() {
    use crate::chunk::format::{anvil::AnvilChunkFile, linear::LinearV2File, pump::PumpFile};
    Box::pin(refused_query_sibling::<AnvilChunkFile<ChunkData>>(
        Default::default(),
    ))
    .await;
    Box::pin(refused_query_sibling::<LinearV2File<ChunkData>>(())).await;
    Box::pin(refused_query_sibling::<PumpFile<ChunkData>>(())).await;
}
