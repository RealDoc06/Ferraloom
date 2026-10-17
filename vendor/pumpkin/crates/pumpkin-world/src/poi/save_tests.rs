//! Stage B source contracts. All tests are UNRUN pending root's exclusive slot.

use super::persistence_tests::{chunk as nbt_chunk, compress_fixture, record, section, write_mca};
use super::*;
use crate::chunk::{
    ChunkData, ChunkSerializingError, ChunkWritingError,
    format::{
        anvil::{AnvilChunkFile, SingleChunkDataSerializer},
        linear::LinearV2File,
        pump::PumpFile,
    },
    io::{ChunkSerializer, Dirtiable, FileIO, LoadedData, file_manager::ChunkFileManager},
    poi_binding::TestRuntime,
};
use crate::level::LevelFolder;
use pumpkin_data::{Block, BlockStateId};
use std::sync::Arc;

fn home() -> BlockStateId {
    Block::WHITE_BED
        .states
        .iter()
        .map(|state| state.id)
        .find(|state| PoiType::from_state(*state) == Some(PoiType::Home))
        .unwrap()
}

fn folder(directory: &tempfile::TempDir) -> LevelFolder {
    let root = directory.path().to_owned();
    let folder = LevelFolder {
        root_folder: root.clone(),
        dim_folder: root.clone(),
        region_folder: root.join("region"),
        entities_folder: root.join("entities"),
        poi_folder: root.join("poi"),
    };
    std::fs::create_dir_all(&folder.region_folder).unwrap();
    std::fs::create_dir_all(&folder.poi_folder).unwrap();
    folder
}

async fn disk_chunk<S>(
    folder: &LevelFolder,
    pos: ChunkPos,
    config: S::ChunkConfig,
) -> crate::level::SyncChunk
where
    S: ChunkSerializer<Data = ChunkData, WriteBackend = PathBuf>,
    S::ChunkConfig: Send + Sync,
{
    let reader = ChunkFileManager::<S>::new(config);
    let (send, mut receive) = tokio::sync::mpsc::channel(1);
    reader.fetch_chunks(folder, &[pos], send).await;
    let Some(LoadedData::Loaded(chunk)) = receive.recv().await else {
        panic!("saved terrain missing")
    };
    chunk
}

async fn format_capture<S>(config: S::ChunkConfig)
where
    S: ChunkSerializer<Data = ChunkData, WriteBackend = PathBuf>,
    S::ChunkConfig: Clone + Send + Sync + 'static,
{
    let directory = tempfile::tempdir().unwrap();
    let folder = Arc::new(folder(&directory));
    let runtime = TestRuntime::new(folder.poi_folder.clone());
    let pos = ChunkPos::new(0, 0);
    let chunk = ChunkData::empty_sync(0, 0);
    chunk.bind_poi(&runtime.runtime).unwrap();
    runtime.chunks.insert(pos, chunk.clone());
    chunk.set_block_absolute_y(1, 0, 1, home());
    let writer = Arc::new(ChunkFileManager::<S>::new(config.clone()));
    writer
        .save_chunks(&folder, vec![(pos, chunk.clone())])
        .await
        .unwrap();
    writer.flush().await.unwrap();

    // Hold the serializer's existing first long-lived field lock on a separate
    // thread. The actual queued backend save cannot capture terrain until release.
    let (held, ready) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel();
    let source = chunk.clone();
    let blocker = std::thread::spawn(move || {
        let _light = source.light_engine.lock().unwrap();
        held.send(()).unwrap();
        released.recv().unwrap();
    });
    ready.await.unwrap();
    chunk.mark_dirty(true);
    let queued_chunk = chunk.clone();
    let queued_writer = writer.clone();
    let queued_folder = folder.clone();
    let (queued, accepted) = tokio::sync::oneshot::channel();
    let save = tokio::spawn(async move {
        queued.send(()).unwrap();
        queued_writer
            .save_chunks(&queued_folder, vec![(pos, queued_chunk)])
            .await
    });
    accepted.await.unwrap();
    // apply_pending clears dirty immediately before dispatching the real codec.
    // This observes admission to that production queue, not only task creation.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while chunk.is_dirty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    chunk.set_block_absolute_y(1, 0, 1, Block::BELL.default_state.id);
    release.send(()).unwrap();
    blocker.join().unwrap();
    save.await.unwrap().unwrap();
    writer.flush().await.unwrap();
    let terrain = disk_chunk::<S>(&folder, pos, config.clone()).await;
    assert_eq!(
        terrain.section.get_block_absolute_y(1, 0, 1),
        Some(Block::BELL.default_state.id)
    );
    let poi_path = folder.poi_folder.join("r.0.0.mca");
    let poi = PoiRegion::load(&poi_path).unwrap();
    let entry = &poi.entries[&(1, 0, 1)];
    assert_eq!(entry.poi_type, PoiType::Meeting.identifier());
    assert_eq!(entry.free_tickets, 32);
    assert_eq!(poi.section_validity[&(0, 0, 0)], 1);

    // F1: an invalid original NBT tree must fail before *any durable POI write*.
    let terrain_path = folder.region_folder.join(S::get_chunk_key(&pos));
    let terrain_before = std::fs::read(&terrain_path).unwrap();
    let poi_before = std::fs::read(&poi_path).unwrap();
    chunk.set_block_absolute_y(1, 0, 1, home());
    chunk.set_custom_data("test", "refused", NbtTag::End);
    assert!(matches!(
        chunk.to_bytes_for_save(),
        Err(ChunkSerializingError::ErrorSerializingChunk(_))
    ));
    assert!(matches!(
        writer
            .save_chunks(&folder, vec![(pos, chunk.clone())])
            .await,
        Err(ChunkWritingError::ChunkSerializingError(_))
    ));
    assert!(chunk.is_dirty());
    assert_eq!(std::fs::read(&terrain_path).unwrap(), terrain_before);
    assert_eq!(std::fs::read(&poi_path).unwrap(), poi_before);
    assert!(writer.flush().await.is_err());
    assert_eq!(std::fs::read(&poi_path).unwrap(), poi_before);
    chunk.set_custom_data("test", "refused", NbtTag::Int(7));
    writer.flush().await.unwrap();
    assert_eq!(
        PoiRegion::load(&poi_path).unwrap().entries[&(1, 0, 1)].poi_type,
        PoiType::Home.identifier()
    );
    assert_eq!(
        disk_chunk::<S>(&folder, pos, config)
            .await
            .section
            .get_block_absolute_y(1, 0, 1),
        Some(home())
    );
}

