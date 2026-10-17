//! Stage B managed-lifetime source contracts. Both tests are UNRUN.

use super::*;
use crate::{
    chunk::{
        ChunkData, ChunkSerializingError, format::anvil::SingleChunkDataSerializer, io::FileIO,
    },
    generation::proto_chunk::GenerationCache,
    poi::{PoiReconcileError, PoiRegion},
};
use pumpkin_config::world::LevelConfig;
use pumpkin_data::{Block, BlockState, BlockStateId, dimension::Dimension, poi::PoiType};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

struct Workers {
    write: tokio::sync::mpsc::Receiver<WriteRequest>,
    read: tokio::sync::mpsc::Receiver<ReadRequest>,
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
    // Private encoding cannot publish POI. Exercise the real post-file boundary.
    level
        .chunk_saver
        .save_chunks(&level.level_folder, vec![(pos, source.clone())])
        .await
        .unwrap();
    level.chunk_saver.flush().await.unwrap();
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
    schedule
        .io_read
        .send(ReadRequest::Chunks(vec![pos]))
        .await
        .unwrap();
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

// C2 additions below are UNRUN. The two Stage B scenarios above are unchanged
// except the approved ReadRequest::Chunks envelope around the same coordinates.
fn request_area(
    schedule: &GenerationSchedule,
    level: &Level,
    pos: ChunkPos,
) -> tokio::sync::oneshot::Receiver<Result<Vec<PoiEntry>, PoiQueryError>> {
    let area =
        crate::poi::query::PoiArea::new(BlockPos::new(pos.x * 16, 0, pos.y * 16), 0).unwrap();
    let columns = {
        let mut storage = level.poi_storage.lock().unwrap();
        storage.preload_area(area).unwrap();
        storage.columns_needing_terrain(area, -4, 20).unwrap()
    };
    let (reply, receive) = tokio::sync::oneshot::channel();
    schedule
        .send_level
        .request_poi(PoiAreaReadRequest {
            area,
            columns,
            min_section: -4,
            end_section: 20,
            reply,
        })
        .unwrap();
    receive
}

async fn await_private_read(schedule: &GenerationSchedule, query: &mut PendingPoiQuery) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(result) = query.poll_disk(&schedule.io_read, &schedule.io_lock) {
                result.unwrap();
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

fn start_reader(
    schedule: &GenerationSchedule,
    level: &Arc<Level>,
    read: tokio::sync::mpsc::Receiver<ReadRequest>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(io_read_work(
        Arc::new(tokio::sync::Mutex::new(read)),
        schedule.send_chunk.clone(),
        level.clone(),
        schedule.io_lock.clone(),
    ))
}

async fn ordinary_read_result(schedule: &GenerationSchedule) -> (ChunkPos, RecvChunk) {
    let receive = schedule.recv_chunk.clone();
    tokio::task::spawn_blocking(move || receive.recv_timeout(Duration::from_secs(5)))
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn cold_query_waits_for_queued_a_and_fences_later_ordinary_b_through_capture() {
    let (_directory, level, mut schedule, workers) = fixture().await;
    schedule.send_level = Arc::new(LevelChannel::new());
    let pos = ChunkPos::new(0, 0);
    let a = publish(&mut schedule, &level, pos);
    a.set_block_absolute_y(0, 0, 0, home());
    schedule = tokio::task::spawn_blocking(move || {
        let chunk = schedule.unpublish_chunk(pos).unwrap();
        schedule.chunk_map.remove(&pos);
        schedule
            .queue_chunk_save(vec![(pos, Chunk::Level(chunk))])
            .unwrap();
        schedule
    })
    .await
    .unwrap();
    let result = request_area(&schedule, &level, pos);
    let mut active = None;
    schedule.poll_poi_requests(&level, &mut active);
    assert!(active.as_ref().unwrap().reservation.is_none());
    assert!(
        active
            .as_ref()
            .unwrap()
            .blocks(&Node::new(pos, StagedChunkEnum::Empty))
    );
    let reader = start_reader(&schedule, &level, workers.read);
    let writer = tokio::spawn(io_write_work(
        workers.write,
        level.clone(),
        schedule.io_lock.clone(),
    ));
    await_private_read(&schedule, active.as_mut().unwrap()).await;
    let query = active.as_ref().unwrap();
    assert!(
        query.disk.as_ref().unwrap()[0].sections.iter().any(
            |section| section.section_y == 0 && section.entries().contains(&(0, PoiType::Home))
        )
    );
    assert!(schedule.io_lock.0.lock().unwrap()[&pos].poi.is_some());
    schedule
        .io_read
        .send(ReadRequest::Chunks(vec![pos]))
        .await
        .unwrap();
    let receive = schedule.recv_chunk.clone();
    assert!(
        tokio::task::spawn_blocking(move || receive.recv_timeout(Duration::from_millis(50)))
            .await
            .unwrap()
            .is_err()
    );
    schedule.poll_poi_requests(&level, &mut active);
    let entries = result.await.unwrap().unwrap();
    assert!(
        entries
            .iter()
            .any(|entry| entry.poi_type == PoiType::Home.identifier())
    );
    assert!(active.is_none());
    let (_, received) = ordinary_read_result(&schedule).await;
    let RecvChunk::IO(Chunk::Level(b)) = received else {
        panic!("ordinary Full read")
    };
    assert!(!Arc::ptr_eq(&a, &b));
    schedule.publish_chunk(pos, &b);
    assert_eq!(b.section.get_block_absolute_y(0, 0, 0), Some(home()));
    assert!(!schedule.io_lock.0.lock().unwrap().contains_key(&pos));
    drop(schedule);
    reader.await.unwrap();
    writer.await.unwrap();
}

#[tokio::test]
async fn cancelled_and_closed_queries_release_reservations_for_following_normal_io() {
    let (_directory, level, mut schedule, workers) = fixture().await;
    schedule.send_level = Arc::new(LevelChannel::new());
    let pos = ChunkPos::new(0, 0);
    let cancelled = request_area(&schedule, &level, pos);
    let mut active = None;
    schedule.poll_poi_requests(&level, &mut active);
    assert!(schedule.io_lock.0.lock().unwrap().contains_key(&pos));
    drop(cancelled);
    schedule.poll_poi_requests(&level, &mut active);
    assert!(active.is_none());
    assert!(!schedule.io_lock.0.lock().unwrap().contains_key(&pos));
    let closed = request_area(&schedule, &level, pos);
    schedule.poll_poi_requests(&level, &mut active);
    assert!(active.is_some());
    let queued = request_area(&schedule, &level, ChunkPos::new(1, 0));
    schedule.close_poi_queries(&mut active);
    assert!(closed.await.is_err());
    assert!(matches!(queued.await.unwrap(), Err(PoiQueryError::Closed)));
    assert!(schedule.io_lock.0.lock().unwrap().is_empty());
    let reader = start_reader(&schedule, &level, workers.read);
    schedule
        .io_read
        .send(ReadRequest::Chunks(vec![pos]))
        .await
        .unwrap();
    let (received_pos, received) = ordinary_read_result(&schedule).await;
    assert_eq!(received_pos, pos);
    assert!(matches!(received, RecvChunk::IO(Chunk::Proto(_))));
    assert!(schedule.io_lock.0.lock().unwrap().is_empty());
    drop(schedule);
    reader.await.unwrap();
}

#[tokio::test]
async fn occupied_proto_query_uses_latest_returned_terrain_and_original_read_footprint() {
    let (_directory, level, mut schedule, _workers) = fixture().await;
    schedule.send_level = Arc::new(LevelChannel::new());
    let decoded = crate::poi::query_tests::sparse_read_fixture(&level.level_folder.poi_folder);
    let mut proto = crate::ProtoChunk::from_chunk_data(&decoded, &level.world_gen.load());
    assert_eq!(proto.poi_read_sections(), &[0, 2]);
    proto.set_block_state(1, 0, 1, BlockState::from_id(home()));
    let pos = ChunkPos::new(0, 0);
    let occupy = schedule
        .graph
        .nodes
        .insert(Node::new(pos, StagedChunkEnum::None));
    schedule.chunk_map.insert(
        pos,
        ChunkHolder {
            current_stage: StagedChunkEnum::Empty,
            occupied: occupy,
            ..ChunkHolder::default()
        },
    );
    let mut cache = Cache::new(0, 0, 1);
    cache.chunks.push(Chunk::Proto(Box::new(proto)));
    let area = crate::poi::query::PoiArea::new(BlockPos::new(1, 0, 1), 0).unwrap();
    level
        .poi_storage
        .lock()
        .unwrap()
        .preload_area(area)
        .unwrap();
    let path = level.level_folder.poi_folder.join("r.0.0.mca");
    let before = std::fs::read(&path).unwrap();
    let (reply, mut receive) = tokio::sync::oneshot::channel();
    schedule
        .send_level
        .request_poi(PoiAreaReadRequest {
            area,
            columns: vec![pos],
            min_section: -4,
            end_section: 20,
            reply,
        })
        .unwrap();
    let mut active = None;
    schedule.poll_poi_requests(&level, &mut active);
    assert!(matches!(
        receive.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    ));
    cache.set_block_state(&Vector3::new(1, 0, 1), Block::BELL.default_state);
    let holder = schedule.chunk_map.get_mut(&pos).unwrap();
    holder.chunk = cache.chunks.pop();
    holder.occupied = NodeKey::null();
    schedule.graph.nodes.remove(occupy);
    schedule.poll_poi_requests(&level, &mut active);
    let entries = receive.await.unwrap().unwrap();
    assert_eq!(
        entries.iter().find(|entry| entry.y == 0).unwrap().poi_type,
        PoiType::Meeting.identifier()
    );
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
    assert!(level.loaded_chunks.is_empty());
    level.poi_storage.lock().unwrap().save_all().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let mut generated = crate::ProtoChunk::new(0, 0, &level.world_gen.load());
    generated.set_block_state(1, 0, 1, Block::BELL.default_state);
    assert!(
        PoiTerrainOverlay::classify_resident(&generated)
            .sections
            .is_empty()
    );
}

// C2 cancellation follow-up: actual pending-write failure and retry, UNRUN.
async fn pending_failed_poi_write(schedule: &mut GenerationSchedule, level: &Level) -> SyncChunk {
    use crate::chunk::io::FileIO;
    let pos = ChunkPos::new(0, 0);
    let source = publish(schedule, level, pos);
    source.set_block_absolute_y(0, 0, 0, home());
    level
        .chunk_saver
        .save_chunks(&level.level_folder, vec![(pos, source.clone())])
        .await
        .unwrap();
    source.set_block_absolute_y(0, 0, 0, Block::BELL.default_state.id);
    source.set_custom_data("test", "end", pumpkin_nbt::tag::NbtTag::End);
    assert!(
        level
            .chunk_saver
            .save_chunks(&level.level_folder, vec![(pos, source.clone())])
            .await
            .is_err()
    );
    assert!(source.is_dirty());
    source
}

async fn assert_cancelled_pending_retry(
    level: &Level,
    source: &SyncChunk,
    terrain_before: &[u8],
    poi_before: &[u8],
) {
    use crate::chunk::{
        format::anvil::AnvilChunkFile,
        io::{FileIO, LoadedData, file_manager::ChunkFileManager},
    };
    let terrain_path = level.level_folder.region_folder.join("r.0.0.mca");
    let poi_path = level.level_folder.poi_folder.join("r.0.0.mca");
    assert!(source.is_dirty());
    assert!(level.chunk_saver.flush().await.is_err());
    level.poi_storage.lock().unwrap().save_all().unwrap();
    assert_eq!(std::fs::read(&terrain_path).unwrap(), terrain_before);
    assert_eq!(std::fs::read(&poi_path).unwrap(), poi_before);
    source.set_custom_data("test", "end", pumpkin_nbt::tag::NbtTag::Int(1));
    // Retry only retained pending work: never re-submit this source with save_chunks.
    level.chunk_saver.flush().await.unwrap();
    let fresh = ChunkFileManager::<AnvilChunkFile<ChunkData>>::new(
        pumpkin_config::chunk::AnvilChunkConfig::default(),
    );
    let (send, mut receive) = tokio::sync::mpsc::channel(1);
    fresh
        .fetch_chunks(&level.level_folder, &[ChunkPos::new(0, 0)], send)
        .await;
    let Some(LoadedData::Loaded(saved)) = receive.recv().await else {
        panic!("retained source did not reach fresh terrain disk");
    };
    assert_eq!(
        saved.section.get_block_absolute_y(0, 0, 0),
        Some(Block::BELL.default_state.id)
    );
    let saved_poi = PoiRegion::load(&poi_path).unwrap();
    let records = saved_poi.get_all();
    assert!(
        records.iter().any(
            |entry| entry.poi_type == PoiType::Meeting.identifier() && entry.free_tickets == 32
        )
    );
    assert!(!source.is_dirty());
}

#[tokio::test]
async fn cancelled_empty_fetch_drains_pending_failure_before_normal_io_and_retry() {
    let (_directory, level, mut schedule, workers) = fixture().await;
    schedule.send_level = Arc::new(LevelChannel::new());
    let source = pending_failed_poi_write(&mut schedule, &level).await;
    let terrain_before = std::fs::read(level.level_folder.region_folder.join("r.0.0.mca")).unwrap();
    let poi_before = std::fs::read(level.level_folder.poi_folder.join("r.0.0.mca")).unwrap();
    let (held, ready) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel();
    let blocked = source.clone();
    let blocker = std::thread::spawn(move || {
        let _light = blocked.light_engine.lock().unwrap();
        held.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(10)).unwrap();
    });
    ready.await.unwrap();
    // Read sibling B: fetch's eviction must attempt pending A's real encoder.
    let queried = ChunkPos::new(1, 0);
    let result = request_area(&schedule, &level, queried);
    let mut active = None;
    schedule.poll_poi_requests(&level, &mut active);
    let reader = start_reader(&schedule, &level, workers.read);
    tokio::time::timeout(Duration::from_secs(5), async {
        while source.is_dirty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    // apply_pending cleared dirty before awaiting the light-blocked encoder.
    drop(result);
    schedule.poll_poi_requests(&level, &mut active);
    assert!(active.is_none());
    assert!(!schedule.io_lock.0.lock().unwrap().contains_key(&queried));
    let following = ChunkPos::new(32, 0);
    schedule
        .io_read
        .send(ReadRequest::Chunks(vec![following]))
        .await
        .unwrap();
    let receive = schedule.recv_chunk.clone();
    assert!(matches!(
        tokio::task::spawn_blocking(move || receive.recv_timeout(Duration::from_millis(50)))
            .await
            .unwrap(),
        Err(crossbeam::channel::RecvTimeoutError::Timeout)
    ));
    release.send(()).unwrap();
    blocker.join().unwrap();
    let (received_pos, received) = ordinary_read_result(&schedule).await;
    assert_eq!(received_pos, following);
    assert!(matches!(received, RecvChunk::IO(Chunk::Proto(_))));
    assert!(schedule.io_lock.0.lock().unwrap().is_empty());
    assert_cancelled_pending_retry(&level, &source, &terrain_before, &poi_before).await;
    drop(schedule);
    tokio::time::timeout(Duration::from_secs(5), reader)
        .await
        .unwrap()
        .unwrap();
}

// R28 actual worker/Level contracts, source-authored and UNRUN. This fixture
// drains Level's automatic workers, then owns the ordinary FIFO write worker.
mod stale_save_deferral {
    use super::*;
    use crate::chunk::{
        ChunkWritingError,
        format::{anvil::AnvilChunkFile, linear::LinearV2File, pump::PumpFile},
        io::{ChunkSerializer, LoadedData, file_manager::ChunkFileManager},
    };
    use pumpkin_config::chunk::ChunkConfig;

    struct SaveWorker {
        _directory: tempfile::TempDir,
        level: Arc<Level>,
        send: tokio::sync::mpsc::Sender<WriteRequest>,
        lock: IOLock,
        task: tokio::task::JoinHandle<()>,
    }

    impl SaveWorker {
        async fn new(chunk: ChunkConfig) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let level = Level::from_root_folder(
                &LevelConfig {
                    chunk,
                    ..LevelConfig::default()
                },
                directory.path().to_owned(),
                0,
                Dimension::OVERWORLD,
            );
            level.shutdown().await.unwrap();
            let (send, receive) = tokio::sync::mpsc::channel(16);
            let lock = Arc::new((
                Mutex::new(HashMapType::default()),
                tokio::sync::Notify::new(),
            ));
            let task = tokio::spawn(io_write_work(receive, level.clone(), lock.clone()));
            Self {
                _directory: directory,
                level,
                send,
                lock,
                task,
            }
        }

        async fn queue(&self, chunks: Vec<(ChunkPos, SyncChunk)>) {
            {
                let mut entries = self.lock.0.lock().unwrap();
                for (pos, _) in &chunks {
                    entries.entry(*pos).or_default().writes += 1;
                }
            }
            self.send
                .send(WriteRequest::Chunks(
                    chunks
                        .into_iter()
                        .map(|(pos, chunk)| (pos, Chunk::Level(chunk)))
                        .collect(),
                ))
                .await
                .unwrap();
        }

        async fn chunks_finished(&self) {
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let notified = self.lock.1.notified();
                    if self.lock.0.lock().unwrap().is_empty() {
                        break;
                    }
                    notified.await;
                }
            })
            .await
            .unwrap();
        }

        async fn request_flush(&self) -> tokio::sync::oneshot::Receiver<Result<(), String>> {
            let (send, receive) = tokio::sync::oneshot::channel();
            self.send
                .send(WriteRequest::Flush(vec![send]))
                .await
                .unwrap();
            receive
        }

        async fn finish(self) {
            drop(self.send);
            tokio::time::timeout(Duration::from_secs(5), self.task)
                .await
                .unwrap()
                .unwrap();
        }
    }

