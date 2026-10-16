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

#[test]
fn proto_capture_stays_incomplete_and_tracker_cannot_move_between_stores() {
    let directory = tempfile::tempdir().unwrap();
    let runtime = TestRuntime::new(directory.path().join("poi"));
    let mut proto = ChunkData::empty(0, 0);
    proto.status = pumpkin_data::chunk::ChunkStatus::Features;
    proto.bind_poi(&runtime.runtime).unwrap();
    proto.set_block_absolute_y(1, 0, 1, home());
    proto.to_bytes_for_save().unwrap();
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
    let full = ChunkData::empty(0, 0);
    full.bind_poi(&runtime.runtime).unwrap();
    full.set_block_absolute_y(1, 0, 1, home());
    full.to_bytes_for_save().unwrap();
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
