//! Stage B managed-lifetime source contracts. Both tests are UNRUN.

use super::*;
use crate::{
    chunk::{ChunkData, ChunkSerializingError, format::anvil::SingleChunkDataSerializer},
    generation::proto_chunk::GenerationCache,
    poi::{PoiReconcileError, PoiRegion},
};
use pumpkin_config::world::LevelConfig;
use pumpkin_data::{Block, BlockState, BlockStateId, dimension::Dimension, poi::PoiType};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

struct Workers {
    write: tokio::sync::mpsc::Receiver<WriteRequest>,
    read: tokio::sync::mpsc::Receiver<Vec<ChunkPos>>,
}

// Real Level/store and real scheduler data, with automatic workers drained so
// these tests control the existing queue, occupancy and IO fences explicitly.
async fn fixture() -> (tempfile::TempDir, Arc<Level>, GenerationSchedule, Workers) {
    let directory = tempfile::tempdir().unwrap();
    let level = Level::from_root_folder(
        &LevelConfig::default(),
        directory.path().to_owned(),
        0,
        Dimension::OVERWORLD,
    );
    level.shutdown().await.unwrap();
    let (send_chunk, recv_chunk) = crossbeam::channel::unbounded();
    let (io_write, write) = tokio::sync::mpsc::channel(16);
    let (io_read, read) = tokio::sync::mpsc::channel(16);
    let schedule = GenerationSchedule {
        queue: BinaryHeap::new(),
        graph: DAG::default(),
        last_level: ChunkLevel::default(),
        last_high_priority: Vec::new(),
        send_level: level.level_channel.clone(),
        public_chunk_map: level.loaded_chunks.clone(),
        loaded_chunk_changes: level.loaded_chunk_changes.clone(),
        chunk_lifecycle: level.chunk_lifecycle.clone(),
        chunk_unload: level.chunk_unload.clone(),
        chunk_activation_changed: level.chunk_activation_changed.clone(),
        chunk_activation_revision: level.chunk_activation_revision.clone(),
        chunk_map: HashMap::new(),
        unload_chunks: HashSetType::default(),
        waiting_for_chunks: HashSetType::default(),
        io_lock: Arc::new((
            Mutex::new(HashMapType::default()),
            tokio::sync::Notify::new(),
        )),
        running_task_count: 0,
        max_in_flight: 1,
        queue_dirty: false,
        recv_chunk,
        io_read,
        io_write,
        send_chunk,
        listener: level.chunk_listener.clone(),
        lighting_config: level.lighting_config,
        last_unload: std::time::Instant::now(),
        generation_pool: Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(1)
                .build()
                .unwrap(),
        ),
        fatal_save_failure: level.fatal_save_failure.clone(),
    };
    (directory, level, schedule, Workers { write, read })
}

fn publish(schedule: &mut GenerationSchedule, level: &Level, pos: ChunkPos) -> SyncChunk {
    let chunk = ChunkData::empty_sync(pos.x, pos.y);
    chunk.light_populated.store(true, Relaxed);
    level.bind_chunk_poi(&chunk).unwrap();
    schedule.publish_chunk(pos, &chunk);
    schedule.chunk_map.insert(
        pos,
        ChunkHolder {
            current_stage: StagedChunkEnum::Full,
            chunk: Some(Chunk::Level(chunk.clone())),
            public: true,
            ..ChunkHolder::default()
        },
    );
    chunk
}

async fn unload(mut schedule: GenerationSchedule, level: Arc<Level>) -> GenerationSchedule {
    tokio::task::spawn_blocking(move || {
        schedule.process_unload_queue(&level);
        schedule
    })
    .await
    .unwrap()
}

fn home() -> BlockStateId {
    Block::WHITE_BED
        .states
        .iter()
        .map(|s| s.id)
        .find(|s| PoiType::from_state(*s) == Some(PoiType::Home))
        .unwrap()
}