#[tokio::test]
async fn anvil_queued_capture_and_end_refusal_keep_terrain_poi_coherent() {
    Box::pin(format_capture::<AnvilChunkFile<ChunkData>>(
        pumpkin_config::chunk::AnvilChunkConfig::default(),
    ))
    .await;
}

#[tokio::test]
async fn linear_queued_capture_and_end_refusal_keep_terrain_poi_coherent() {
    Box::pin(format_capture::<LinearV2File<ChunkData>>(())).await;
}

#[tokio::test]
async fn pump_queued_capture_and_end_refusal_keep_terrain_poi_coherent() {
    Box::pin(format_capture::<PumpFile<ChunkData>>(())).await;
}

#[tokio::test]
async fn refused_poi_preserves_original_bytes_while_healthy_region_saves() {
    for extension in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let folder = folder(&directory);
        let runtime = TestRuntime::new(folder.poi_folder.clone());
        let mut nbt = nbt_chunk(&[(
            "0",
            section(Some(1), vec![record([1, 0, 1], "future:unknown", Some(-7))]),
        )]);
        if extension {
            nbt.put_int("FutureRoot", 3);
        }
        let path = folder.poi_folder.join("r.0.0.mca");
        write_mca(&path, &[(0, &compress_fixture(nbt))]);
        let before = std::fs::read(&path).unwrap();
        let refused = ChunkData::empty_sync(0, 0);
        let healthy = ChunkData::empty_sync(32, 0);
        for chunk in [&refused, &healthy] {
            chunk.bind_poi(&runtime.runtime).unwrap();
            chunk.set_block_absolute_y(1, 0, 1, home());
        }
        let writer = ChunkFileManager::<AnvilChunkFile<ChunkData>>::new(
            pumpkin_config::chunk::AnvilChunkConfig::default(),
        );
        assert!(
            writer
                .save_chunks(
                    &folder,
                    vec![
                        (ChunkPos::new(0, 0), refused.clone()),
                        (ChunkPos::new(32, 0), healthy)
                    ]
                )
                .await
                .is_err()
        );
        assert!(writer.flush().await.is_err());
        assert!(refused.is_dirty());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(!folder.region_folder.join("r.0.0.mca").exists());
        let healthy_poi = PoiRegion::load(&folder.poi_folder.join("r.1.0.mca")).unwrap();
        assert_eq!(
            healthy_poi.entries[&(513, 0, 1)].poi_type,
            PoiType::Home.identifier()
        );
        let healthy_terrain = disk_chunk::<AnvilChunkFile<ChunkData>>(
            &folder,
            ChunkPos::new(32, 0),
            pumpkin_config::chunk::AnvilChunkConfig::default(),
        )
        .await;
        assert_eq!(
            healthy_terrain.section.get_block_absolute_y(1, 0, 1),
            Some(home())
        );
    }
}