    fn source(level: &Level, x: i32) -> SyncChunk {
        let chunk = ChunkData::empty_sync(x, 0);
        level.bind_chunk_poi(&chunk).unwrap();
        level
            .loaded_chunks
            .insert(ChunkPos::new(x, 0), chunk.clone());
        level.set_block_state(&BlockPos::new(x * 16 + 1, 0, 1), home());
        chunk
    }

    async fn disk<S>(level: &Level, pos: ChunkPos, config: S::ChunkConfig) -> SyncChunk
    where
        S: ChunkSerializer<Data = ChunkData, WriteBackend = std::path::PathBuf>,
        S::ChunkConfig: Send + Sync,
    {
        let reader = ChunkFileManager::<S>::new(config);
        let (send, mut receive) = tokio::sync::mpsc::channel(1);
        reader.fetch_chunks(&level.level_folder, &[pos], send).await;
        let Some(LoadedData::Loaded(saved)) = receive.recv().await else {
            panic!("actual backend did not persist terrain")
        };
        saved
    }

    #[allow(
        clippy::too_many_lines,
        reason = "Keep ordinary Chunks deferral, strict Flush refusal and actual disk completion together."
    )]
    async fn worker_retry<S>(chunk_config: ChunkConfig, config: S::ChunkConfig)
    where
        S: ChunkSerializer<Data = ChunkData, WriteBackend = std::path::PathBuf>,
        S::ChunkConfig: Clone + Send + Sync + 'static,
    {
        for (watched, completion_stale) in [(false, false), (true, false), (false, true)] {
            for stale_flush in [false, true] {
                let worker = SaveWorker::new(chunk_config.clone()).await;
                let level = &worker.level;
                let pos = ChunkPos::new(0, 0);
                let chunk = source(level, 0);
                level
                    .chunk_saver
                    .save_chunks(&level.level_folder, vec![(pos, chunk.clone())])
                    .await
                    .unwrap();
                {
                    let mut storage = level.poi_storage.lock().unwrap();
                    storage
                        .add_with_free_tickets(
                            BlockPos::new(1, 0, 1),
                            PoiType::Home.identifier(),
                            0,
                        )
                        .unwrap();
                    storage.save_all().unwrap();
                };
                let terrain_path = level
                    .level_folder
                    .region_folder
                    .join(S::get_chunk_key(&pos));
                let poi_path = level.level_folder.poi_folder.join("r.0.0.mca");
                let terrain_before = std::fs::read(&terrain_path).unwrap();
                let poi_before = std::fs::read(&poi_path).unwrap();
                if watched {
                    level
                        .chunk_saver
                        .watch_chunks(&level.level_folder, &[pos])
                        .await;
                }
                level.set_block_state(&BlockPos::new(2, 0, 1), Block::STONE.default_state.id);
                let (entered, resume) = if completion_stale {
                    level.poi_storage.lock().unwrap().pause_region_write(pos)
                } else {
                    level.poi_runtime.pause_capture_admission(&chunk)
                };
                worker.queue(vec![(pos, chunk.clone())]).await;
                tokio::time::timeout(Duration::from_secs(5), entered)
                    .await
                    .unwrap()
                    .unwrap();
                // Same published Arc: both ABA history and an ordinary setter
                // invalidate the private capture while no ownership guard is held.
                level.set_block_state(&BlockPos::new(1, 0, 1), BlockStateId::AIR);
                level.set_block_state(&BlockPos::new(1, 0, 1), home());
                level.set_block_state(&BlockPos::new(2, 0, 1), Block::DIRT.default_state.id);
                resume.send(()).unwrap();
                worker.chunks_finished().await;
                assert!(chunk.is_dirty());
                assert!(Arc::ptr_eq(
                    level.loaded_chunks.get(&pos).unwrap().value(),
                    &chunk
                ));
                if completion_stale {
                    let captured = disk::<S>(level, pos, config.clone()).await;
                    assert_eq!(
                        captured.section.get_block_absolute_y(2, 0, 1),
                        Some(Block::STONE.default_state.id)
                    );
                    assert_eq!(
                        PoiRegion::load(&poi_path).unwrap().get_all()[0].free_tickets,
                        0
                    );
                } else {
                    assert_eq!(std::fs::read(&terrain_path).unwrap(), terrain_before);
                    assert_eq!(std::fs::read(&poi_path).unwrap(), poi_before);
                }
                let terrain_deferred = std::fs::read(&terrain_path).unwrap();
                let poi_deferred = std::fs::read(&poi_path).unwrap();
                let expected_marker = if stale_flush {
                    let (entered, resume) = if completion_stale {
                        level.poi_storage.lock().unwrap().pause_region_write(pos)
                    } else {
                        level.poi_runtime.pause_capture_admission(&chunk)
                    };
                    let mut finished = worker.request_flush().await;
                    tokio::time::timeout(Duration::from_secs(5), entered)
                        .await
                        .unwrap()
                        .unwrap();
                    assert!(matches!(
                        finished.try_recv(),
                        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
                    ));
                    level.set_block_state(
                        &BlockPos::new(2, 0, 1),
                        Block::GOLD_BLOCK.default_state.id,
                    );
                    resume.send(()).unwrap();
                    let failure = tokio::time::timeout(Duration::from_secs(5), finished)
                        .await
                        .unwrap()
                        .unwrap()
                        .unwrap_err();
                    assert_eq!(failure, ChunkWritingError::StaleSaveCapture.to_string());
                    assert!(chunk.is_dirty());
                    if completion_stale {
                        let captured = disk::<S>(level, pos, config.clone()).await;
                        assert_eq!(
                            captured.section.get_block_absolute_y(2, 0, 1),
                            Some(Block::DIRT.default_state.id)
                        );
                        assert_eq!(
                            PoiRegion::load(&poi_path).unwrap().get_all()[0].free_tickets,
                            1
                        );
                    } else {
                        assert_eq!(std::fs::read(&terrain_path).unwrap(), terrain_deferred);
                        assert_eq!(std::fs::read(&poi_path).unwrap(), poi_deferred);
                    }
                    Block::GOLD_BLOCK.default_state.id
                } else {
                    Block::DIRT.default_state.id
                };
                // This actual FIFO Flush retries retained work without another
                // Chunks request. It must not return the old background refusal.
                tokio::time::timeout(Duration::from_secs(5), worker.request_flush().await)
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
                let saved = disk::<S>(level, pos, config.clone()).await;
                assert!(!Arc::ptr_eq(&saved, &chunk));
                assert_eq!(saved.section.get_block_absolute_y(1, 0, 1), Some(home()));
                assert_eq!(
                    saved.section.get_block_absolute_y(2, 0, 1),
                    Some(expected_marker)
                );
                let poi = PoiRegion::load(&poi_path).unwrap();
                assert_eq!(poi.get_all()[0].poi_type, PoiType::Home.identifier());
                assert_eq!(poi.get_all()[0].free_tickets, 1);
                assert!(!chunk.is_dirty());
                worker.finish().await;
            }
        }
    }

    #[tokio::test]
    async fn all_formats_actual_chunks_defer_stale_and_actual_flush_stays_strict() {
        let anvil = pumpkin_config::chunk::AnvilChunkConfig::default();
        Box::pin(worker_retry::<AnvilChunkFile<ChunkData>>(
            ChunkConfig::Anvil(anvil.clone()),
            anvil,
        ))
        .await;
        Box::pin(worker_retry::<LinearV2File<ChunkData>>(
            ChunkConfig::Linear,
            (),
        ))
        .await;
        Box::pin(worker_retry::<PumpFile<ChunkData>>(ChunkConfig::Pump, ())).await;
    }

    #[allow(
        clippy::too_many_lines,
        reason = "Observe real mixed hard failures through the worker's historical Level latch."
    )]
    async fn worker_retains_hard_sibling<S>(chunk_config: ChunkConfig, config: S::ChunkConfig)
    where
        S: ChunkSerializer<Data = ChunkData, WriteBackend = std::path::PathBuf>,
        S::ChunkConfig: Clone + Send + Sync + 'static,
    {
        for across_regions in [false, true] {
            let worker = SaveWorker::new(chunk_config.clone()).await;
            let level = &worker.level;
            let bx = if across_regions { 32 } else { 1 };
            let cx = if across_regions { 64 } else { 2 };
            let positions = [
                ChunkPos::new(0, 0),
                ChunkPos::new(bx, 0),
                ChunkPos::new(cx, 0),
            ];
            let chunks = [source(level, 0), source(level, bx), source(level, cx)];
            let submitted = || {
                positions
                    .iter()
                    .copied()
                    .zip(chunks.iter().cloned())
                    .collect()
            };
            level
                .chunk_saver
                .save_chunks(&level.level_folder, submitted())
                .await
                .unwrap();
            {
                let mut storage = level.poi_storage.lock().unwrap();
                for x in [0, bx] {
                    storage
                        .add_with_free_tickets(
                            BlockPos::new(x * 16 + 1, 0, 1),
                            PoiType::Home.identifier(),
                            0,
                        )
                        .unwrap();
                }
                storage.save_all().unwrap();
            };
            chunks[0].mark_dirty(true);
            level.set_block_state(
                &BlockPos::new(bx * 16 + 1, 0, 1),
                Block::BELL.default_state.id,
            );
            level.set_block_state(
                &BlockPos::new(cx * 16 + 1, 0, 1),
                Block::BELL.default_state.id,
            );
            chunks[1].set_custom_data("r28", "refused", pumpkin_nbt::tag::NbtTag::End);
            let expected_hard =
                ChunkWritingError::from(chunks[1].to_bytes_for_save().unwrap_err()).to_string();
            let (entered, resume) = level.poi_runtime.pause_capture_admission(&chunks[0]);
            worker.queue(submitted()).await;
            tokio::time::timeout(Duration::from_secs(5), entered)
                .await
                .unwrap()
                .unwrap();
            level.set_block_state(&BlockPos::new(2, 0, 1), Block::STONE.default_state.id);
            resume.send(()).unwrap();
            worker.chunks_finished().await;
            assert!(chunks[0].is_dirty());
            assert!(chunks[1].is_dirty());
            assert_eq!(
                disk::<S>(level, positions[1], config.clone())
                    .await
                    .section
                    .get_block_absolute_y(1, 0, 1),
                Some(home())
            );
            assert_eq!(
                disk::<S>(level, positions[2], config.clone())
                    .await
                    .section
                    .get_block_absolute_y(1, 0, 1),
                Some(Block::BELL.default_state.id)
            );
            for x in [0, bx] {
                let poi = PoiRegion::load(
                    &level
                        .level_folder
                        .poi_folder
                        .join(format!("r.{}.0.mca", x >> 5)),
                )
                .unwrap();
                let entry = poi
                    .get_all()
                    .into_iter()
                    .find(|entry| entry.pos() == BlockPos::new(x * 16 + 1, 0, 1))
                    .unwrap();
                assert_eq!(entry.poi_type, PoiType::Home.identifier());
                assert_eq!(entry.free_tickets, 0);
            }
            chunks[1].set_custom_data("r28", "refused", pumpkin_nbt::tag::NbtTag::Int(1));
            // The stable attempt physically succeeds, yet the real earlier hard
            // failure must still be returned once by Level's unchanged latch.
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(5), worker.request_flush().await)
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap_err(),
                expected_hard
            );
            assert!(!chunks[0].is_dirty());
            assert!(!chunks[1].is_dirty());
            assert_eq!(
                disk::<S>(level, positions[1], config.clone())
                    .await
                    .section
                    .get_block_absolute_y(1, 0, 1),
                Some(Block::BELL.default_state.id)
            );
            tokio::time::timeout(Duration::from_secs(5), worker.request_flush().await)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            worker.finish().await;
        }
    }

    #[tokio::test]
    async fn all_formats_actual_worker_keeps_hard_sibling_failure_after_stable_retry() {
        let anvil = pumpkin_config::chunk::AnvilChunkConfig::default();
        Box::pin(worker_retains_hard_sibling::<AnvilChunkFile<ChunkData>>(
            ChunkConfig::Anvil(anvil.clone()),
            anvil,
        ))
        .await;
        Box::pin(worker_retains_hard_sibling::<LinearV2File<ChunkData>>(
            ChunkConfig::Linear,
            (),
        ))
        .await;
        Box::pin(worker_retains_hard_sibling::<PumpFile<ChunkData>>(
            ChunkConfig::Pump,
            (),
        ))
        .await;
    }
}

