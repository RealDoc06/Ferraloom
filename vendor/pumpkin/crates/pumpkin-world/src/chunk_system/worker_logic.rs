use super::chunk_state::{Chunk, StagedChunkEnum};
use super::generation_cache::Cache;
use super::{ChunkPos, IOLock};
use crate::ProtoChunk;
use crate::chunk::format::LightContainer;
use crate::chunk::io::LoadedData::Loaded;
use crate::chunk::io::{FileIO, LoadedData, run_blocking};
use crate::level::Level;
use crate::poi::{PoiQueryError, query::PoiTerrainOverlay};
use pumpkin_config::lighting::LightingEngineConfig;
use pumpkin_data::chunk::ChunkStatus;
use std::collections::hash_map::Entry;
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;
use tracing::{debug, error, warn};

pub enum RecvChunk {
    IO(Chunk),
    ReadFailure(String),
    Generation(Cache),
    GenerationFailure {
        pos: ChunkPos,
        stage: StagedChunkEnum,
        error: String,
    },
}

pub enum WriteRequest {
    Chunks(Vec<(ChunkPos, Chunk)>),
    Entities,
    Flush(Vec<tokio::sync::oneshot::Sender<Result<(), String>>>),
}

pub(crate) enum ReadRequest {
    Chunks(Vec<ChunkPos>),
    Poi(PoiIoReadRequest),
}

pub(crate) struct PoiIoReadRequest {
    pub(crate) positions: Vec<ChunkPos>,
    pub(crate) reservation: Arc<()>,
    pub(crate) reply: tokio::sync::oneshot::Sender<Result<Vec<PoiTerrainOverlay>, PoiQueryError>>,
}

async fn read_poi_area(mut request: PoiIoReadRequest, level: &Level, lock: &IOLock) {
    let result = read_reserved_poi(&mut request, level, lock).await;
    let _ = request.reply.send(result);
    level.level_channel.notify();
}

async fn read_reserved_poi(
    request: &mut PoiIoReadRequest,
    level: &Level,
    lock: &IOLock,
) -> Result<Vec<PoiTerrainOverlay>, PoiQueryError> {
    for pos in &request.positions {
        loop {
            let notified = lock.1.notified();
            let ready = {
                let entries = lock
                    .0
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let Some(entry) = entries.get(pos) else {
                    return Err(PoiQueryError::Closed);
                };
                if !entry
                    .poi
                    .as_ref()
                    .is_some_and(|id| Arc::ptr_eq(id, &request.reservation))
                {
                    return Err(PoiQueryError::Closed);
                }
                entry.writes == 0
            };
            if ready {
                break;
            }
            tokio::select! {
                () = notified => {},
                () = request.reply.closed() => return Err(PoiQueryError::Closed),
            }
        }
    }
    // This request owns only a tag. It never waits on its own read reservation,
    // and has no storage, publication or terrain guard across disk IO.
    if request.reply.is_closed() {
        return Err(PoiQueryError::Closed);
    }
    let (send, mut receive) = tokio::sync::mpsc::channel(request.positions.len().max(1));
    // Fetch can flush pending writes during eviction. Once started, let it
    // finish restoring failed sources or publishing successful serialization;
    // dropping that future could leave a pending source incorrectly clean.
    level
        .chunk_saver
        .fetch_chunks(&level.level_folder, &request.positions, send)
        .await;
    if request.reply.is_closed() {
        return Err(PoiQueryError::Closed);
    }
    let mut overlays = Vec::with_capacity(request.positions.len());
    while let Some(data) = receive.recv().await {
        match data {
            Loaded(chunk) => overlays.push(PoiTerrainOverlay::classify_decoded(&chunk)?),
            LoadedData::Missing(pos) => overlays.push(PoiTerrainOverlay {
                pos,
                sections: Vec::new(),
            }),
            LoadedData::Error((pos, error)) => {
                let reason = error.to_string();
                level.record_fatal_save_failure(format!(
                    "Could not read saved chunk {pos:?}: {reason}"
                ));
                return Err(PoiQueryError::NeedsTerrain { pos, reason });
            }
        }
    }
    if overlays.len() != request.positions.len() {
        return Err(PoiQueryError::Closed);
    }
    Ok(overlays)
}

/// Checks if a chunk needs relighting based on the current lighting configuration
/// Returns true if the chunk has uniform lighting (from full/dark mode) but the server
/// is now running in default mode (which needs proper lighting calculation)
fn needs_relighting(chunk: &crate::chunk::ChunkData, config: LightingEngineConfig) -> bool {
    if config != LightingEngineConfig::Default {
        return false;
    }

    // If the chunk says it's already lit, believe it.
    if chunk.light_populated.load(Relaxed) {
        return false;
    }

    let engine = chunk
        .light_engine
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    // Scan for any complex lighting data
    let has_complex_light = engine.sky_light.iter().any(|lc| match lc {
        LightContainer::Full(data) => data.iter().any(|&b| b != 0x00 && b != 0xFF),
        LightContainer::Empty(val) => *val != 0 && *val != 15,
    }) || engine.block_light.iter().any(|lc| match lc {
        LightContainer::Full(data) => data.iter().any(|&b| b != 0x00 && b != 0xFF),
        LightContainer::Empty(val) => *val != 0 && *val != 15,
    });

    // If it has complex light, we don't need to relight.
    !has_complex_light
}