#[tokio::test]
async fn proto_capture_stays_incomplete_and_tracker_cannot_move_between_stores() {
    let directory = tempfile::tempdir().unwrap();
    let folder = folder(&directory);
    let runtime = TestRuntime::new(folder.poi_folder.clone());
    let writer = ChunkFileManager::<PumpFile<ChunkData>>::new(());
    let mut proto = ChunkData::empty(0, 0);
    proto.status = pumpkin_data::chunk::ChunkStatus::Features;
    proto.bind_poi(&runtime.runtime).unwrap();
    proto.set_block_absolute_y(1, 0, 1, home());
    let proto = Arc::new(proto);
    proto.to_bytes_for_save().unwrap();
    assert!(!folder.poi_folder.join("r.0.0.mca").exists());
    writer
        .save_chunks(&folder, vec![(ChunkPos::new(0, 0), proto.clone())])
        .await
        .unwrap();
    {
        let mut storage = runtime.runtime.storage.lock().unwrap();
        assert!(matches!(
            proto
                .read_for_poi()
                .reconcile(&mut storage.loaded_poi_region(ChunkPos::new(0, 0)).unwrap()),
            Err(PoiReconcileError::IncompleteTerrain)
        ));
    };
    let saved = PoiRegion::load(&directory.path().join("poi/r.0.0.mca")).unwrap();
    assert_eq!(saved.section_validity[&(0, 0, 0)], 0);
    assert_eq!(
        saved.entries[&(1, 0, 1)].poi_type,
        PoiType::Home.identifier()
    );
    let full = ChunkData::empty_sync(0, 0);
    full.bind_poi(&runtime.runtime).unwrap();
    full.set_block_absolute_y(1, 0, 1, home());
    writer
        .save_chunks(&folder, vec![(ChunkPos::new(0, 0), full.clone())])
        .await
        .unwrap();
    assert_eq!(
        PoiRegion::load(&directory.path().join("poi/r.0.0.mca"))
            .unwrap()
            .section_validity[&(0, 0, 0)],
        1
    );
    let other = TestRuntime::new(directory.path().join("other"));
    assert!(matches!(
        full.bind_poi(&other.runtime),
        Err(PoiReconcileError::WrongStorage)
    ));
    let mut storage = other.runtime.storage.lock().unwrap();
    storage.preload_poi_region(ChunkPos::new(0, 0)).unwrap();
    assert!(matches!(
        full.read_for_poi()
            .reconcile(&mut storage.loaded_poi_region(ChunkPos::new(0, 0)).unwrap()),
        Err(PoiReconcileError::WrongStorage)
    ));
}

// UNRUN: real backend paths, with both chunks in the same terrain and POI region.
fn assert_entry(poi: &PoiRegion, pos: (i32, i32, i32), kind: PoiType, tickets: i32) {
    assert_eq!(poi.entries[&pos].poi_type, kind.identifier());
    assert_eq!(poi.entries[&pos].free_tickets, tickets);
}

async fn assert_same_region_disk<S>(folder: &LevelFolder, config: S::ChunkConfig, retried: bool)
where
    S: ChunkSerializer<Data = ChunkData, WriteBackend = PathBuf>,
    S::ChunkConfig: Clone + Send + Sync,
{
    let a = disk_chunk::<S>(folder, ChunkPos::new(0, 0), config.clone()).await;
    let b = disk_chunk::<S>(folder, ChunkPos::new(1, 0), config).await;
    let a_kind = if retried {
        PoiType::Meeting
    } else {
        PoiType::Home
    };
    assert_eq!(
        a.section.get_block_absolute_y(1, 0, 1),
        Some(if retried {
            Block::BELL.default_state.id
        } else {
            home()
        })
    );
    assert_eq!(a.section.get_block_absolute_y(2, 0, 1), Some(home()));
    assert_eq!(
        b.section.get_block_absolute_y(1, 0, 1),
        Some(Block::BELL.default_state.id)
    );
    let poi = PoiRegion::load(&folder.poi_folder.join("r.0.0.mca")).unwrap();
    assert_entry(&poi, (1, 0, 1), a_kind, if retried { 32 } else { 0 });
    assert_entry(&poi, (2, 0, 1), PoiType::Home, i32::from(retried));
    assert_entry(&poi, (17, 0, 1), PoiType::Meeting, 32);
    assert_eq!(poi.section_validity[&(0, 0, 0)], 1);
    assert_eq!(poi.section_validity[&(1, 0, 0)], 1);
}

