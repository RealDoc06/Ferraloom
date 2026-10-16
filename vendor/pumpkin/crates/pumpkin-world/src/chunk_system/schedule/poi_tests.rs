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
) -> tokio::sync::oneshot::Receiver<Result<AreaCapture, PoiQueryError>> {
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
        query.disk.as_ref().unwrap()[0]
            .sections
            .iter()
            .any(|(y, palette)| *y == 0 && palette.get(0, 0, 0) == home())
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
    let AreaCapture::Ready(entries) = result.await.unwrap().unwrap() else {
        panic!("cold capture")
    };
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
    cache.set_block_state(&Vector3::new(1, 0, 1), &Block::BELL.default_state);
    let holder = schedule.chunk_map.get_mut(&pos).unwrap();
    holder.chunk = cache.chunks.pop();
    holder.occupied = NodeKey::null();
    schedule.graph.nodes.remove(occupy);
    schedule.poll_poi_requests(&level, &mut active);
    let AreaCapture::Ready(entries) = receive.await.unwrap().unwrap() else {
        panic!("private Proto capture")
    };
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
    generated.set_block_state(1, 0, 1, &Block::BELL.default_state);
    assert!(PoiTerrainOverlay::resident(&generated).sections.is_empty());
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
    let fresh = ChunkFileManager::<AnvilChunkFile<ChunkData>>::new(Default::default());
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