// Tick-phase retirement through the real scheduler and backend fixture.
#[tokio::test]
async fn active_block_phase_defers_already_prepared_retirement_then_retries_without_poi_changes() {
    use crate::tick::TickPriority;
    use rustc_hash::FxHashSet;
    let (_directory, level, mut schedule, mut workers) = fixture().await;
    let pos = ChunkPos::new(0, 0);
    let chunk = publish(&mut schedule, &level, pos);
    chunk.post_processing_ready.store(true, Relaxed);
    level.set_tick_game_time(100);
    level.schedule_block_tick(
        &Block::STONE,
        BlockPos::new(1, 64, 1),
        0,
        TickPriority::Normal,
    );
    schedule.unload_chunks.insert(pos);
    schedule = unload(schedule, level.clone()).await;
    // The real standalone unload pass has already prepared this exact Arc.
    assert!(level.chunk_unload.access().requests().is_empty());
    assert!(level.loaded_chunks.contains_key(&pos));
    assert!(workers.write.try_recv().is_err());
    let before = level.prepare_chunk_poi(&chunk).unwrap();
    level.collect_block_ticks(100, 65536, &FxHashSet::from_iter([pos]));
    assert_eq!(
        chunk
            .block_tick_phase_owners
            .load(std::sync::atomic::Ordering::Acquire),
        1
    );
    assert!(matches!(
        chunk.capture_for_save(),
        Err(ChunkSerializingError::Poi(
            PoiReconcileError::StaleSaveCapture
        ))
    ));
    schedule = unload(schedule, level.clone()).await;
    assert!(
        level
            .loaded_chunks
            .get(&pos)
            .is_some_and(|current| Arc::ptr_eq(current.value(), &chunk))
    );
    assert!(workers.write.try_recv().is_err());
    assert!(level.chunk_unload.access().is_requested(pos, &chunk));
    assert_eq!(
        level.pop_block_tick().unwrap().position,
        BlockPos::new(1, 64, 1)
    );
    assert!(level.pop_block_tick().is_none());
    level.finish_block_ticks();
    assert_eq!(
        chunk
            .block_tick_phase_owners
            .load(std::sync::atomic::Ordering::Acquire),
        0
    );
    assert_eq!(level.prepare_chunk_poi(&chunk).unwrap(), before);
    assert!(chunk.capture_for_save().is_ok());
    schedule = unload(schedule, level.clone()).await; // Requested -> Prepared
    assert!(level.loaded_chunks.contains_key(&pos));
    schedule = unload(schedule, level.clone()).await; // actual exclusive retirement
    assert!(!level.loaded_chunks.contains_key(&pos));
    let WriteRequest::Chunks(saved) = workers.write.try_recv().unwrap() else {
        panic!("expected terrain save")
    };
    assert_eq!(saved.len(), 1);
    let Chunk::Level(source) = &saved[0].1 else {
        panic!("expected Full terrain")
    };
    assert!(Arc::ptr_eq(source, &chunk));
    // Consume the actual queued source through the existing backend, as the
    // original fixture does. No phase records are a separate disk owner.
    level
        .chunk_saver
        .save_chunks(&level.level_folder, vec![(pos, source.clone())])
        .await
        .unwrap();
    level.chunk_saver.flush().await.unwrap();
    assert!(chunk.read_for_poi().is_capture_current(&before));
    assert!(schedule.chunk_map.is_empty());
}