async fn same_region_encode_refusal<S>(config: S::ChunkConfig)
where
    S: ChunkSerializer<Data = ChunkData, WriteBackend = PathBuf>,
    S::ChunkConfig: Clone + Send + Sync + 'static,
{
    let directory = tempfile::tempdir().unwrap();
    let folder = folder(&directory);
    let runtime = TestRuntime::new(folder.poi_folder.clone());
    let a_pos = ChunkPos::new(0, 0);
    let b_pos = ChunkPos::new(1, 0);
    let a = ChunkData::empty_sync(0, 0);
    let b = ChunkData::empty_sync(1, 0);
    for (pos, chunk) in [(a_pos, &a), (b_pos, &b)] {
        chunk.bind_poi(&runtime.runtime).unwrap();
        runtime.chunks.insert(pos, chunk.clone());
        chunk.set_block_absolute_y(1, 0, 1, home());
    }
    a.set_block_absolute_y(2, 0, 1, home());
    let writer = ChunkFileManager::<S>::new(config.clone());
    writer
        .save_chunks(&folder, vec![(a_pos, a.clone()), (b_pos, b.clone())])
        .await
        .unwrap();
    writer.flush().await.unwrap();
    {
        let mut storage = runtime.runtime.storage.lock().unwrap();
        for x in [1, 2] {
            storage
                .add_with_free_tickets(BlockPos::new(x, 0, 1), PoiType::Home.identifier(), 0)
                .unwrap();
        }
        storage.save_all().unwrap();
    };
    let terrain_path = folder.region_folder.join(S::get_chunk_key(&a_pos));
    let terrain_before = std::fs::read(&terrain_path).unwrap();
    let poi_path = folder.poi_folder.join("r.0.0.mca");
    let poi_before = std::fs::read(&poi_path).unwrap();
    a.set_block_absolute_y(1, 0, 1, Block::BELL.default_state.id);
    a.set_block_absolute_y(2, 0, 1, BlockStateId::AIR);
    a.set_block_absolute_y(2, 0, 1, home());
    a.set_custom_data("test", "refused", NbtTag::End);
    assert!(matches!(
        a.prepare_poi(),
        Err(PoiReconcileError::TerrainEncoding(_))
    ));
    {
        let mut storage = runtime.runtime.storage.lock().unwrap();
        storage.save_all().unwrap();
        assert_eq!(std::fs::read(&poi_path).unwrap(), poi_before);
    };
    assert!(matches!(
        a.to_bytes_for_save(),
        Err(ChunkSerializingError::ErrorSerializingChunk(_))
    ));
    {
        let mut storage = runtime.runtime.storage.lock().unwrap();
        storage.save_all().unwrap();
        assert_eq!(std::fs::read(&poi_path).unwrap(), poi_before);
        // Force an actual same-region save_all write using an independent ticket edit.
        storage
            .add_with_free_tickets(BlockPos::new(17, 0, 1), PoiType::Home.identifier(), 0)
            .unwrap();
        storage.save_all().unwrap();
    };
    let poi = PoiRegion::load(&poi_path).unwrap();
    assert_entry(&poi, (1, 0, 1), PoiType::Home, 0);
    assert_entry(&poi, (2, 0, 1), PoiType::Home, 0);
    assert_entry(&poi, (17, 0, 1), PoiType::Home, 0);
    assert_eq!(std::fs::read(&terrain_path).unwrap(), terrain_before);
    b.set_block_absolute_y(1, 0, 1, Block::BELL.default_state.id);
    assert!(
        writer
            .save_chunks(&folder, vec![(a_pos, a.clone()), (b_pos, b)])
            .await
            .is_err()
    );
    assert!(writer.flush().await.is_err());
    assert!(a.is_dirty());
    let (send, mut receive) = tokio::sync::mpsc::channel(1);
    writer.fetch_chunks(&folder, &[a_pos], send).await;
    let Some(LoadedData::Loaded(retained)) = receive.recv().await else {
        panic!("failed source was discarded")
    };
    assert!(Arc::ptr_eq(&retained, &a));
    {
        let mut storage = runtime.runtime.storage.lock().unwrap();
        storage.save_all().unwrap();
    };
    assert_same_region_disk::<S>(&folder, config.clone(), false).await;
    a.set_custom_data("test", "refused", NbtTag::Int(7));
    let before_prepare = std::fs::read(&poi_path).unwrap();
    let stamp = a.prepare_poi().unwrap();
    {
        let terrain = a.read_for_poi();
        assert!(terrain.is_capture_current(&stamp));
    };
    runtime.runtime.storage.lock().unwrap().save_all().unwrap();
    assert_eq!(std::fs::read(&poi_path).unwrap(), before_prepare);
    writer.flush().await.unwrap();
    assert!(!a.is_dirty());
    assert_same_region_disk::<S>(&folder, config, true).await;
}