#[tokio::test]
async fn no_be_retirement_rechecks_late_level_and_occupied_generation_writes() {
    let (_directory, level, mut schedule, mut workers) = fixture().await;
    let pos = ChunkPos::new(0, 0);
    let chunk = publish(&mut schedule, &level, pos);
    level.set_block_state(&BlockPos::new(1, 0, 1), home());
    schedule.unload_chunks.insert(pos);
    schedule = unload(schedule, level.clone()).await; // prepares without any BEs
    assert!(level.loaded_chunks.contains_key(&pos));
    assert!(workers.write.try_recv().is_err());
    level.set_block_state(&BlockPos::new(1, 0, 1), Block::BELL.default_state.id);
    schedule = unload(schedule, level.clone()).await; // late revision must defer
    assert!(level.loaded_chunks.contains_key(&pos));
    assert!(workers.write.try_recv().is_err());

    // Use the actual Cache Level writer while its holder is occupied and its
    // chunk moved into Cache, exactly the ownership used by generation dispatch.
    let fence = schedule
        .graph
        .nodes
        .insert(Node::new(pos, StagedChunkEnum::Features));
    let holder = schedule.chunk_map.get_mut(&pos).unwrap();
    holder.occupied = fence;
    let mut cache = Cache::new(0, 0, 1);
    cache.chunks.push(holder.chunk.take().unwrap());
    chunk.mark_dirty(false);
    cache.set_block_state(&Vector3::new(1, 0, 1), BlockState::from_id(home()));
    assert!(chunk.is_dirty());
    schedule = unload(schedule, level.clone()).await;
    assert!(level.loaded_chunks.contains_key(&pos));
    assert!(workers.write.try_recv().is_err());
    let holder = schedule.chunk_map.get_mut(&pos).unwrap();
    holder.chunk = cache.chunks.pop();
    holder.occupied = NodeKey::null();
    schedule.graph.nodes.remove(fence);
    schedule = unload(schedule, level.clone()).await; // fresh preparation
    assert!(level.loaded_chunks.contains_key(&pos));
    schedule = unload(schedule, level.clone()).await; // exclusive entry + stamp
    assert!(!level.loaded_chunks.contains_key(&pos));
    let WriteRequest::Chunks(saved) = workers.write.try_recv().unwrap() else {
        panic!("expected terrain save")
    };
    let Chunk::Level(source) = &saved[0].1 else {
        panic!("expected Full terrain")
    };
    assert!(Arc::ptr_eq(source, &chunk));
    source.to_bytes_for_save().unwrap();
    let poi = PoiRegion::load(&level.level_folder.poi_folder.join("r.0.0.mca")).unwrap();
    assert!(
        poi.get_all()
            .iter()
            .any(|entry| entry.poi_type == PoiType::Home.identifier() && entry.free_tickets == 1)
    );
    assert!(schedule.chunk_map.is_empty());
}

#[tokio::test]
async fn queued_retirement_a_fences_replacement_b_until_actual_writer_finishes() {
    let (_directory, level, mut schedule, workers) = fixture().await;
    let pos = ChunkPos::new(0, 0);
    let a = publish(&mut schedule, &level, pos);
    level.set_block_state(&BlockPos::new(1, 0, 1), home());
    schedule.unload_chunks.insert(pos);
    schedule = unload(schedule, level.clone()).await;
    schedule = unload(schedule, level.clone()).await;
    assert!(!level.loaded_chunks.contains_key(&pos));
    assert!(schedule.io_lock.0.lock().unwrap().contains_key(&pos));

    let (held, ready) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel();
    let source = a.clone();
    let blocker = std::thread::spawn(move || {
        let _light = source.light_engine.lock().unwrap();
        held.send(()).unwrap();
        released.recv().unwrap();
    });
    ready.await.unwrap();
    let reader = tokio::spawn(io_read_work(
        Arc::new(tokio::sync::Mutex::new(workers.read)),
        schedule.send_chunk.clone(),
        level.clone(),
        schedule.io_lock.clone(),
    ));
    let writer = tokio::spawn(io_write_work(
        workers.write,
        level.clone(),
        schedule.io_lock.clone(),
    ));
    schedule.io_read.send(vec![pos]).await.unwrap();
    // The existing read worker cannot return B while A owns the queued IO lock.
    let receive = schedule.recv_chunk.clone();
    assert!(
        tokio::task::spawn_blocking(move || receive.recv_timeout(Duration::from_millis(50)))
            .await
            .unwrap()
            .is_err()
    );
    assert!(schedule.io_lock.0.lock().unwrap().contains_key(&pos));
    release.send(()).unwrap();
    blocker.join().unwrap();
    let receive = schedule.recv_chunk.clone();
    let (received_pos, received) =
        tokio::task::spawn_blocking(move || receive.recv_timeout(Duration::from_secs(5)))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(received_pos, pos);
    let RecvChunk::IO(Chunk::Level(b)) = received else {
        panic!("replacement Full read failed")
    };
    assert!(!Arc::ptr_eq(&a, &b));
    assert!(b.has_poi_binding());
    assert!(!schedule.io_lock.0.lock().unwrap().contains_key(&pos));
    assert_eq!(b.section.get_block_absolute_y(1, 0, 1), Some(home()));
    schedule.publish_chunk(pos, &b);
    let poi_path = level.level_folder.poi_folder.join("r.0.0.mca");
    let before = std::fs::read(&poi_path).unwrap();
    assert!(matches!(
        a.to_bytes_for_save(),
        Err(ChunkSerializingError::Poi(
            PoiReconcileError::ObsoletePublication
        ))
    ));
    assert_eq!(std::fs::read(&poi_path).unwrap(), before);
    level.set_block_state(&BlockPos::new(1, 0, 1), Block::BELL.default_state.id);
    schedule = tokio::task::spawn_blocking(move || {
        schedule
            .queue_chunk_save(vec![(pos, Chunk::Level(b))])
            .unwrap();
        schedule
    })
    .await
    .unwrap();
    let (send, finished) = tokio::sync::oneshot::channel();
    schedule
        .io_write
        .send(WriteRequest::Flush(vec![send]))
        .await
        .unwrap();
    finished.await.unwrap().unwrap();
    assert!(
        PoiRegion::load(&poi_path)
            .unwrap()
            .get_all()
            .iter()
            .any(
                |entry| entry.poi_type == PoiType::Meeting.identifier() && entry.free_tickets == 32
            )
    );
    drop(schedule);
    reader.await.unwrap();
    writer.await.unwrap();
}