async fn fresh_retired_tick_delays(level: &Level, pos: ChunkPos) -> (Vec<i32>, Vec<i32>) {
    use crate::chunk::{
        format::anvil::AnvilChunkFile,
        io::{LoadedData, file_manager::ChunkFileManager},
    };
    use pumpkin_config::chunk::AnvilChunkConfig;
    let reader = ChunkFileManager::<AnvilChunkFile<ChunkData>>::new(AnvilChunkConfig::default());
    let (send, mut receive) = tokio::sync::mpsc::channel(1);
    reader.fetch_chunks(&level.level_folder, &[pos], send).await;
    let Some(LoadedData::Loaded(saved)) = receive.recv().await else {
        panic!("missing retired chunk")
    };
    (
        saved
            .block_ticks
            .pack_at(0)
            .iter()
            .map(|tick| tick.delay)
            .collect(),
        saved
            .fluid_ticks
            .pack_at(0)
            .iter()
            .map(|tick| tick.delay)
            .collect(),
    )
}

fn schedule_retirement_ticks(level: &Level) {
    use crate::tick::TickPriority;
    use pumpkin_data::fluid::Fluid;
    level.schedule_block_tick(
        &Block::STONE,
        BlockPos::new(1, 64, 1),
        200,
        TickPriority::High,
    );
    level.schedule_fluid_tick(
        &Fluid::WATER,
        BlockPos::new(2, 64, 1),
        250,
        TickPriority::Low,
    );
}