#[tokio::test]
async fn anvil_failed_encode_cannot_leak_through_same_region_sibling_or_save_all() {
    Box::pin(same_region_encode_refusal::<AnvilChunkFile<ChunkData>>(
        pumpkin_config::chunk::AnvilChunkConfig::default(),
    ))
    .await;
}

#[tokio::test]
async fn linear_failed_encode_cannot_leak_through_same_region_sibling_or_save_all() {
    Box::pin(same_region_encode_refusal::<LinearV2File<ChunkData>>(())).await;
}

#[tokio::test]
async fn pump_failed_encode_cannot_leak_through_same_region_sibling_or_save_all() {
    Box::pin(same_region_encode_refusal::<PumpFile<ChunkData>>(())).await;
}

// UNRUN: a later refused section must not publish/ack an earlier valid plan.
#[tokio::test]
async fn later_section_refusal_keeps_earlier_save_plan_and_aba_history_unpublished() {
    let directory = tempfile::tempdir().unwrap();
    let folder = folder(&directory);
    let runtime = TestRuntime::new(folder.poi_folder.clone());
    let chunk = ChunkData::empty_sync(0, 0);
    chunk.bind_poi(&runtime.runtime).unwrap();
    runtime.chunks.insert(ChunkPos::new(0, 0), chunk.clone());
    chunk.set_block_absolute_y(1, 0, 1, home());
    let writer = ChunkFileManager::<PumpFile<ChunkData>>::new(());
    writer
        .save_chunks(&folder, vec![(ChunkPos::new(0, 0), chunk.clone())])
        .await
        .unwrap();
    {
        let mut storage = runtime.runtime.storage.lock().unwrap();
        storage
            .add_with_free_tickets(BlockPos::new(1, 0, 1), PoiType::Home.identifier(), 0)
            .unwrap();
        storage
            .add_with_free_tickets(BlockPos::new(1, 16, 1), "future:unknown", -7)
            .unwrap();
        storage.save_all().unwrap();
    };
    let poi_path = folder.poi_folder.join("r.0.0.mca");
    let before = std::fs::read(&poi_path).unwrap();
    chunk.set_block_absolute_y(1, 0, 1, BlockStateId::AIR);
    chunk.set_block_absolute_y(1, 0, 1, home());
    chunk.set_block_absolute_y(1, 16, 1, Block::BELL.default_state.id);
    assert!(matches!(
        chunk.prepare_poi(),
        Err(PoiReconcileError::UnknownType { .. })
    ));
    assert!(matches!(
        chunk.to_bytes_for_save(),
        Err(ChunkSerializingError::Poi(
            PoiReconcileError::UnknownType { .. }
        ))
    ));
    {
        let mut storage = runtime.runtime.storage.lock().unwrap();
        storage.save_all().unwrap();
        assert_eq!(std::fs::read(&poi_path).unwrap(), before);
        assert_entry(&storage.regions[&(0, 0)], (1, 0, 1), PoiType::Home, 0);
        assert_eq!(
            storage.regions[&(0, 0)].entries[&(1, 16, 1)].poi_type,
            "future:unknown"
        );
        assert_eq!(
            storage.regions[&(0, 0)].entries[&(1, 16, 1)].free_tickets,
            -7
        );
        // Explicitly repair the refused input; no lossy automatic admission.
        storage.remove(&BlockPos::new(1, 16, 1)).unwrap();
    };
    writer
        .save_chunks(&folder, vec![(ChunkPos::new(0, 0), chunk.clone())])
        .await
        .unwrap();
    let poi = PoiRegion::load(&poi_path).unwrap();
    assert_entry(&poi, (1, 0, 1), PoiType::Home, 1);
    assert_entry(&poi, (1, 16, 1), PoiType::Meeting, 32);
}