fn load_proto_chunk(chunk: &crate::chunk::ChunkData, level: &Level) -> ProtoChunk {
    ProtoChunk::from_chunk_data(chunk, &level.world_gen.load())
}

fn process_loaded_chunk(chunk: Arc<crate::chunk::ChunkData>, level: &Level) -> Chunk {
    let pos = ChunkPos::new(chunk.x, chunk.z);
    if chunk.status == ChunkStatus::Full {
        let needs_relight = needs_relighting(&chunk, level.lighting_config);
        if needs_relight {
            debug!(
                "Chunk {pos:?} has uniform lighting, downgrading to Features stage for relighting"
            );

            let mut proto = load_proto_chunk(&chunk, level);

            // Clear all lighting data
            let section_count = proto.light.sky_light.len();
            proto.light.sky_light = (0..section_count)
                .map(|_| LightContainer::new_empty(15))
                .collect();
            proto.light.block_light = (0..section_count)
                .map(|_| LightContainer::new_empty(0))
                .collect();
            proto.stage = StagedChunkEnum::Features;
            Chunk::Proto(Box::new(proto))
        } else {
            Chunk::Level(chunk)
        }
    } else {
        let proto = load_proto_chunk(&chunk, level);
        Chunk::Proto(Box::new(proto))
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep read dispatch, result delivery and the owned fetch drain in one worker loop."
)]
pub(crate) async fn io_read_work(
    recv: Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<ReadRequest>>>,
    send: crossbeam::channel::Sender<(ChunkPos, RecvChunk)>,
    level: Arc<Level>,
    lock: IOLock,
) {
    debug!("io read thread start");

    // Cleaner loop and async recv
    loop {
        let batch = {
            let mut lock_rx = recv.lock().await;
            lock_rx.recv().await
        };
        let Some(request) = batch else {
            break;
        };
        let batch = match request {
            ReadRequest::Chunks(batch) => batch,
            ReadRequest::Poi(request) => {
                read_poi_area(request, &level, &lock).await;
                continue;
            }
        };
        for pos in &batch {
            // Lock handling
            loop {
                let notified = lock.1.notified();
                if !lock
                    .0
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains_key(pos)
                {
                    break;
                }
                notified.await;
            }
        }

        let (t_send, mut t_recv) = tokio::sync::mpsc::channel(1000);

        let batch_len = batch.len();
        let level_clone = level.clone();

        let fetch_task = tokio::spawn(async move {
            level_clone
                .chunk_saver
                .fetch_chunks(&level_clone.level_folder, &batch, t_send)
                .await;
        });

        for _ in 0..batch_len {
            let Some(data) = t_recv.recv().await else {
                break;
            };

            match data {
                Loaded(chunk) => {
                    let pos = ChunkPos::new(chunk.x, chunk.z);
                    let chunk_level = level.clone();
                    let result = run_blocking(move || {
                        chunk_level.bind_chunk_poi(&chunk)?;
                        Ok::<_, crate::poi::PoiReconcileError>(process_loaded_chunk(
                            chunk,
                            &chunk_level,
                        ))
                    })
                    .await;
                    let received = match result {
                        Ok(Ok(processed)) => RecvChunk::IO(processed),
                        Ok(Err(error)) => {
                            let message = format!("Could not bind saved chunk {pos:?}: {error}");
                            level.record_fatal_save_failure(message.clone());
                            RecvChunk::ReadFailure(message)
                        }
                        Err(err) => {
                            let message = format!("Could not activate saved chunk {pos:?}: {err}");
                            level.record_fatal_save_failure(message.clone());
                            RecvChunk::ReadFailure(message)
                        }
                    };
                    if send.send((pos, received)).is_err() {
                        break;
                    }
                }
                LoadedData::Missing(pos) => {
                    if send
                        .send((
                            pos,
                            RecvChunk::IO(Chunk::Proto(Box::new(ProtoChunk::new(
                                pos.x,
                                pos.y,
                                &level.world_gen.load(),
                            )))),
                        ))
                        .is_err()
                    {
                        break;
                    }
                }
                LoadedData::Error((pos, error)) => {
                    let message = format!("Could not read saved chunk {pos:?}: {error}");
                    level.record_fatal_save_failure(message.clone());
                    if send.send((pos, RecvChunk::ReadFailure(message))).is_err() {
                        break;
                    }
                }
            }
        }
        let _ = fetch_task.await;
    }
    debug!("io read thread stop");
}