#[tokio::test]
async fn prepared_retirement_fixes_both_delay_lists_before_deferred_backend_write() {
    let (_directory, level, mut schedule, mut workers) = fixture().await;
    let pos = ChunkPos::new(0, 0);
    let chunk = publish(&mut schedule, &level, pos);
    level.set_tick_game_time(100);
    schedule_retirement_ticks(&level);
    schedule.unload_chunks.insert(pos);
    level.set_tick_game_time(140);
    schedule = unload(schedule, level.clone()).await;
    // Requested/prepared work still has absolute queues. Initial ticket loss
    // must not fix the delay before actual terminal retirement.
    assert!(level.loaded_chunks.contains_key(&pos));
    assert_eq!(chunk.block_ticks.get_all()[0].trigger_tick, 300);
    assert_eq!(chunk.fluid_ticks.get_all()[0].trigger_tick, 350);
    level.set_tick_game_time(150);
    schedule = unload(schedule, level.clone()).await;
    assert!(!level.loaded_chunks.contains_key(&pos));
    assert!(chunk.block_ticks.get_all().is_empty());
    assert!(chunk.fluid_ticks.get_all().is_empty());
    assert!(
        chunk
            .block_ticks
            .is_scheduled(BlockPos::new(1, 64, 1), &Block::STONE)
    );
    assert!(
        chunk
            .fluid_ticks
            .is_scheduled(BlockPos::new(2, 64, 1), &pumpkin_data::fluid::Fluid::WATER)
    );
    let WriteRequest::Chunks(saved) = workers.write.try_recv().unwrap() else {
        panic!("expected terrain save")
    };
    assert_eq!(saved.len(), 1);
    let Chunk::Level(source) = &saved[0].1 else {
        panic!("expected Full terrain")
    };
    assert!(Arc::ptr_eq(source, &chunk));
    level.set_tick_game_time(1_000_150);
    level
        .chunk_saver
        .save_chunks(&level.level_folder, vec![(pos, source.clone())])
        .await
        .unwrap();
    level.chunk_saver.flush().await.unwrap();
    assert_eq!(
        fresh_retired_tick_delays(&level, pos).await,
        (vec![150], vec![200])
    );
    assert!(schedule.chunk_map.is_empty());
}