// UNRUN: normal save waits on a real backend's reserved physical POI write.
#[allow(
    clippy::too_many_lines,
    reason = "Keep actual backend wait, independent-region progress and current-version disk proof together."
)]
async fn managed_save_waits_for_completion<S>(config: S::ChunkConfig)
where
    S: ChunkSerializer<Data = ChunkData, WriteBackend = PathBuf>,
    S::ChunkConfig: Clone + Send + Sync + 'static,
{
    let directory = tempfile::tempdir().unwrap();
    let folder = folder(&directory);
    let runtime = TestRuntime::new(folder.poi_folder.clone());
    let storage = &runtime.runtime.storage;
    let pos = ChunkPos::new(0, 0);
    let source = ChunkData::empty_sync(0, 0);
    source.bind_poi(&runtime.runtime).unwrap();
    runtime.chunks.insert(pos, source.clone());
    source.set_block_absolute_y(1, 0, 1, home());
    let writer = Arc::new(ChunkFileManager::<S>::new(config.clone()));
    writer.watch_chunks(&folder, &[pos]).await;
    writer
        .save_chunks(&folder, vec![(pos, source.clone())])
        .await
        .unwrap();
    writer.flush().await.unwrap();
    let poi_path = folder.poi_folder.join("r.0.0.mca");
    let before = std::fs::read(&poi_path).unwrap();
    source.set_block_absolute_y(1, 0, 1, Block::BELL.default_state.id);
    writer
        .save_chunks(&folder, vec![(pos, source.clone())])
        .await
        .unwrap();
    let (entered, release) = storage.lock().unwrap().pause_region_write(pos);
    let flushing = writer.clone();
    let flush = tokio::spawn(async move { flushing.flush().await });
    tokio::time::timeout(std::time::Duration::from_secs(5), entered)
        .await
        .unwrap()
        .unwrap();

    let mut cancelled = Box::pin(PoiStorage::save_all_managed(storage));
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(cancelled.as_mut().poll(cx)))
            .await
            .is_pending()
    );
    // The only initially dirty region is reserved by the paused backend. This
    // poll therefore reached its actual gate, not a task-start notification.
    drop(cancelled);
    assert!(runtime.runtime.lifecycle.try_write().is_ok());
    assert!(runtime.chunks.try_get(&pos).is_present());
    {
        let mut live = storage
            .try_lock()
            .expect("waiting save must release storage");
        assert_eq!(
            live.save_all().unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        live.add_with_free_tickets(BlockPos::new(1, 0, 1), PoiType::Meeting.identifier(), -9)
            .unwrap()
    };
    assert_eq!(std::fs::read(&poi_path).unwrap(), before);
    let healthy_gate = {
        let mut live = storage.lock().unwrap();
        live.add_portal(BlockPos::new(513, 0, 1)).unwrap();
        live.regions[&(1, 0)].write_active.clone()
    };
    let mut save = Box::pin(PoiStorage::save_all_managed(storage));
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(save.as_mut().poll(cx)))
            .await
            .is_pending()
    );
    // This batch must drain the healthy region while the first remains paused,
    // rather than collecting every gate before it dispatches any region's IO.
    drop(
        tokio::time::timeout(std::time::Duration::from_secs(5), healthy_gate.lock_owned())
            .await
            .unwrap(),
    );
    assert_eq!(
        PoiRegion::load(&folder.poi_folder.join("r.1.0.mca"))
            .unwrap()
            .get_all()[0]
            .pos(),
        BlockPos::new(513, 0, 1)
    );
    // This new region is outside the finite initial batch and stays dirty.
    storage
        .lock()
        .unwrap()
        .add_portal(BlockPos::new(1025, 0, 1))
        .unwrap();
    release.send(()).unwrap();
    flush.await.unwrap().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), save)
        .await
        .unwrap()
        .unwrap();
    assert_entry(
        &PoiRegion::load(&poi_path).unwrap(),
        (1, 0, 1),
        PoiType::Meeting,
        -9,
    );
    {
        let live = storage.lock().unwrap();
        assert!(!live.region_is_dirty(pos));
        assert!(!live.region_is_dirty(ChunkPos::new(32, 0)));
        assert!(live.region_is_dirty(ChunkPos::new(64, 0)));
        assert!(!folder.poi_folder.join("r.2.0.mca").exists())
    };
    assert_eq!(
        disk_chunk::<S>(&folder, pos, config)
            .await
            .section
            .get_block_absolute_y(1, 0, 1),
        Some(Block::BELL.default_state.id)
    );
}