pub async fn io_write_work(
    mut recv: tokio::sync::mpsc::Receiver<WriteRequest>,
    level: Arc<Level>,
    lock: IOLock,
) {
    loop {
        // Don't check cancel_token here (keep saving chunks)
        let Some(request) = recv.recv().await else {
            break;
        };
        let data = match request {
            WriteRequest::Chunks(data) => data,
            WriteRequest::Entities => {
                if let Err(error) = level.save_pending_entity_chunks().await {
                    error!("Failed to save entity chunks: {error}");
                    level.record_save_failure(error);
                }
                continue;
            }
            WriteRequest::Flush(waiters) => {
                // This barrier follows all chunks already enqueued by the
                // scheduler. Cached watched regions also need a physical write.
                let flushed = level.flush_chunk_files().await;
                for waiter in waiters {
                    let _ = waiter.send(flushed.clone());
                }
                continue;
            }
        };
        // debug!("io write thread receive chunks size {}", data.len());
        let positions = data.iter().map(|(pos, _)| *pos).collect::<Vec<_>>();
        let level_for_upgrade = level.clone();
        let upgrade_result = run_blocking(move || {
            let mut vec = Vec::with_capacity(data.len());
            for (pos, chunk) in data {
                match chunk {
                    Chunk::Level(chunk) => {
                        level_for_upgrade.bind_chunk_poi(&chunk)?;
                        vec.push((pos, chunk));
                    }
                    Chunk::Proto(chunk) => {
                        let mut temp = Chunk::Proto(chunk);
                        temp.upgrade_to_level_chunk(
                            level_for_upgrade.world_gen.load().dimension(),
                            &level_for_upgrade.lighting_config,
                        );
                        let Chunk::Level(chunk) = temp else { panic!() };
                        // status retains the Proto stage; actual save writes POI
                        // Valid0 without acknowledging a FULL terrain revision.
                        level_for_upgrade.bind_chunk_poi(&chunk)?;
                        vec.push((pos, chunk));
                    }
                }
            }
            Ok::<_, crate::poi::PoiReconcileError>(vec)
        })
        .await;
        let upgrade_failed = match upgrade_result {
            Ok(Ok(vec)) => {
                if let Err(e) = level
                    .chunk_saver
                    .save_chunks(&level.level_folder, vec)
                    .await
                {
                    if e.is_stale_save_capture() {
                        debug!("Deferred changed chunk save capture: {e}");
                    } else {
                        error!("Failed to save chunks: {:?}", e);
                        level.record_save_failure(e.to_string());
                    }
                }
                false
            }
            Ok(Err(error)) => {
                level.record_fatal_save_failure(error.to_string());
                true
            }
            Err(_) => true,
        };

        {
            let mut data = lock
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for i in positions {
                match data.entry(i) {
                    Entry::Occupied(mut entry) => {
                        let rc = entry.get_mut();
                        rc.writes = rc.writes.saturating_sub(1);
                        if rc.writes == 0 && rc.poi.is_none() {
                            entry.remove();
                        }
                    }
                    Entry::Vacant(_) => {
                        warn!(
                            "io_write: attempted to release missing lock entry for {:?}",
                            i
                        );
                    }
                }
            }
        }
        lock.1.notify_waiters();

        if upgrade_failed {
            error!("Failed to upgrade chunks for saving");
            level.record_fatal_save_failure("Failed to upgrade chunks for saving".to_string());
        }
    }
}

pub fn run_generation(
    pos: ChunkPos,
    mut cache: Cache,
    stage: StagedChunkEnum,
    level: &Level,
) -> RecvChunk {
    let portal = level.world_portal.load_full();
    let Some(portal_ref) = portal.as_deref() else {
        error!("Chunk generation FAILED at {pos:?} ({stage:?}): World portal is not initialized");
        return RecvChunk::GenerationFailure {
            pos,
            stage,
            error: "World portal is not initialized".to_string(),
        };
    };
    // Run generation with panic catching
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cache.advance(
            stage,
            &level.world_gen.load(),
            portal_ref,
            &level.lighting_config,
        );
        cache // Return cache on success
    }));

    match result {
        Ok(cache) => {
            for chunk in &cache.chunks {
                if let Chunk::Level(chunk) = chunk
                    && let Err(error) = level.bind_chunk_poi(chunk)
                {
                    return RecvChunk::GenerationFailure {
                        pos,
                        stage,
                        error: error.to_string(),
                    };
                }
            }
            RecvChunk::Generation(cache)
        }
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| {
                    payload
                        .downcast_ref::<String>()
                        .map(std::string::String::as_str)
                })
                .unwrap_or("Unknown panic payload");

            error!("Chunk generation FAILED at {pos:?} ({stage:?}): {msg}");

            RecvChunk::GenerationFailure {
                pos,
                stage,
                error: msg.to_string(),
            }
        }
    }
}