#[tokio::test]
async fn clean_retirement_preserves_earlier_capture_without_resubmission() {
    use crate::chunk::io::LoadedData;
    let (_directory, level, mut schedule, mut workers) = fixture().await;
    let pos = ChunkPos::new(0, 0);
    let chunk = publish(&mut schedule, &level, pos);
    level.set_tick_game_time(100);
    schedule_retirement_ticks(&level);
    schedule.unload_chunks.insert(pos);
    schedule = unload(schedule, level.clone()).await;
    assert!(level.chunk_unload.access().requests().is_empty());
    assert!(level.loaded_chunks.contains_key(&pos));
    level
        .chunk_saver
        .watch_chunks(&level.level_folder, &[pos])
        .await;
    // Existing admission gate is after capture/compression and its dirty clear,
    // with no lifecycle read guard left to exclude terminal retirement.
    let (entered, resume) = level.poi_runtime.pause_capture_admission(&chunk);
    let mut saving = Box::pin(
        level
            .chunk_saver
            .save_chunks(&level.level_folder, vec![(pos, chunk.clone())]),
    );
    tokio::select! {
        result = &mut saving => panic!("older capture bypassed admission gate: {result:?}"),
        result = tokio::time::timeout(std::time::Duration::from_secs(5), entered) => result.unwrap().unwrap(),
    }
    assert!(!chunk.is_dirty());
    level.set_tick_game_time(120);
    let pure = ChunkData::from_bytes(&chunk.to_bytes().unwrap(), pos).unwrap();
    assert_eq!(pure.block_ticks.pack_at(0)[0].delay, 180);
    assert_eq!(pure.fluid_ticks.pack_at(0)[0].delay, 230);
    assert!(!chunk.is_dirty());
    level.set_tick_game_time(140);
    schedule = unload(schedule, level.clone()).await;
    assert!(!level.loaded_chunks.contains_key(&pos));
    // Representation packing must preserve the already accepted clean save.
    assert!(!chunk.is_dirty());
    assert!(workers.write.try_recv().is_err());
    level.set_tick_game_time(1_000_140);
    resume.send(()).unwrap();
    saving.await.unwrap();
    let (send, mut receive) = tokio::sync::mpsc::channel(1);
    level
        .chunk_saver
        .fetch_chunks(&level.level_folder, &[pos], send)
        .await;
    let Some(LoadedData::Loaded(reused)) = receive.recv().await else {
        panic!("missing retained retired chunk")
    };
    assert!(Arc::ptr_eq(&reused, &chunk));
    assert_eq!(
        reused
            .block_ticks
            .pack_at(0)
            .iter()
            .map(|tick| tick.delay)
            .collect::<Vec<_>>(),
        vec![200]
    );
    assert_eq!(
        reused
            .fluid_ticks
            .pack_at(0)
            .iter()
            .map(|tick| tick.delay)
            .collect::<Vec<_>>(),
        vec![250]
    );
    level.chunk_saver.flush().await.unwrap();
    assert!(!chunk.is_dirty());
    assert_eq!(
        fresh_retired_tick_delays(&level, pos).await,
        (vec![200], vec![250])
    );
    assert!(schedule.chunk_map.is_empty());
}