#[tokio::test]
async fn all_formats_managed_save_waits_for_completion_and_captures_current_poi() {
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        Box::pin(
            managed_save_waits_for_completion::<AnvilChunkFile<ChunkData>>(
                pumpkin_config::chunk::AnvilChunkConfig::default(),
            ),
        )
        .await;
        Box::pin(managed_save_waits_for_completion::<LinearV2File<ChunkData>>(())).await;
        Box::pin(managed_save_waits_for_completion::<PumpFile<ChunkData>>(())).await;
    })
    .await
    .unwrap();
}

// UNRUN: reverse contention retains the exact backend receipt, including when
// the normal caller is cancelled while its already-owned IO is still running.
async fn managed_owner_preserves_backend_retry<S>(config: S::ChunkConfig)
where
    S: ChunkSerializer<Data = ChunkData, WriteBackend = PathBuf>,
    S::ChunkConfig: Clone + Send + Sync + 'static,
{
    let directory = tempfile::tempdir().unwrap();
    let folder = folder(&directory);
    let runtime = TestRuntime::new(folder.poi_folder.clone());
    let storage = runtime.runtime.storage.clone();
    let pos = ChunkPos::new(0, 0);
    storage
        .lock()
        .unwrap()
        .add_portal(BlockPos::new(2, 0, 2))
        .unwrap();
    let (entered, release) = storage.lock().unwrap().pause_region_write(pos);
    let saving = storage.clone();
    let save = tokio::spawn(async move { PoiStorage::save_all_managed(&saving).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), entered)
        .await
        .unwrap()
        .unwrap();
    save.abort();
    assert!(save.await.unwrap_err().is_cancelled());
    assert_eq!(
        storage.try_lock().unwrap().save_all().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    let source = ChunkData::empty_sync(0, 0);
    source.bind_poi(&runtime.runtime).unwrap();
    source.set_block_absolute_y(1, 0, 1, home());
    let original = Arc::downgrade(&source);
    let writer = ChunkFileManager::<S>::new(config.clone());
    writer.watch_chunks(&folder, &[pos]).await;
    writer
        .save_chunks(&folder, vec![(pos, source)])
        .await
        .unwrap();
    assert!(
        matches!(writer.flush().await, Err(ChunkWritingError::IoError(error)) if error.kind() == io::ErrorKind::WouldBlock)
    );
    assert!(original.upgrade().is_some());
    let terrain_path = folder.region_folder.join(S::get_chunk_key(&pos));
    let terrain_before = std::fs::read(&terrain_path).unwrap();
    storage
        .lock()
        .unwrap()
        .add_with_free_tickets(BlockPos::new(1, 0, 1), PoiType::Home.identifier(), -13)
        .unwrap();
    let gate = storage.lock().unwrap().regions[&(0, 0)]
        .write_active
        .clone();
    release.send(()).unwrap();
    // Fence the cancelled normal owner's real ack, not just its caller task.
    drop(
        tokio::time::timeout(std::time::Duration::from_secs(5), gate.lock_owned())
            .await
            .unwrap(),
    );
    assert!(storage.lock().unwrap().region_is_dirty(pos));
    writer.flush().await.unwrap();
    assert_eq!(std::fs::read(&terrain_path).unwrap(), terrain_before);
    assert_entry(
        &PoiRegion::load(&folder.poi_folder.join("r.0.0.mca")).unwrap(),
        (1, 0, 1),
        PoiType::Home,
        -13,
    );
    assert!(original.upgrade().is_none());
    assert!(!storage.lock().unwrap().region_is_dirty(pos));
    assert_eq!(
        disk_chunk::<S>(&folder, pos, config)
            .await
            .section
            .get_block_absolute_y(1, 0, 1),
        Some(home())
    );
}

#[tokio::test]
async fn all_formats_managed_owner_cancellation_preserves_exact_backend_retry() {
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        Box::pin(managed_owner_preserves_backend_retry::<
            AnvilChunkFile<ChunkData>,
        >(pumpkin_config::chunk::AnvilChunkConfig::default()))
        .await;
        Box::pin(managed_owner_preserves_backend_retry::<
            LinearV2File<ChunkData>,
        >(()))
        .await;
        Box::pin(managed_owner_preserves_backend_retry::<PumpFile<ChunkData>>(())).await;
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn managed_save_reports_real_io_and_load_refusals_after_healthy_region_writes() {
    let directory = tempfile::tempdir().unwrap();
    let folder = folder(&directory);
    let storage = Arc::new(Mutex::new(PoiStorage::new(folder.poi_folder.clone())));
    let corrupt = folder.poi_folder.join("r.2.0.mca");
    std::fs::write(&corrupt, b"refused original POI").unwrap();
    let blocked = folder.poi_folder.join("r.0.0.mca");
    {
        let mut live = storage.lock().unwrap();
        live.add_portal(BlockPos::new(1, 0, 1)).unwrap();
        live.add_portal(BlockPos::new(513, 0, 1)).unwrap();
        assert!(live.preload_poi_region(ChunkPos::new(64, 0)).is_err())
    };
    std::fs::create_dir(&blocked).unwrap();
    assert!(PoiStorage::save_all_managed(&storage).await.is_err());
    assert_eq!(std::fs::read(&corrupt).unwrap(), b"refused original POI");
    assert!(blocked.is_dir());
    assert_eq!(
        PoiRegion::load(&folder.poi_folder.join("r.1.0.mca"))
            .unwrap()
            .get_all()[0]
            .pos(),
        BlockPos::new(513, 0, 1)
    );
    {
        let live = storage.lock().unwrap();
        assert!(live.region_is_dirty(ChunkPos::new(0, 0)));
        assert!(!live.region_is_dirty(ChunkPos::new(32, 0)))
    };
    std::fs::remove_dir(&blocked).unwrap();
    // Repairing IO does not clear an unrelated sticky lazy-load refusal.
    assert_eq!(
        PoiStorage::save_all_managed(&storage)
            .await
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        PoiRegion::load(&blocked).unwrap().get_all()[0].pos(),
        BlockPos::new(1, 0, 1)
    );
    assert_eq!(std::fs::read(&corrupt).unwrap(), b"refused original POI");
}

#[test]
fn cancelled_queued_managed_save_keeps_gate_until_actual_ack() {
    tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(15), async {
                let directory = tempfile::tempdir().unwrap();
                let storage = Arc::new(Mutex::new(PoiStorage::new(directory.path().to_owned())));
                let position = BlockPos::new(1, 0, 1);
                storage
                    .lock()
                    .unwrap()
                    .add_with_free_tickets(position, PoiType::Home.identifier(), 1)
                    .unwrap();
                let (entered, ready) = tokio::sync::oneshot::channel();
                let (release, released) = std::sync::mpsc::channel();
                let blocker = tokio::task::spawn_blocking(move || {
                    entered.send(()).unwrap();
                    released
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .unwrap();
                });
                ready.await.unwrap();
                let mut cancelled = Box::pin(PoiStorage::save_all_managed(&storage));
                assert!(
                    std::future::poll_fn(|cx| std::task::Poll::Ready(cancelled.as_mut().poll(cx)))
                        .await
                        .is_pending()
                );
                drop(cancelled);
                {
                    let mut live = storage.try_lock().unwrap();
                    assert_eq!(
                        live.save_all().unwrap_err().kind(),
                        io::ErrorKind::WouldBlock
                    );
                    live.add_with_free_tickets(position, PoiType::Home.identifier(), -17)
                        .unwrap()
                };
                let mut next = Box::pin(PoiStorage::save_all_managed(&storage));
                assert!(
                    std::future::poll_fn(|cx| std::task::Poll::Ready(next.as_mut().poll(cx)))
                        .await
                        .is_pending()
                );
                assert!(!directory.path().join("r.0.0.mca").exists());
                release.send(()).unwrap();
                blocker.await.unwrap();
                next.await.unwrap();
                assert_entry(
                    &PoiRegion::load(&directory.path().join("r.0.0.mca")).unwrap(),
                    (1, 0, 1),
                    PoiType::Home,
                    -17,
                );
                let live = storage.lock().unwrap();
                assert!(!live.region_is_dirty(ChunkPos::new(0, 0)));
                assert_eq!(live.completed_region_writes(ChunkPos::new(0, 0)), 2);
            })
            .await
            .unwrap();
        });
}