#[tokio::test]
async fn logical_tick_mutation_during_capture_recaptures_retired_delays() {
    use crate::tick::TickPriority;
    let (_directory, level, mut schedule, mut workers) = fixture().await;
    let pos = ChunkPos::new(0, 0);
    let chunk = publish(&mut schedule, &level, pos);
    level.set_tick_game_time(100);
    schedule_retirement_ticks(&level);
    schedule.unload_chunks.insert(pos);
    schedule = unload(schedule, level.clone()).await;
    assert!(level.chunk_unload.access().requests().is_empty());
    assert!(level.loaded_chunks.contains_key(&pos));
    let stamp = level.prepare_chunk_poi(&chunk).unwrap();
    level
        .chunk_saver
        .watch_chunks(&level.level_folder, &[pos])
        .await;
    let (entered, resume) = level.poi_runtime.pause_capture_admission(&chunk);
    let mut saving = Box::pin(
        level
            .chunk_saver
            .save_chunks(&level.level_folder, vec![(pos, chunk.clone())]),
    );
    tokio::select! {
        result = &mut saving => panic!("older capture bypassed admission gate: {result:?}"),
        result = tokio::time::timeout(std::time::Duration::from_secs(5), entered) => result.unwrap().unwrap(),
    }
    assert!(!chunk.is_dirty());
    level.set_tick_game_time(140);
    level.schedule_block_tick(
        &Block::STONE,
        BlockPos::new(3, 64, 1),
        40,
        TickPriority::Normal,
    );
    assert!(chunk.is_dirty());
    assert_eq!(chunk.block_ticks.get_all().len(), 2);
    assert_eq!(level.prepare_chunk_poi(&chunk).unwrap(), stamp);
    schedule = unload(schedule, level.clone()).await;
    assert!(!level.loaded_chunks.contains_key(&pos));
    assert!(chunk.is_dirty());
    let WriteRequest::Chunks(saved) = workers.write.try_recv().unwrap() else {
        panic!("expected frozen terrain source")
    };
    assert_eq!(saved.len(), 1);
    let Chunk::Level(source) = &saved[0].1 else {
        panic!("expected Full terrain")
    };
    assert!(Arc::ptr_eq(source, &chunk));
    level.set_tick_game_time(1_000_140);
    resume.send(()).unwrap();
    saving.await.unwrap();
    // Do not submit the queued retirement source: the completed older receipt
    // must retain the logically dirty source for the existing recapture path.
    level.chunk_saver.flush().await.unwrap();
    assert!(chunk.is_dirty());
    level.chunk_saver.flush().await.unwrap();
    assert!(!chunk.is_dirty());
    assert_eq!(
        fresh_retired_tick_delays(&level, pos).await,
        (vec![160, 40], vec![210])
    );
    assert!(chunk.read_for_poi().is_capture_current(&stamp));
    assert!(schedule.chunk_map.is_empty());
}
