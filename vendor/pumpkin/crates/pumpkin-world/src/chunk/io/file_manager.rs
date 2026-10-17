use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use futures::future::join_all;
use pumpkin_util::math::vector2::Vector2;
use tokio::{
    join,
    sync::{Mutex, OnceCell, RwLock, mpsc},
};
use tracing::{debug, error, trace};

use crate::{
    chunk::{ChunkReadingError, ChunkWritingError, io::Dirtiable},
    level::LevelFolder,
};

use super::{ChunkSerializer, EncodedPoiCommit, FileIO, LoadedData, run_blocking};
use crate::chunk::poi_binding::complete_poi_batch;

/// A simple implementation of the `ChunkSerializer` trait that loads and saves data
/// to disk using parallelism and a lazy-loading cache keyed by file path.
///
/// ### Concurrency model
///
/// * `file_locks` caches one lazy region per path. Each loader retains source
///   chunks before initialization, and serializes updates through its pending
///   mutex followed by the `RwLock<CachedRegion<S>>`. Failed sources survive
///   separately from successfully serialized changes awaiting a physical write.
/// * `watchers` counts active chunks per region path. Watched regions keep
///   ordinary saves cached; explicit flushes still write them. The final
///   unwatch writes pending changes before attempting eviction.
///
/// ### Lock ordering (must never be violated to avoid deadlocks)
///
/// Release `file_locks` before awaiting a region lock or disk IO. The final
/// eviction check holds the map lock and uses only a nonblocking `try_read`
/// after verifying that no outside region handles remain.
/// Acquire a loader's pending mutex before its region lock, never the reverse.
///
/// Acquire `watchers` in its own critical section, separately from both the
/// map and region locks. Inside a loader: pending -> cached region -> receipts.
/// The async receipt guard may span backend encoding awaits; it is acquired
/// BEFORE bytes change. Completion acquires an owned receipt guard before
/// queuing its blocking task, so cancellation cannot expose a later receipt to
/// an earlier terrain write. Blocking completion owns only receipts, then briefly
/// lifecycle/publication -> store -> terrain -> metadata. It releases all of
/// those guards before POI encoding/IO. Cancellation leaves receipts owned by
/// the cached region, and draining also waits for their blocking completion.
pub struct ChunkFileManager<S: ChunkSerializer<WriteBackend = PathBuf>> {
    file_locks: RwLock<BTreeMap<PathBuf, Arc<ChunkSerializerLazyLoader<S>>>>,
    watchers: RwLock<BTreeMap<PathBuf, usize>>,
    chunk_config: S::ChunkConfig,
}

pub(crate) trait PathFromLevelFolder {
    fn file_path(folder: &LevelFolder, file_name: &str) -> PathBuf;
}

type PendingChunks<T> = BTreeMap<(i32, i32), Arc<T>>;

/// Until cached bytes and their receipt are installed, cancellation must restore
/// dirty admission on the source retained in `unapplied`.
struct EncodingAttempt<'a, D: Dirtiable> {
    source: &'a D,
    accepted: bool,
}

impl<D: Dirtiable> Drop for EncodingAttempt<'_, D> {
    fn drop(&mut self) {
        if !self.accepted {
            self.source.mark_dirty(true);
        }
    }
}

struct ChunkSerializerLazyLoader<S: ChunkSerializer<WriteBackend = PathBuf>> {
    path: PathBuf,
    // Retain the newest source chunk even when opening the region fails.
    unapplied: Mutex<PendingChunks<S::Data>>,
    /// Initialised at most once; subsequent calls reuse the same Arc.
    internal: OnceCell<Arc<RwLock<CachedRegion<S>>>>,
}

type PoiReceipts<T> = BTreeMap<(i32, i32), (Arc<T>, EncodedPoiCommit)>;

struct CachedRegion<S: ChunkSerializer<WriteBackend = PathBuf>> {
    serializer: S,
    needs_write: bool,
    // A blocking completion keeps this same owned map if its async caller is
    // cancelled. Never move the only source/receipt into a disposable future.
    receipts: Arc<Mutex<PoiReceipts<S::Data>>>,
    // Stop after actual terrain IO so a test can occupy the blocking pool before
    // completion is queued. This does not replace the backend or completion.
    #[cfg(test)]
    completion_queue_gate: Option<(
        tokio::sync::oneshot::Sender<()>,
        tokio::sync::oneshot::Receiver<()>,
    )>,
}

impl<S: ChunkSerializer<WriteBackend = PathBuf>> CachedRegion<S> {
    async fn flush(&mut self, path: &PathBuf) -> Result<(), ChunkWritingError> {
        if self.needs_write {
            self.serializer
                .write(path)
                .await
                .map_err(ChunkWritingError::IoError)?;
            self.needs_write = false;
        }
        #[cfg(test)]
        if let Some((entered, resume)) = self.completion_queue_gate.take() {
            let _ = entered.send(());
            let _ = resume.await;
        }
        // Fence the exact map before spawn_blocking can queue the task. While
        // awaiting this guard, cancellation leaves all receipts in the cache;
        // afterwards the queued task owns it even if its caller is dropped.
        let mut receipts = Arc::clone(&self.receipts).lock_owned().await;
        if receipts.is_empty() {
            return Ok(());
        }
        run_blocking(move || {
            let result = complete_poi_batch(receipts.values_mut().map(|(_, receipt)| receipt));
            receipts.retain(|_, (source, receipt)| !receipt.is_complete() || source.is_dirty());
            result
        })
        .await
        .map_err(|error| ChunkWritingError::IoError(std::io::Error::other(error)))?
    }

    async fn collect_recaptures(&self, pending: &mut PendingChunks<S::Data>) {
        let mut receipts = self.receipts.lock().await;
        receipts.retain(|position, (source, receipt)| {
            if receipt.needs_recapture() || (receipt.is_complete() && source.is_dirty()) {
                source.mark_dirty(true);
                // Newest submitted owner always wins. A stale source cannot
                // replace a newer pending Arc for the same coordinates.
                pending.entry(*position).or_insert_with(|| source.clone());
                false
            } else {
                true
            }
        });
    }
}

impl<S: ChunkSerializer<WriteBackend = PathBuf> + 'static> ChunkSerializerLazyLoader<S> {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            unapplied: Mutex::new(BTreeMap::new()),
            internal: OnceCell::new(),
        }
    }

    /// Returns `true` only when no outside caller still holds a clone of this
    /// loader *or* the inner serializer.
    ///
    /// # Safety requirement
    /// **Must be called while the write-lock on the parent `file_locks` map is
    /// held.**  That guarantees no new `Arc` clones can be issued while we
    /// inspect the strong counts.
    fn can_remove(loader: &Arc<Self>) -> bool {
        // The map itself holds 1 strong count; anything above that means an
        // active caller still has a handle.
        if Arc::strong_count(loader) > 1 {
            return false;
        }
        if !loader
            .unapplied
            .try_lock()
            .is_ok_and(|pending| pending.is_empty())
        {
            return false;
        }
        loader.internal.get().is_none_or(|arc| {
            Arc::strong_count(arc) == 1
                && arc.try_read().is_ok_and(|cached| {
                    !cached.needs_write
                        && Arc::strong_count(&cached.receipts) == 1
                        && cached
                            .receipts
                            .try_lock()
                            .is_ok_and(|receipts| receipts.is_empty())
                })
        })
    }

    /// Returns the serializer, initialising it from disk on the first call.
    async fn get(&self) -> Result<Arc<RwLock<CachedRegion<S>>>, ChunkReadingError> {
        self.internal
            .get_or_try_init(|| async {
                let serializer = self.read_from_disk().await?;
                Ok(Arc::new(RwLock::new(CachedRegion {
                    serializer,
                    needs_write: false,
                    receipts: Arc::new(Mutex::new(BTreeMap::new())),
                    #[cfg(test)]
                    completion_queue_gate: None,
                })))
            })
            .await
            .cloned()
    }

    async fn apply_pending(&self, config: &S::ChunkConfig) -> Result<(), ChunkWritingError> {
        let mut pending = self.unapplied.lock().await;
        if let Some(cached) = self.internal.get() {
            cached.write().await.collect_recaptures(&mut pending).await;
        }
        if pending.is_empty() {
            return Ok(());
        }
        let cached = self.get().await.map_err(|error| {
            ChunkWritingError::IoError(std::io::Error::other(error.to_string()))
        })?;
        let mut region = cached.write().await;
        let mut applied = Vec::new();
        let mut result = Ok(());
        for (position, chunk) in pending.iter() {
            if !chunk.is_dirty() {
                applied.push(*position);
                continue;
            }
            // Acquire before backend update: after it replaces bytes there must
            // be no cancellation point before installing their matching receipt.
            let receipts = region.receipts.clone();
            let mut receipts = receipts.lock().await;
            let mut attempt = EncodingAttempt {
                source: chunk.as_ref(),
                accepted: false,
            };
            chunk.mark_dirty(false);
            match region.serializer.update_chunk(chunk.clone(), config).await {
                Ok(receipt) => {
                    if let Some(receipt) = receipt {
                        receipts.insert(*position, (chunk.clone(), receipt));
                    } else {
                        receipts.remove(position);
                    }
                    region.needs_write = true;
                    attempt.accepted = true;
                    applied.push(*position);
                }
                Err(error) => {
                    chunk.mark_dirty(true);
                    result = ChunkWritingError::combine_results(result, Err(error));
                }
            }
        }
        for position in applied {
            pending.remove(&position);
        }
        result
    }

    async fn flush_serialized(&self) -> Result<(), ChunkWritingError> {
        let mut pending = self.unapplied.lock().await;
        if let Some(cached) = self.internal.get() {
            let mut region = cached.write().await;
            let written = region.flush(&self.path).await;
            region.collect_recaptures(&mut pending).await;
            written?;
        }
        Ok(())
    }

    async fn flush(&self, config: &S::ChunkConfig) -> Result<(), ChunkWritingError> {
        let applied = self.apply_pending(config).await;
        let written = self.flush_serialized().await;
        ChunkWritingError::combine_results(applied, written)
    }

    async fn read_from_disk(&self) -> Result<S, ChunkReadingError> {
        trace!("Opening file from disk: {}", self.path.display());

        match tokio::fs::read(&self.path).await {
            Ok(bytes) => {
                if bytes.is_empty() {
                    trace!(
                        "File is empty (0 bytes), using default for: {}",
                        self.path.display()
                    );
                    return Ok(S::default());
                }
                let value = run_blocking(move || S::read(bytes.into()))
                    .await
                    .map_err(|_| {
                        ChunkReadingError::IoError(std::io::Error::other(
                            "chunk deserialization task failed",
                        ))
                    })??;
                trace!("Successfully read file from disk: {}", self.path.display());
                Ok(value)
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                trace!("File not found, using default for: {}", self.path.display());
                Ok(S::default())
            }
            Err(err) => Err(ChunkReadingError::IoError(err)),
        }
    }
}

impl<S: ChunkSerializer<WriteBackend = PathBuf>> ChunkFileManager<S> {
    pub fn new(chunk_config: S::ChunkConfig) -> Self {
        Self {
            file_locks: RwLock::new(BTreeMap::new()),
            watchers: RwLock::new(BTreeMap::new()),
            chunk_config,
        }
    }
}

impl<S: ChunkSerializer<WriteBackend = PathBuf>> ChunkFileManager<S> {
    /// Return a retained loader before attempting any fallible disk access.
    ///
    /// Uses an optimistic read-first pattern: in the common case (cache hit)
    /// we never need a write-lock on the map.
    async fn get_loader(&self, path: &Path) -> Arc<ChunkSerializerLazyLoader<S>> {
        {
            let locks = self.file_locks.read().await;
            if let Some(loader) = locks.get(path) {
                // Clone the Arc *before* releasing the lock so it stays alive.
                return loader.clone();
            }
        }

        {
            let mut locks = self.file_locks.write().await;
            locks
                .entry(path.into())
                .or_insert_with(|| Arc::new(ChunkSerializerLazyLoader::new(path.into())))
                .clone()
            // Write-lock dropped here — `loader.get()` may block on I/O and
            // must not hold the map lock.
        }
    }

    /// Attempt to evict the cached serializer for `path`.
    ///
    /// The entry is only removed when all conditions hold:
    /// 1. No watcher still references the path.
    /// 2. No other `Arc` clone is live (ensured via `can_remove`).
    /// 3. Pending snapshots and serialized changes have reached disk.
    async fn maybe_evict(&self, path: &PathBuf) {
        // Check watchers independently of file_locks to honour lock ordering.
        let still_watched = {
            let watchers = self.watchers.read().await;
            watchers.get(path).is_some_and(|&c| c > 0)
        };

        if still_watched {
            return;
        }

        let loader = self.file_locks.read().await.get(path).cloned();
        if let Some(loader) = &loader {
            let flushed = loader.flush(&self.chunk_config).await;
            if let Err(error) = flushed {
                error!(
                    "Retaining unsaved region {} after write failure: {error}",
                    path.display()
                );
                return;
            }
        }
        drop(loader);

        let mut locks = self.file_locks.write().await;
        let removable = locks
            .get(path)
            .is_some_and(ChunkSerializerLazyLoader::can_remove);

        if removable {
            locks.remove(path);
            trace!("Evicted serializer cache for {}", path.display());
        } else {
            trace!(
                "Skipping eviction for {} — references still live",
                path.display()
            );
        }
    }
}

impl<P, S> FileIO for ChunkFileManager<S>
where
    P: PathFromLevelFolder + Send + Sync + Sized + Dirtiable + 'static,
    S: ChunkSerializer<Data = P, WriteBackend = PathBuf>,
    S::ChunkConfig: Send + Sync,
{
    type Data = Arc<S::Data>;

    async fn watch_chunks<'a>(&'a self, folder: &'a LevelFolder, chunks: &'a [Vector2<i32>]) {
        let paths: Vec<_> = chunks
            .iter()
            .map(|c| P::file_path(folder, &S::get_chunk_key(c)))
            .collect();

        let mut watchers = self.watchers.write().await;
        for path in paths {
            *watchers.entry(path).or_insert(0) += 1;
        }
    }

    async fn unwatch_chunks<'a>(&'a self, folder: &'a LevelFolder, chunks: &'a [Vector2<i32>]) {
        let paths: Vec<_> = chunks
            .iter()
            .map(|c| P::file_path(folder, &S::get_chunk_key(c)))
            .collect();

        let mut paths_to_evict = Vec::new();
        {
            let mut watchers = self.watchers.write().await;
            for path in paths {
                if let std::collections::btree_map::Entry::Occupied(mut e) = watchers.entry(path) {
                    let count = e.get_mut();
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        let (path, _) = e.remove_entry();
                        paths_to_evict.push(path);
                    }
                }
            }
        }

        for path in paths_to_evict {
            self.maybe_evict(&path).await;
        }
    }

    async fn clear_watched_chunks(&self) {
        let paths: Vec<PathBuf> = {
            let mut watchers = self.watchers.write().await;
            let keys: Vec<_> = watchers.keys().cloned().collect();
            watchers.clear();
            keys
        };
        for path in paths {
            self.maybe_evict(&path).await;
        }
    }

    async fn fetch_chunks<'a>(
        &'a self,
        folder: &'a LevelFolder,
        chunk_coords: &'a [Vector2<i32>],
        stream: mpsc::Sender<LoadedData<Self::Data, ChunkReadingError>>,
    ) {
        // Group requested chunk coords by their region file.
        let mut regions_chunks: BTreeMap<String, Vec<Vector2<i32>>> = BTreeMap::new();
        for at in chunk_coords {
            regions_chunks
                .entry(S::get_chunk_key(at))
                .or_default()
                .push(*at);
        }

        let region_tasks = regions_chunks.into_iter().map(|(file_name, chunks)| {
            let task_stream = stream.clone();
            async move {
                let path = P::file_path(folder, &file_name);
                let loader = self.get_loader(&path).await;
                let (retained, remaining) = {
                    let pending = loader.unapplied.lock().await;
                    let cached = if let Some(cached) = loader.internal.get() {
                        Some(cached.read().await)
                    } else {
                        None
                    };
                    let receipts = if let Some(cached) = &cached {
                        Some(cached.receipts.lock().await)
                    } else {
                        None
                    };
                    let mut retained = Vec::new();
                    let mut remaining = Vec::new();
                    for position in chunks {
                        if let Some(chunk) = pending.get(&(position.x, position.y)).or_else(|| {
                            receipts.as_ref().and_then(|receipts| {
                                receipts
                                    .get(&(position.x, position.y))
                                    .map(|(source, _)| source)
                            })
                        }) {
                            retained.push(chunk.clone());
                        } else {
                            remaining.push(position);
                        }
                    }
                    (retained, remaining)
                };
                for chunk in retained {
                    if task_stream.send(LoadedData::Loaded(chunk)).await.is_err() {
                        return;
                    }
                }
                if remaining.is_empty() {
                    drop(loader);
                    self.maybe_evict(&path).await;
                    return;
                }
                let chunk_serializer = match loader.get().await {
                    Ok(s) => s,
                    Err(err) => {
                        let message = err.to_string();
                        for position in remaining {
                            let error =
                                ChunkReadingError::IoError(std::io::Error::other(message.clone()));
                            if task_stream
                                .send(LoadedData::Error((position, error)))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        return;
                    }
                };

                // A bounded channel of 1 keeps backpressure between the
                // serializer and the caller without unbounded buffering.
                let (send, mut recv) = mpsc::channel::<LoadedData<S::Data, ChunkReadingError>>(1);

                // Forward received chunks, wrapping them in `Arc`.
                // Captured move is intentional — `task_stream` is consumed here.
                let forward = async move {
                    while let Some(data) = recv.recv().await {
                        let wrapped = data.map_loaded(Arc::new);
                        if task_stream.send(wrapped).await.is_err() {
                            // Receiver dropped; abort early to avoid wasted work.
                            return;
                        }
                    }
                };

                // Hold the read lock only for the duration of `get_chunks`.
                let read = async move {
                    let serializer = chunk_serializer.read().await;
                    serializer.serializer.get_chunks(remaining, send).await;
                };

                join!(forward, read);
                drop(loader);

                // Evict if not watched and references are dropped
                self.maybe_evict(&path).await;
            }
        });

        join_all(region_tasks).await;
    }

    async fn save_chunks<'a>(
        &'a self,
        folder: &'a LevelFolder,
        chunks_data: Vec<(Vector2<i32>, Self::Data)>,
    ) -> Result<(), ChunkWritingError> {
        // Group chunks by region file.
        let mut regions_chunks: BTreeMap<String, BTreeMap<(i32, i32), Self::Data>> =
            BTreeMap::new();
        for (at, chunk) in chunks_data {
            regions_chunks
                .entry(S::get_chunk_key(&at))
                .or_default()
                .insert((at.x, at.y), chunk);
        }

        let tasks = regions_chunks
            .into_iter()
            .map(|(file_name, chunk_locks)| async move {
                let path = P::file_path(folder, &file_name);
                trace!("Saving chunks into {}", path.display());

                let loader = self.get_loader(&path).await;
                loader.unapplied.lock().await.extend(chunk_locks);
                let applied = loader.apply_pending(&self.chunk_config).await;

                trace!("Chunk data updated for {}", path.display());

                // Watcher and region locks are acquired in separate sections.
                let is_watched = {
                    let watchers = self.watchers.read().await;
                    watchers.get(&path).is_some_and(|&c| c > 0)
                };

                if is_watched {
                    applied?;
                } else {
                    debug!("Flushing {} to disk", path.display());
                    let written = loader.flush_serialized().await;
                    ChunkWritingError::combine_results(applied, written)?;
                    drop(loader);
                    self.maybe_evict(&path).await;
                }

                Ok(())
            });

        // Attempt every region; a hard sibling failure dominates a stale capture.
        let results: Vec<Result<(), ChunkWritingError>> = join_all(tasks).await;
        results
            .into_iter()
            .fold(Ok(()), ChunkWritingError::combine_results)
    }

    /// Blocks until all in-flight serialiser operations have completed by
    /// acquiring (and immediately releasing) a write-lock on every cached
    /// serialiser.
    ///
    /// This is a linearisation point: after this future resolves no mutation
    /// started before the call is still running.
    async fn block_and_await_ongoing_tasks(&self) {
        // Snapshot the current set of loaders under a read-lock so we do
        // not block new insertions longer than necessary.
        let loaders: Vec<Arc<ChunkSerializerLazyLoader<S>>> =
            { self.file_locks.read().await.values().cloned().collect() };

        // For each loader that has been initialised, acquire a write-lock
        // and release it immediately.  This guarantees that any concurrent
        // read or write operation that was in progress has finished.
        let drain_tasks = loaders.into_iter().map(|loader| async move {
            let _pending = loader.unapplied.lock().await;
            if let Some(serializer_arc) = loader.internal.get() {
                // Acquiring + immediately dropping the write-lock acts as a
                // barrier: it can only succeed once all current lock holders
                // have released their guards.
                let guard = serializer_arc.write().await;
                let receipts = guard.receipts.clone();
                // A cancelled async flush may have left its blocking completion
                // alive. The shared receipt mutex is that work's final fence.
                let _completion = receipts.lock().await;
            }
        });

        join_all(drain_tasks).await;
    }

    async fn flush(&self) -> Result<(), ChunkWritingError> {
        #[expect(
            clippy::needless_collect,
            reason = "Release file_locks before awaiting region locks and disk IO."
        )]
        let loaders: Vec<_> = self.file_locks.read().await.values().cloned().collect();
        let writes = loaders
            .into_iter()
            .map(|loader| async move { loader.flush(&self.chunk_config).await });
        let results: Vec<Result<(), ChunkWritingError>> = join_all(writes).await;
        results
            .into_iter()
            .fold(Ok(()), ChunkWritingError::combine_results)
    }
}

pub enum LevelFileIO<Linear, Anvil, Pump>
where
    Linear: ChunkSerializer<WriteBackend = PathBuf>,
    Anvil: ChunkSerializer<WriteBackend = PathBuf>,
    Pump: ChunkSerializer<WriteBackend = PathBuf>,
{
    Linear(ChunkFileManager<Linear>),
    Anvil(ChunkFileManager<Anvil>),
    Pump(ChunkFileManager<Pump>),
}

impl<P, Linear, Anvil, Pump> FileIO for LevelFileIO<Linear, Anvil, Pump>
where
    P: PathFromLevelFolder + Send + Sync + Sized + Dirtiable + 'static,
    Linear: ChunkSerializer<Data = P, WriteBackend = PathBuf>,
    Anvil: ChunkSerializer<Data = P, WriteBackend = PathBuf>,
    Pump: ChunkSerializer<Data = P, WriteBackend = PathBuf>,
    Linear::ChunkConfig: Send + Sync,
    Anvil::ChunkConfig: Send + Sync,
    Pump::ChunkConfig: Send + Sync,
{
    type Data = Arc<P>;

    async fn fetch_chunks<'a>(
        &'a self,
        folder: &'a LevelFolder,
        chunk_coords: &'a [Vector2<i32>],
        stream: tokio::sync::mpsc::Sender<LoadedData<Self::Data, ChunkReadingError>>,
    ) {
        match self {
            Self::Linear(io) => io.fetch_chunks(folder, chunk_coords, stream).await,
            Self::Anvil(io) => io.fetch_chunks(folder, chunk_coords, stream).await,
            Self::Pump(io) => io.fetch_chunks(folder, chunk_coords, stream).await,
        }
    }

    async fn save_chunks<'a>(
        &'a self,
        folder: &'a LevelFolder,
        chunks_data: Vec<(Vector2<i32>, Self::Data)>,
    ) -> Result<(), ChunkWritingError> {
        match self {
            Self::Linear(io) => io.save_chunks(folder, chunks_data).await,
            Self::Anvil(io) => io.save_chunks(folder, chunks_data).await,
            Self::Pump(io) => io.save_chunks(folder, chunks_data).await,
        }
    }

    async fn watch_chunks<'a>(&'a self, folder: &'a LevelFolder, chunks: &'a [Vector2<i32>]) {
        match self {
            Self::Linear(io) => io.watch_chunks(folder, chunks).await,
            Self::Anvil(io) => io.watch_chunks(folder, chunks).await,
            Self::Pump(io) => io.watch_chunks(folder, chunks).await,
        }
    }

    async fn unwatch_chunks<'a>(&'a self, folder: &'a LevelFolder, chunks: &'a [Vector2<i32>]) {
        match self {
            Self::Linear(io) => io.unwatch_chunks(folder, chunks).await,
            Self::Anvil(io) => io.unwatch_chunks(folder, chunks).await,
            Self::Pump(io) => io.unwatch_chunks(folder, chunks).await,
        }
    }

    async fn clear_watched_chunks(&self) {
        match self {
            Self::Linear(io) => io.clear_watched_chunks().await,
            Self::Anvil(io) => io.clear_watched_chunks().await,
            Self::Pump(io) => io.clear_watched_chunks().await,
        }
    }

    async fn block_and_await_ongoing_tasks(&self) {
        match self {
            Self::Linear(io) => io.block_and_await_ongoing_tasks().await,
            Self::Anvil(io) => io.block_and_await_ongoing_tasks().await,
            Self::Pump(io) => io.block_and_await_ongoing_tasks().await,
        }
    }

    async fn flush(&self) -> Result<(), ChunkWritingError> {
        match self {
            Self::Linear(io) => io.flush().await,
            Self::Anvil(io) => io.flush().await,
            Self::Pump(io) => io.flush().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{
        ChunkData, ChunkSerializingError,
        format::anvil::{AnvilChunkFile, SingleChunkDataSerializer},
    };
    use bytes::Bytes;
    use pumpkin_data::{Block, BlockStateId};
    use std::sync::atomic::{AtomicBool, Ordering};
    use tempfile::TempDir;

    fn folder(directory: &TempDir) -> LevelFolder {
        let root = directory.path().to_path_buf();
        let folder = LevelFolder {
            root_folder: root.clone(),
            dim_folder: root.clone(),
            region_folder: root.join("region"),
            entities_folder: root.join("entities"),
            poi_folder: root.join("poi"),
        };
        std::fs::create_dir_all(&folder.region_folder).unwrap();
        folder
    }

    fn marked_chunk(x: i32, block: &Block) -> Arc<ChunkData> {
        let chunk = ChunkData::empty_sync(x, 0);
        chunk.set_block_absolute_y(3, 200, 11, block.default_state.id);
        chunk.mark_dirty(true);
        chunk
    }

    async fn disk_marker(folder: &LevelFolder, x: i32) -> BlockStateId {
        let reader = ChunkFileManager::<AnvilChunkFile<ChunkData>>::new(
            pumpkin_config::chunk::AnvilChunkConfig::default(),
        );
        let (send, mut receive) = mpsc::channel(1);
        reader
            .fetch_chunks(folder, &[Vector2::new(x, 0)], send)
            .await;
        let Some(LoadedData::Loaded(chunk)) = receive.recv().await else {
            panic!("Expected saved chunk from a fresh cache");
        };
        chunk.section.get_block_absolute_y(3, 200, 11).unwrap()
    }

    #[tokio::test]
    async fn terrain_init_failure_retains_only_source_for_reads_and_repaired_flush() {
        let directory = TempDir::new().unwrap();
        let folder = folder(&directory);
        let region = folder.region_folder.join("r.0.0.mca");
        std::fs::create_dir(&region).unwrap();
        let writer = ChunkFileManager::<AnvilChunkFile<ChunkData>>::new(
            pumpkin_config::chunk::AnvilChunkConfig::default(),
        );
        let position = Vector2::new(0, 0);
        let chunk = marked_chunk(0, &Block::DIAMOND_BLOCK);
        let runtime = crate::chunk::poi_binding::TestRuntime::new(folder.poi_folder.clone());
        chunk.bind_poi(&runtime.runtime).unwrap();
        let original = Arc::downgrade(&chunk);
        // Move the only strong reference into the failed save, as an unload does.
        assert!(
            writer
                .save_chunks(&folder, vec![(position, chunk)])
                .await
                .is_err()
        );
        assert!(original.upgrade().is_some());
        let (send, mut receive) = mpsc::channel(1);
        writer.fetch_chunks(&folder, &[position], send).await;
        let Some(LoadedData::Loaded(retained)) = receive.recv().await else {
            panic!("An unapplied source must be readable even while region initialization fails");
        };
        assert_eq!(
            retained.section.get_block_absolute_y(3, 200, 11),
            Some(Block::DIAMOND_BLOCK.default_state.id)
        );
        drop(retained);
        assert!(writer.flush().await.is_err());
        std::fs::remove_dir(&region).unwrap();
        writer.flush().await.unwrap();
        assert_eq!(
            disk_marker(&folder, 0).await,
            Block::DIAMOND_BLOCK.default_state.id
        );
    }

    #[tokio::test]
    async fn region_read_failure_replies_for_every_requested_chunk() {
        let directory = TempDir::new().unwrap();
        let folder = folder(&directory);
        std::fs::create_dir(folder.region_folder.join("r.0.0.mca")).unwrap();
        let reader = ChunkFileManager::<AnvilChunkFile<ChunkData>>::new(
            pumpkin_config::chunk::AnvilChunkConfig::default(),
        );
        let positions = [Vector2::new(0, 0), Vector2::new(1, 0), Vector2::new(2, 0)];
        let (send, mut receive) = mpsc::channel(positions.len());
        reader.fetch_chunks(&folder, &positions, send).await;
        for expected in positions {
            assert!(
                matches!(receive.recv().await, Some(LoadedData::Error((position, _))) if position == expected)
            );
        }
        assert!(receive.recv().await.is_none());
    }

    struct FallibleChunk {
        chunk: Arc<ChunkData>,
        reject: AtomicBool,
    }

    impl Dirtiable for FallibleChunk {
        fn is_dirty(&self) -> bool {
            self.chunk.is_dirty()
        }
        fn mark_dirty(&self, dirty: bool) {
            self.chunk.mark_dirty(dirty);
        }
    }

    impl PathFromLevelFolder for FallibleChunk {
        fn file_path(folder: &LevelFolder, name: &str) -> PathBuf {
            folder.region_folder.join(name)
        }
    }

    impl SingleChunkDataSerializer for FallibleChunk {
        fn to_bytes(&self) -> Result<Bytes, ChunkSerializingError> {
            if self.reject.load(Ordering::Relaxed) {
                return Err(ChunkSerializingError::ErrorSerializingChunk(
                    pumpkin_nbt::Error::SerdeError("Injected per-chunk failure".to_string()),
                ));
            }
            self.chunk.to_bytes()
        }
        fn from_bytes(bytes: &Bytes, position: Vector2<i32>) -> Result<Self, ChunkReadingError> {
            Ok(Self {
                chunk: Arc::new(ChunkData::from_bytes(bytes, position)?),
                reject: AtomicBool::new(false),
            })
        }
        fn position(&self) -> (i32, i32) {
            (self.chunk.x, self.chunk.z)
        }
    }

    #[tokio::test]
    async fn one_unserializable_chunk_does_not_block_good_chunks_in_the_same_region() {
        let directory = TempDir::new().unwrap();
        let folder = folder(&directory);
        let writer = ChunkFileManager::<AnvilChunkFile<FallibleChunk>>::new(
            pumpkin_config::chunk::AnvilChunkConfig::default(),
        );
        let bad = Arc::new(FallibleChunk {
            chunk: marked_chunk(0, &Block::DIAMOND_BLOCK),
            reject: AtomicBool::new(true),
        });
        let good = Arc::new(FallibleChunk {
            chunk: marked_chunk(1, &Block::EMERALD_BLOCK),
            reject: AtomicBool::new(false),
        });
        assert!(
            writer
                .save_chunks(
                    &folder,
                    vec![
                        (Vector2::new(0, 0), bad.clone()),
                        (Vector2::new(1, 0), good)
                    ]
                )
                .await
                .is_err()
        );
        assert_eq!(
            disk_marker(&folder, 1).await,
            Block::EMERALD_BLOCK.default_state.id
        );
        assert!(writer.flush().await.is_err());
        let (send, mut receive) = mpsc::channel(1);
        writer
            .fetch_chunks(&folder, &[Vector2::new(0, 0)], send)
            .await;
        let Some(LoadedData::Loaded(retained)) = receive.recv().await else {
            panic!("A failed source must take precedence over old serialized data");
        };
        assert_eq!(
            retained.chunk.section.get_block_absolute_y(3, 200, 11),
            Some(Block::DIAMOND_BLOCK.default_state.id)
        );
        bad.reject.store(false, Ordering::Relaxed);
        writer.flush().await.unwrap();
        assert_eq!(
            disk_marker(&folder, 0).await,
            Block::DIAMOND_BLOCK.default_state.id
        );
        assert_eq!(
            disk_marker(&folder, 1).await,
            Block::EMERALD_BLOCK.default_state.id
        );
    }

    // Poll the real operation once using this task's waker, returning Pending as
    // a value rather than yielding. No optional futures macro feature is needed.
    async fn poll_once<F: Future>(future: F) -> std::task::Poll<F::Output> {
        let mut future = std::pin::pin!(future);
        std::future::poll_fn(|context| std::task::Poll::Ready(future.as_mut().poll(context))).await
    }

    fn bound_home(runtime: &crate::chunk::poi_binding::TestRuntime, x: i32) -> Arc<ChunkData> {
        let chunk = ChunkData::empty_sync(x, 0);
        chunk.bind_poi(&runtime.runtime).unwrap();
        let home = Block::WHITE_BED
            .states
            .iter()
            .map(|state| state.id)
            .find(|state| {
                pumpkin_data::poi::PoiType::from_state(*state)
                    == Some(pumpkin_data::poi::PoiType::Home)
            })
            .unwrap();
        chunk.set_block_absolute_y(1, 0, 1, home);
        chunk
    }

    async fn saved_block<S>(folder: &LevelFolder, config: S::ChunkConfig, x: i32) -> BlockStateId
    where
        S: ChunkSerializer<Data = ChunkData, WriteBackend = PathBuf>,
        S::ChunkConfig: Send + Sync,
    {
        let reader = ChunkFileManager::<S>::new(config);
        let (send, mut receive) = mpsc::channel(1);
        reader
            .fetch_chunks(folder, &[Vector2::new(x, 0)], send)
            .await;
        let Some(LoadedData::Loaded(chunk)) = receive.recv().await else {
            panic!("missing terrain");
        };
        chunk.section.get_block_absolute_y(1, 0, 1).unwrap()
    }

    // UNRUN: the runtime's only blocking thread is occupied AFTER actual terrain
    // IO. The completion task is queued, not paused after acquiring its mutex.
    #[allow(
        clippy::too_many_lines,
        reason = "Keep the queued completion, cancellation, replacement save and disk observations together."
    )]
    async fn queued_completion_fences_replacement<S>(config: S::ChunkConfig)
    where
        S: ChunkSerializer<Data = ChunkData, WriteBackend = PathBuf>,
        S::ChunkConfig: Clone + Send + Sync + 'static,
    {
        use crate::chunk::poi_binding::TestRuntime;
        use crate::poi::PoiRegion;
        use pumpkin_data::poi::PoiType;
        use pumpkin_util::math::position::BlockPos;
        let directory = TempDir::new().unwrap();
        let folder = folder(&directory);
        let runtime = TestRuntime::new(folder.poi_folder.clone());
        let pos = Vector2::new(0, 0);
        let writer = ChunkFileManager::<S>::new(config.clone());
        let a = bound_home(&runtime, 0);
        let home = a.section.get_block_absolute_y(1, 0, 1).unwrap();
        runtime.chunks.insert(pos, a.clone());
        writer
            .save_chunks(&folder, vec![(pos, a.clone())])
            .await
            .unwrap();
        let path = folder.region_folder.join(S::get_chunk_key(&pos));
        let poi_path = folder.poi_folder.join("r.0.0.mca");
        let writes_before = {
            let mut storage = runtime.runtime.storage.lock().unwrap();
            storage
                .add_with_free_tickets(BlockPos::new(1, 0, 1), PoiType::Home.identifier(), 0)
                .unwrap();
            storage.save_all().unwrap();
            storage.completed_region_writes(pos)
        };
        let poi_before = std::fs::read(&poi_path).unwrap();
        writer.watch_chunks(&folder, &[pos]).await;
        a.set_block_absolute_y(2, 0, 1, Block::STONE.default_state.id);
        writer
            .save_chunks(&folder, vec![(pos, a.clone())])
            .await
            .unwrap();
        let loader = writer.get_loader(&path).await;
        let cached = loader.get().await.unwrap();
        let (entered, reached) = tokio::sync::oneshot::channel();
        let (resume, resumed) = tokio::sync::oneshot::channel();
        cached.write().await.completion_queue_gate = Some((entered, resumed));
        let mut flush = Box::pin(writer.flush());
        tokio::select! {
            result = &mut flush => panic!("completion bypassed queue gate: {result:?}"),
            result = reached => result.unwrap(),
        }
        assert_eq!(saved_block::<S>(&folder, config.clone(), 0).await, home);
        let terrain_a = std::fs::read(&path).unwrap();
        assert_eq!(std::fs::read(&poi_path).unwrap(), poi_before);

        let (occupied, ready) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        let blocker = tokio::task::spawn_blocking(move || {
            occupied.send(()).unwrap();
            released
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
        });
        ready.await.unwrap();
        resume.send(()).unwrap();
        // Ignore cooperative budget so Pending means awaited work, not a yield.
        // One explicit poll submits completion to the occupied blocking pool.
        assert!(
            poll_once(tokio::task::unconstrained(flush.as_mut()))
                .await
                .is_pending()
        );
        drop(flush); // Cancel while its blocking completion has not started.
        {
            let mut drain = Box::pin(writer.block_and_await_ongoing_tasks());
            assert!(
                poll_once(tokio::task::unconstrained(drain.as_mut()))
                    .await
                    .is_pending()
            );
            // Cancelling a guard waiter must also release pending/cache locks.
            drop(drain);
        };

        // The same published Arc changes after A's terrain write. Its later ABA
        // must survive A's completion and remain private until B's terrain write.
        let b = a.clone();
        b.set_blocks_batch([(1, 0, 1, BlockStateId::AIR), (1, 0, 1, home)]);
        b.set_block_absolute_y(2, 0, 1, Block::DIRT.default_state.id);
        let original_b = Arc::downgrade(&b);
        let mut save_b = Box::pin(writer.save_chunks(&folder, vec![(pos, b)]));
        assert!(
            poll_once(tokio::task::unconstrained(save_b.as_mut()))
                .await
                .is_pending()
        );
        assert!(original_b.upgrade().unwrap().is_dirty());
        drop(save_b); // Cancellation while waiting leaves B admitted and dirty.
        {
            let pending = loader.unapplied.lock().await;
            assert!(Arc::ptr_eq(
                pending.get(&(pos.x, pos.y)).unwrap(),
                &original_b.upgrade().unwrap()
            ));
            drop(pending);
        };
        // Resume the retained submission through the same apply_pending path
        // used by watched saves, without resubmitting B or writing its terrain.
        let mut apply_b = Box::pin(loader.apply_pending(&writer.chunk_config));
        assert!(
            poll_once(tokio::task::unconstrained(apply_b.as_mut()))
                .await
                .is_pending()
        );
        assert_eq!(std::fs::read(&poi_path).unwrap(), poi_before);
        assert_eq!(std::fs::read(&path).unwrap(), terrain_a);
        release.send(()).unwrap();
        blocker.await.unwrap();
        apply_b.await.unwrap();
        writer.block_and_await_ongoing_tasks().await;

        // A's stale worker finishes before B can install bytes and its receipt.
        // B's ABA stays private through save_all until B's actual terrain write.
        runtime.runtime.storage.lock().unwrap().save_all().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), terrain_a);
        assert_eq!(std::fs::read(&poi_path).unwrap(), poi_before);
        assert_eq!(
            PoiRegion::load(&poi_path).unwrap().get_all()[0].free_tickets,
            0
        );
        {
            let region = cached.read().await;
            assert!(region.needs_write);
            let receipts = region.receipts.lock().await;
            let (source, receipt) = receipts.get(&(pos.x, pos.y)).unwrap();
            assert!(Arc::ptr_eq(source, &original_b.upgrade().unwrap()));
            assert!(!receipt.is_complete());
            drop(receipts);
            drop(region);
        };
        writer.flush().await.unwrap();
        let reader = ChunkFileManager::<S>::new(config);
        let (send, mut receive) = mpsc::channel(1);
        reader.fetch_chunks(&folder, &[pos], send).await;
        let Some(LoadedData::Loaded(saved_b)) = receive.recv().await else {
            panic!("missing completed terrain B");
        };
        assert_eq!(saved_b.section.get_block_absolute_y(1, 0, 1), Some(home));
        assert_eq!(
            saved_b.section.get_block_absolute_y(2, 0, 1),
            Some(Block::DIRT.default_state.id)
        );
        assert_ne!(std::fs::read(&path).unwrap(), terrain_a);
        assert_eq!(
            PoiRegion::load(&poi_path).unwrap().get_all()[0].poi_type,
            PoiType::Home.identifier()
        );
        assert_eq!(
            PoiRegion::load(&poi_path).unwrap().get_all()[0].free_tickets,
            1
        );
        assert_eq!(
            runtime
                .runtime
                .storage
                .lock()
                .unwrap()
                .completed_region_writes(pos),
            writes_before + 1
        );
        assert!(!a.is_dirty());
        assert!(cached.read().await.receipts.lock().await.is_empty());
    }

    #[test]
    fn all_formats_queued_completion_fences_cancelled_caller_and_replacement_save() {
        tokio::runtime::Builder::new_current_thread()
            .max_blocking_threads(1)
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                tokio::time::timeout(std::time::Duration::from_secs(15), async {
                    Box::pin(queued_completion_fences_replacement::<
                        AnvilChunkFile<ChunkData>,
                    >(
                        pumpkin_config::chunk::AnvilChunkConfig::default()
                    ))
                    .await;
                    Box::pin(queued_completion_fences_replacement::<
                        crate::chunk::format::linear::LinearV2File<ChunkData>,
                    >(()))
                    .await;
                    Box::pin(queued_completion_fences_replacement::<
                        crate::chunk::format::pump::PumpFile<ChunkData>,
                    >(()))
                    .await;
                })
                .await
                .unwrap();
            });
    }

    // UNRUN: actual all-format completion, cancellation, fetch and eviction.
    // The only pause is immediately before the real reserved POI file write.
    async fn poi_failure_retains_cancelled_completion<S>(config: S::ChunkConfig)
    where
        S: ChunkSerializer<Data = ChunkData, WriteBackend = PathBuf>,
        S::ChunkConfig: Clone + Send + Sync + 'static,
    {
        use crate::chunk::poi_binding::TestRuntime;
        use crate::poi::PoiRegion;
        use pumpkin_data::poi::PoiType;
        use pumpkin_util::math::position::BlockPos;
        let directory = TempDir::new().unwrap();
        let folder = folder(&directory);
        std::fs::create_dir_all(&folder.poi_folder).unwrap();
        let runtime = TestRuntime::new(folder.poi_folder.clone());
        let pos = Vector2::new(0, 0);
        let writer = Arc::new(ChunkFileManager::<S>::new(config.clone()));
        writer.watch_chunks(&folder, &[pos]).await;
        let source = bound_home(&runtime, 0);
        let original = Arc::downgrade(&source);
        let home = source.section.get_block_absolute_y(1, 0, 1).unwrap();
        // No published map entry: this is the existing retired/queued IO owner.
        writer
            .save_chunks(&folder, vec![(pos, source)])
            .await
            .unwrap();
        let poi_path = folder.poi_folder.join("r.0.0.mca");
        std::fs::create_dir(&poi_path).unwrap();
        let (entered, release) = runtime
            .runtime
            .storage
            .lock()
            .unwrap()
            .pause_region_write(pos);
        let flushing = writer.clone();
        let flush = tokio::spawn(async move { flushing.flush().await });
        tokio::time::timeout(std::time::Duration::from_secs(5), entered)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved_block::<S>(&folder, config.clone(), 0).await, home);
        flush.abort();
        assert!(flush.await.unwrap_err().is_cancelled());
        assert!(original.upgrade().is_some());
        {
            // Store access remains available during IO; changing current tickets
            // after snapshot capture must survive both failed and later writes.
            let mut storage = runtime.runtime.storage.lock().unwrap();
            storage
                .add_with_free_tickets(BlockPos::new(1, 0, 1), PoiType::Home.identifier(), -7)
                .unwrap();
            assert_eq!(
                storage.save_all().unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock
            );
            assert!(storage.region_is_dirty(pos));
            drop(storage);
        };
        let healthy = bound_home(&runtime, 32);
        writer
            .save_chunks(&folder, vec![(Vector2::new(32, 0), healthy)])
            .await
            .unwrap();
        assert_eq!(
            PoiRegion::load(&folder.poi_folder.join("r.1.0.mca"))
                .unwrap()
                .get_all()
                .len(),
            1
        );
        release.send(()).unwrap();
        writer.block_and_await_ongoing_tasks().await;
        writer.unwatch_chunks(&folder, &[pos]).await;
        let path = folder.region_folder.join(S::get_chunk_key(&pos));
        assert!(writer.file_locks.read().await.contains_key(&path));
        assert!(writer.flush().await.is_err());
        let (send, mut receive) = mpsc::channel(1);
        writer.fetch_chunks(&folder, &[pos], send).await;
        let Some(LoadedData::Loaded(retained)) = receive.recv().await else {
            panic!("lost pending POI source");
        };
        assert!(Arc::ptr_eq(&retained, &original.upgrade().unwrap()));
        drop(retained);
        std::fs::remove_dir(&poi_path).unwrap();
        // No source submission, re-encoding or new terrain write is required.
        let terrain_before = std::fs::read(&path).unwrap();
        writer.flush().await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), terrain_before);
        let poi = PoiRegion::load(&poi_path).unwrap();
        assert_eq!(poi.get_all()[0].free_tickets, -7);
        assert_eq!(
            runtime
                .runtime
                .storage
                .lock()
                .unwrap()
                .completed_region_writes(pos),
            1
        );
        assert!(original.upgrade().is_none());
        writer.maybe_evict(&path).await;
        assert!(!writer.file_locks.read().await.contains_key(&path));
    }

    #[tokio::test]
    async fn anvil_poi_failure_retains_cancelled_completion_until_retry() {
        Box::pin(poi_failure_retains_cancelled_completion::<
            AnvilChunkFile<ChunkData>,
        >(pumpkin_config::chunk::AnvilChunkConfig::default()))
        .await;
    }
    #[tokio::test]
    async fn linear_poi_failure_retains_cancelled_completion_until_retry() {
        Box::pin(poi_failure_retains_cancelled_completion::<
            crate::chunk::format::linear::LinearV2File<ChunkData>,
        >(()))
        .await;
    }
    #[tokio::test]
    async fn pump_poi_failure_retains_cancelled_completion_until_retry() {
        Box::pin(poi_failure_retains_cancelled_completion::<
            crate::chunk::format::pump::PumpFile<ChunkData>,
        >(()))
        .await;
    }

    // UNRUN: a real terrain write refusal cannot leak otherwise encodable A
    // through save_all. Retained bytes/receipt retry without caller resubmission.
    async fn terrain_refusal_and_batch<S>(config: S::ChunkConfig)
    where
        S: ChunkSerializer<Data = ChunkData, WriteBackend = PathBuf>,
        S::ChunkConfig: Clone + Send + Sync + 'static,
    {
        use crate::chunk::poi_binding::TestRuntime;
        use crate::poi::PoiRegion;
        let directory = TempDir::new().unwrap();
        let folder = folder(&directory);
        let runtime = TestRuntime::new(folder.poi_folder.clone());
        let pos = Vector2::new(0, 0);
        let other = Vector2::new(1, 0);
        let writer = ChunkFileManager::<S>::new(config.clone());
        writer.watch_chunks(&folder, &[pos, other]).await;
        let a = bound_home(&runtime, 0);
        let b = bound_home(&runtime, 1);
        writer
            .save_chunks(&folder, vec![(pos, a.clone()), (other, b)])
            .await
            .unwrap();
        let path = folder.region_folder.join(S::get_chunk_key(&pos));
        std::fs::create_dir(&path).unwrap();
        assert!(writer.flush().await.is_err());
        runtime.runtime.storage.lock().unwrap().save_all().unwrap();
        let poi_path = folder.poi_folder.join("r.0.0.mca");
        assert!(!poi_path.exists());
        let (send, mut receive) = mpsc::channel(1);
        writer.fetch_chunks(&folder, &[pos], send).await;
        let Some(LoadedData::Loaded(retained)) = receive.recv().await else {
            panic!("lost refused terrain source");
        };
        assert!(Arc::ptr_eq(&a, &retained));
        std::fs::remove_dir(&path).unwrap();
        writer.flush().await.unwrap();
        assert_eq!(PoiRegion::load(&poi_path).unwrap().get_all().len(), 2);
        // Both completed chunks touch the same POI region: exactly one real write.
        assert_eq!(
            runtime
                .runtime
                .storage
                .lock()
                .unwrap()
                .completed_region_writes(pos),
            1
        );
        writer.flush().await.unwrap();
        assert_eq!(
            runtime
                .runtime
                .storage
                .lock()
                .unwrap()
                .completed_region_writes(pos),
            1
        );
        assert_eq!(
            saved_block::<S>(&folder, config, 0).await,
            a.section.get_block_absolute_y(1, 0, 1).unwrap()
        );
    }

    #[tokio::test]
    async fn anvil_terrain_refusal_retries_one_poi_write_per_batch() {
        Box::pin(terrain_refusal_and_batch::<AnvilChunkFile<ChunkData>>(
            pumpkin_config::chunk::AnvilChunkConfig::default(),
        ))
        .await;
    }
    #[tokio::test]
    async fn linear_terrain_refusal_retries_one_poi_write_per_batch() {
        Box::pin(terrain_refusal_and_batch::<
            crate::chunk::format::linear::LinearV2File<ChunkData>,
        >(()))
        .await;
    }
    #[tokio::test]
    async fn pump_terrain_refusal_retries_one_poi_write_per_batch() {
        Box::pin(terrain_refusal_and_batch::<
            crate::chunk::format::pump::PumpFile<ChunkData>,
        >(()))
        .await;
    }

    // UNRUN: replace cached bytes, make a late ABA write, and finally replace the
    // publication Arc while an old receipt exists. All IO uses the actual backend.
    #[allow(
        clippy::too_many_lines,
        reason = "Keep the actual capture, late-write refusal and recovery observations together."
    )]
    async fn late_capture_and_replacement<S>(config: S::ChunkConfig)
    where
        S: ChunkSerializer<Data = ChunkData, WriteBackend = PathBuf>,
        S::ChunkConfig: Clone + Send + Sync + 'static,
    {
        use crate::chunk::poi_binding::TestRuntime;
        use crate::poi::PoiRegion;
        use pumpkin_data::poi::PoiType;
        use pumpkin_util::math::position::BlockPos;
        let directory = TempDir::new().unwrap();
        let folder = folder(&directory);
        let runtime = TestRuntime::new(folder.poi_folder.clone());
        let pos = Vector2::new(0, 0);
        let writer = ChunkFileManager::<S>::new(config.clone());
        let source = bound_home(&runtime, 0);
        let home = source.section.get_block_absolute_y(1, 0, 1).unwrap();
        runtime.chunks.insert(pos, source.clone());
        writer
            .save_chunks(&folder, vec![(pos, source.clone())])
            .await
            .unwrap();
        {
            let mut storage = runtime.runtime.storage.lock().unwrap();
            storage
                .add_with_free_tickets(BlockPos::new(1, 0, 1), PoiType::Home.identifier(), 0)
                .unwrap();
            storage.save_all().unwrap();
            drop(storage);
        };
        writer.watch_chunks(&folder, &[pos]).await;
        source.set_block_absolute_y(2, 0, 1, Block::STONE.default_state.id);
        writer
            .save_chunks(&folder, vec![(pos, source.clone())])
            .await
            .unwrap();
        runtime
            .runtime
            .storage
            .lock()
            .unwrap()
            .add_with_free_tickets(BlockPos::new(1, 0, 1), PoiType::Home.identifier(), -7)
            .unwrap();
        writer.flush().await.unwrap();
        let path = folder.poi_folder.join("r.0.0.mca");
        assert_eq!(
            PoiRegion::load(&path).unwrap().get_all()[0].free_tickets,
            -7
        );
        {
            let mut storage = runtime.runtime.storage.lock().unwrap();
            storage
                .add_with_free_tickets(BlockPos::new(1, 0, 1), PoiType::Home.identifier(), 0)
                .unwrap();
            storage.save_all().unwrap();
            drop(storage);
        };
        source.set_block_absolute_y(1, 0, 1, Block::BELL.default_state.id);
        writer
            .save_chunks(&folder, vec![(pos, source.clone())])
            .await
            .unwrap();
        source.set_block_absolute_y(1, 0, 1, home);
        writer
            .save_chunks(&folder, vec![(pos, source.clone())])
            .await
            .unwrap();
        // Later revision invalidates even the newer receipt despite equal HOME.
        source.set_blocks_batch([(1, 0, 1, BlockStateId::AIR), (1, 0, 1, home)]);
        assert!(writer.flush().await.is_err());
        let poi_path = folder.poi_folder.join("r.0.0.mca");
        runtime.runtime.storage.lock().unwrap().save_all().unwrap();
        assert_eq!(
            PoiRegion::load(&poi_path).unwrap().get_all()[0].free_tickets,
            0
        );
        assert!(source.is_dirty());
        writer.flush().await.unwrap();
        assert_eq!(
            PoiRegion::load(&poi_path).unwrap().get_all()[0].free_tickets,
            1
        );
        source.set_block_absolute_y(1, 0, 1, Block::BELL.default_state.id);
        writer
            .save_chunks(&folder, vec![(pos, source.clone())])
            .await
            .unwrap();
        let replacement = bound_home(&runtime, 0);
        replacement.set_block_absolute_y(1, 0, 1, Block::LECTERN.default_state.id);
        {
            let lifecycle = runtime.runtime.lifecycle.write().unwrap();
            runtime.chunks.insert(pos, replacement.clone());
            drop(lifecycle);
        };
        assert!(writer.flush().await.is_err());
        assert_eq!(
            PoiRegion::load(&poi_path).unwrap().get_all()[0].poi_type,
            PoiType::Home.identifier()
        );
        assert!(replacement.is_dirty());
        writer
            .save_chunks(&folder, vec![(pos, replacement.clone())])
            .await
            .unwrap();
        writer.flush().await.unwrap();
        assert!(Arc::ptr_eq(
            runtime.chunks.get(&pos).unwrap().value(),
            &replacement
        ));
        assert_eq!(
            PoiRegion::load(&poi_path).unwrap().get_all()[0].poi_type,
            PoiType::Librarian.identifier()
        );
        assert_eq!(
            saved_block::<S>(&folder, config, 0).await,
            Block::LECTERN.default_state.id
        );
    }

    #[tokio::test]
    async fn anvil_late_aba_and_replacement_cannot_ack_old_receipts() {
        Box::pin(late_capture_and_replacement::<AnvilChunkFile<ChunkData>>(
            pumpkin_config::chunk::AnvilChunkConfig::default(),
        ))
        .await;
    }
    #[tokio::test]
    async fn linear_late_aba_and_replacement_cannot_ack_old_receipts() {
        Box::pin(late_capture_and_replacement::<
            crate::chunk::format::linear::LinearV2File<ChunkData>,
        >(()))
        .await;
    }
    #[tokio::test]
    async fn pump_late_aba_and_replacement_cannot_ack_old_receipts() {
        Box::pin(late_capture_and_replacement::<
            crate::chunk::format::pump::PumpFile<ChunkData>,
        >(()))
        .await;
    }

    #[tokio::test]
    async fn anvil_compression_refusal_keeps_capture_private_and_source_retryable() {
        use crate::chunk::poi_binding::TestRuntime;
        use crate::poi::PoiRegion;
        use pumpkin_data::poi::PoiType;
        let directory = TempDir::new().unwrap();
        let folder = folder(&directory);
        let runtime = TestRuntime::new(folder.poi_folder.clone());
        let source = bound_home(&runtime, 0);
        let pos = Vector2::new(0, 0);
        let mut config = pumpkin_config::chunk::AnvilChunkConfig::default();
        config.compression.algorithm = pumpkin_config::chunk::Compression::LZ4;
        config.compression.level = 0;
        let mut writer = ChunkFileManager::<AnvilChunkFile<ChunkData>>::new(config);
        writer
            .save_chunks(&folder, vec![(pos, source.clone())])
            .await
            .unwrap();
        let poi_path = folder.poi_folder.join("r.0.0.mca");
        let before = std::fs::read(&poi_path).unwrap();
        source.set_block_absolute_y(1, 0, 1, Block::BELL.default_state.id);
        writer.chunk_config.compression.level = 16;
        assert!(matches!(
            writer
                .save_chunks(&folder, vec![(pos, source.clone())])
                .await,
            Err(ChunkWritingError::Compression(_))
        ));
        assert!(source.is_dirty());
        runtime.runtime.storage.lock().unwrap().save_all().unwrap();
        assert_eq!(std::fs::read(&poi_path).unwrap(), before);
        writer.chunk_config.compression.level = 0;
        writer.flush().await.unwrap();
        assert_eq!(
            PoiRegion::load(&poi_path).unwrap().get_all()[0].poi_type,
            PoiType::Meeting.identifier()
        );
    }

    async fn concurrent_poi_mutation_keeps_snapshot_dirty<S>(config: S::ChunkConfig)
    where
        S: ChunkSerializer<Data = ChunkData, WriteBackend = PathBuf>,
        S::ChunkConfig: Clone + Send + Sync + 'static,
    {
        use crate::chunk::poi_binding::TestRuntime;
        use crate::poi::PoiRegion;
        use pumpkin_data::poi::PoiType;
        use pumpkin_util::math::position::BlockPos;
        for late_terrain_write in [false, true] {
            let directory = TempDir::new().unwrap();
            let folder = folder(&directory);
            let runtime = TestRuntime::new(folder.poi_folder.clone());
            let pos = Vector2::new(0, 0);
            let writer = Arc::new(ChunkFileManager::<S>::new(config.clone()));
            writer.watch_chunks(&folder, &[pos]).await;
            let source = bound_home(&runtime, 0);
            let home = source.section.get_block_absolute_y(1, 0, 1).unwrap();
            writer
                .save_chunks(&folder, vec![(pos, source.clone())])
                .await
                .unwrap();
            let (entered, release) = runtime
                .runtime
                .storage
                .lock()
                .unwrap()
                .pause_region_write(pos);
            let flushing = writer.clone();
            let flush = tokio::spawn(async move { flushing.flush().await });
            tokio::time::timeout(std::time::Duration::from_secs(5), entered)
                .await
                .unwrap()
                .unwrap();
            runtime
                .runtime
                .storage
                .lock()
                .unwrap()
                .add_with_free_tickets(BlockPos::new(1, 0, 1), PoiType::Home.identifier(), -9)
                .unwrap();
            if late_terrain_write {
                source.set_blocks_batch([(1, 0, 1, BlockStateId::AIR), (1, 0, 1, home)]);
            }
            release.send(()).unwrap();
            let completed = flush.await.unwrap();
            if late_terrain_write {
                assert!(completed.is_err());
            } else {
                completed.unwrap();
            }
            let path = folder.poi_folder.join("r.0.0.mca");
            assert_eq!(PoiRegion::load(&path).unwrap().get_all()[0].free_tickets, 1);
            {
                let mut storage = runtime.runtime.storage.lock().unwrap();
                assert!(storage.region_is_dirty(pos));
                // Also proves successful completion released its reservation.
                storage.save_all().unwrap();
                assert!(!storage.region_is_dirty(pos));
                assert_eq!(storage.completed_region_writes(pos), 2);
                drop(storage);
            };
            assert_eq!(
                PoiRegion::load(&path).unwrap().get_all()[0].free_tickets,
                -9
            );
            if late_terrain_write {
                // The late ABA was not acknowledged by the old snapshot's success.
                assert!(source.is_dirty());
                writer.flush().await.unwrap();
                assert_eq!(PoiRegion::load(&path).unwrap().get_all()[0].free_tickets, 1);
            }
        }
    }

    #[tokio::test]
    async fn anvil_snapshot_does_not_ack_concurrent_poi_mutation() {
        Box::pin(concurrent_poi_mutation_keeps_snapshot_dirty::<
            AnvilChunkFile<ChunkData>,
        >(pumpkin_config::chunk::AnvilChunkConfig::default()))
        .await;
    }
    #[tokio::test]
    async fn linear_snapshot_does_not_ack_concurrent_poi_mutation() {
        Box::pin(concurrent_poi_mutation_keeps_snapshot_dirty::<
            crate::chunk::format::linear::LinearV2File<ChunkData>,
        >(()))
        .await;
    }
    #[tokio::test]
    async fn pump_snapshot_does_not_ack_concurrent_poi_mutation() {
        Box::pin(concurrent_poi_mutation_keeps_snapshot_dirty::<
            crate::chunk::format::pump::PumpFile<ChunkData>,
        >(()))
        .await;
    }

    // UNRUN: cancellation while the real chunk encoder is awaiting its first
    // terrain field cannot pair the previous cached bytes with a new receipt or
    // silently skip the retained source because admission cleared its dirty bit.
    async fn cancelled_encoding_keeps_bytes_and_receipt_paired<S>(config: S::ChunkConfig)
    where
        S: ChunkSerializer<Data = ChunkData, WriteBackend = PathBuf>,
        S::ChunkConfig: Clone + Send + Sync + 'static,
    {
        use crate::chunk::poi_binding::TestRuntime;
        use crate::poi::PoiRegion;
        use pumpkin_data::poi::PoiType;
        let directory = TempDir::new().unwrap();
        let folder = Arc::new(folder(&directory));
        let runtime = TestRuntime::new(folder.poi_folder.clone());
        let pos = Vector2::new(0, 0);
        let source = bound_home(&runtime, 0);
        let home = source.section.get_block_absolute_y(1, 0, 1).unwrap();
        let writer = Arc::new(ChunkFileManager::<S>::new(config.clone()));
        writer
            .save_chunks(&folder, vec![(pos, source.clone())])
            .await
            .unwrap();
        writer.watch_chunks(&folder, &[pos]).await;
        source.set_block_absolute_y(1, 0, 1, Block::BELL.default_state.id);
        writer
            .save_chunks(&folder, vec![(pos, source.clone())])
            .await
            .unwrap();
        source.set_block_absolute_y(1, 0, 1, home);
        let (held, ready) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        let blocked_source = source.clone();
        let blocker = std::thread::spawn(move || {
            let _light = blocked_source.light_engine.lock().unwrap();
            held.send(()).unwrap();
            released
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
        });
        ready.await.unwrap();
        let saving = writer.clone();
        let saving_folder = folder.clone();
        let saving_source = source.clone();
        let save = tokio::spawn(async move {
            saving
                .save_chunks(&saving_folder, vec![(pos, saving_source)])
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while source.is_dirty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        save.abort();
        assert!(save.await.unwrap_err().is_cancelled());
        assert!(source.is_dirty());
        release.send(()).unwrap();
        blocker.join().unwrap();
        writer.flush().await.unwrap();
        assert_eq!(saved_block::<S>(&folder, config, 0).await, home);
        let poi = PoiRegion::load(&folder.poi_folder.join("r.0.0.mca")).unwrap();
        assert_eq!(poi.get_all()[0].poi_type, PoiType::Home.identifier());
        assert_eq!(poi.get_all()[0].free_tickets, 1);
        assert!(!source.is_dirty());
    }

    #[tokio::test]
    async fn anvil_cancelled_encoding_retains_matching_bytes_and_source() {
        Box::pin(cancelled_encoding_keeps_bytes_and_receipt_paired::<
            AnvilChunkFile<ChunkData>,
        >(pumpkin_config::chunk::AnvilChunkConfig::default()))
        .await;
    }
    #[tokio::test]
    async fn linear_cancelled_encoding_retains_matching_bytes_and_source() {
        Box::pin(cancelled_encoding_keeps_bytes_and_receipt_paired::<
            crate::chunk::format::linear::LinearV2File<ChunkData>,
        >(()))
        .await;
    }
    #[tokio::test]
    async fn pump_cancelled_encoding_retains_matching_bytes_and_source() {
        Box::pin(cancelled_encoding_keeps_bytes_and_receipt_paired::<
            crate::chunk::format::pump::PumpFile<ChunkData>,
        >(()))
        .await;
    }

    // R28 additions are source-authored and UNRUN. Every encoder, region write
    // and decoder below is the actual backend; only admission can be paused.
    #[allow(
        clippy::too_many_lines,
        reason = "Keep exact-source refusal and fresh disk proof together."
    )]
    async fn stale_admission_retains_source<S>(config: S::ChunkConfig)
    where
        S: ChunkSerializer<Data = ChunkData, WriteBackend = PathBuf>,
        S::ChunkConfig: Clone + Send + Sync + 'static,
    {
        use crate::chunk::poi_binding::TestRuntime;
        use crate::poi::PoiRegion;
        use pumpkin_data::poi::PoiType;
        use pumpkin_util::math::position::BlockPos;
        for watched in [false, true] {
            for aba in [false, true] {
                let directory = TempDir::new().unwrap();
                let folder = folder(&directory);
                let runtime = TestRuntime::new(folder.poi_folder.clone());
                let writer = ChunkFileManager::<S>::new(config.clone());
                let source = bound_home(&runtime, 0);
                let pos = Vector2::new(0, 0);
                let home = source.section.get_block_absolute_y(1, 0, 1).unwrap();
                runtime.chunks.insert(pos, source.clone());
                writer
                    .save_chunks(&folder, vec![(pos, source.clone())])
                    .await
                    .unwrap();
                {
                    let mut storage = runtime.runtime.storage.lock().unwrap();
                    storage
                        .add_with_free_tickets(
                            BlockPos::new(1, 0, 1),
                            PoiType::Home.identifier(),
                            0,
                        )
                        .unwrap();
                    storage.save_all().unwrap();
                }
                let path = folder.region_folder.join(S::get_chunk_key(&pos));
                let poi_path = folder.poi_folder.join("r.0.0.mca");
                let terrain_before = std::fs::read(&path).unwrap();
                let poi_before = std::fs::read(&poi_path).unwrap();
                if watched {
                    writer.watch_chunks(&folder, &[pos]).await;
                }
                source.set_block_absolute_y(2, 0, 1, Block::STONE.default_state.id);
                let (entered, resume) = runtime.runtime.pause_capture_admission(&source);
                let mut saving = Box::pin(writer.save_chunks(&folder, vec![(pos, source.clone())]));
                tokio::select! {
                    result = &mut saving => panic!("capture bypassed admission gate: {result:?}"),
                    result = tokio::time::timeout(std::time::Duration::from_secs(5), entered) => result.unwrap().unwrap(),
                }
                if aba {
                    source.set_blocks_batch([(1, 0, 1, BlockStateId::AIR), (1, 0, 1, home)]);
                }
                // An ordinary non-POI setter alone must also invalidate capture.
                source.set_block_absolute_y(2, 0, 1, Block::DIRT.default_state.id);
                resume.send(()).unwrap();
                assert!(matches!(
                    saving.await,
                    Err(ChunkWritingError::StaleSaveCapture)
                ));
                assert!(source.is_dirty());
                let loader = writer.get_loader(&path).await;
                assert!(Arc::ptr_eq(
                    loader.unapplied.lock().await.get(&(0, 0)).unwrap(),
                    &source
                ));
                assert_eq!(std::fs::read(&path).unwrap(), terrain_before);
                assert_eq!(std::fs::read(&poi_path).unwrap(), poi_before);
                // No resubmission: the failed backend admission owns this retry.
                writer.flush().await.unwrap();
                let reader = ChunkFileManager::<S>::new(config.clone());
                let (send, mut receive) = mpsc::channel(1);
                reader.fetch_chunks(&folder, &[pos], send).await;
                let Some(LoadedData::Loaded(saved)) = receive.recv().await else {
                    panic!("missing retried terrain")
                };
                assert!(!Arc::ptr_eq(&saved, &source));
                assert_eq!(saved.section.get_block_absolute_y(1, 0, 1), Some(home));
                assert_eq!(
                    saved.section.get_block_absolute_y(2, 0, 1),
                    Some(Block::DIRT.default_state.id)
                );
                let poi = PoiRegion::load(&poi_path).unwrap();
                let entry = poi
                    .get_all()
                    .into_iter()
                    .find(|entry| entry.pos() == BlockPos::new(1, 0, 1))
                    .unwrap();
                assert_eq!(entry.poi_type, PoiType::Home.identifier());
                assert_eq!(entry.free_tickets, if aba { 1 } else { 0 });
                assert!(!source.is_dirty());
                assert!(loader.unapplied.lock().await.is_empty());
                assert!(
                    loader
                        .get()
                        .await
                        .unwrap()
                        .read()
                        .await
                        .receipts
                        .lock()
                        .await
                        .is_empty()
                );
            }
        }
    }

    #[tokio::test]
    async fn all_formats_stale_admission_retries_exact_source_and_aba_without_resubmission() {
        Box::pin(stale_admission_retains_source::<AnvilChunkFile<ChunkData>>(
            pumpkin_config::chunk::AnvilChunkConfig::default(),
        ))
        .await;
        Box::pin(stale_admission_retains_source::<
            crate::chunk::format::linear::LinearV2File<ChunkData>,
        >(()))
        .await;
        Box::pin(stale_admission_retains_source::<
            crate::chunk::format::pump::PumpFile<ChunkData>,
        >(()))
        .await;
    }

    #[allow(
        clippy::too_many_lines,
        reason = "Exercise real mixed backend failures, retained sources and healthy siblings together."
    )]
    async fn stale_sibling_cannot_hide_end<S>(config: S::ChunkConfig)
    where
        S: ChunkSerializer<Data = ChunkData, WriteBackend = PathBuf>,
        S::ChunkConfig: Clone + Send + Sync + 'static,
    {
        use crate::chunk::poi_binding::TestRuntime;
        use crate::poi::PoiRegion;
        use pumpkin_data::poi::PoiType;
        use pumpkin_util::math::position::BlockPos;
        for across_regions in [false, true] {
            for explicit_flush in [false, true] {
                let directory = TempDir::new().unwrap();
                let folder = folder(&directory);
                let runtime = TestRuntime::new(folder.poi_folder.clone());
                let writer = ChunkFileManager::<S>::new(config.clone());
                let bx = if across_regions { 32 } else { 1 };
                let cx = if across_regions { 64 } else { 2 };
                let positions = [Vector2::new(0, 0), Vector2::new(bx, 0), Vector2::new(cx, 0)];
                let sources = [
                    bound_home(&runtime, 0),
                    bound_home(&runtime, bx),
                    bound_home(&runtime, cx),
                ];
                let home = sources[0].section.get_block_absolute_y(1, 0, 1).unwrap();
                for (pos, source) in positions.iter().zip(&sources) {
                    runtime.chunks.insert(*pos, source.clone());
                }
                let submitted = || {
                    positions
                        .iter()
                        .copied()
                        .zip(sources.iter().cloned())
                        .collect()
                };
                writer.save_chunks(&folder, submitted()).await.unwrap();
                let a_path = folder.region_folder.join(S::get_chunk_key(&positions[0]));
                let b_path = folder.region_folder.join(S::get_chunk_key(&positions[1]));
                let poi_a = folder.poi_folder.join("r.0.0.mca");
                let poi_b = folder.poi_folder.join(format!("r.{}.0.mca", bx >> 5));
                {
                    let mut storage = runtime.runtime.storage.lock().unwrap();
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
                }
                let poi_a_before = std::fs::read(&poi_a).unwrap();
                let poi_b_before = std::fs::read(&poi_b).unwrap();
                if explicit_flush {
                    writer.watch_chunks(&folder, &positions).await;
                }
                sources[0].mark_dirty(true);
                sources[1].set_custom_data("r28", "refused", pumpkin_nbt::tag::NbtTag::End);
                sources[1].set_block_absolute_y(1, 0, 1, Block::BELL.default_state.id);
                sources[2].set_block_absolute_y(1, 0, 1, Block::BELL.default_state.id);
                // For explicit flush, admit real failed sources first. A stays
                // pending after this first stale attempt; C has actual cached bytes.
                let (entered, resume) = runtime.runtime.pause_capture_admission(&sources[0]);
                let mut saving = Box::pin(writer.save_chunks(&folder, submitted()));
                tokio::select! {
                    result = &mut saving => panic!("capture bypassed admission gate: {result:?}"),
                    result = tokio::time::timeout(std::time::Duration::from_secs(5), entered) => result.unwrap().unwrap(),
                }
                sources[0].set_block_absolute_y(2, 0, 1, Block::STONE.default_state.id);
                resume.send(()).unwrap();
                assert!(matches!(
                    saving.await,
                    Err(ChunkWritingError::ChunkSerializingError(_))
                ));
                if explicit_flush {
                    let (entered, resume) = runtime.runtime.pause_capture_admission(&sources[0]);
                    let mut flushing = Box::pin(writer.flush());
                    tokio::select! {
                        result = &mut flushing => panic!("flush bypassed admission gate: {result:?}"),
                        result = tokio::time::timeout(std::time::Duration::from_secs(5), entered) => result.unwrap().unwrap(),
                    }
                    sources[0].set_block_absolute_y(2, 0, 1, Block::DIRT.default_state.id);
                    resume.send(()).unwrap();
                    assert!(matches!(
                        flushing.await,
                        Err(ChunkWritingError::ChunkSerializingError(_))
                    ));
                }
                for (pos, source, path) in [
                    (positions[0], &sources[0], &a_path),
                    (positions[1], &sources[1], &b_path),
                ] {
                    assert!(source.is_dirty());
                    let loader = writer.get_loader(path).await;
                    assert!(Arc::ptr_eq(
                        loader.unapplied.lock().await.get(&(pos.x, pos.y)).unwrap(),
                        source
                    ));
                }
                assert_eq!(saved_block::<S>(&folder, config.clone(), 0).await, home);
                assert_eq!(saved_block::<S>(&folder, config.clone(), bx).await, home);
                if across_regions {
                    assert_eq!(std::fs::read(&poi_a).unwrap(), poi_a_before);
                    assert_eq!(std::fs::read(&poi_b).unwrap(), poi_b_before);
                }
                for x in [0, bx] {
                    let poi =
                        PoiRegion::load(&folder.poi_folder.join(format!("r.{}.0.mca", x >> 5)))
                            .unwrap();
                    let entry = poi
                        .get_all()
                        .into_iter()
                        .find(|entry| entry.pos() == BlockPos::new(x * 16 + 1, 0, 1))
                        .unwrap();
                    assert_eq!(entry.poi_type, PoiType::Home.identifier());
                    assert_eq!(entry.free_tickets, 0);
                }
                assert_eq!(
                    saved_block::<S>(&folder, config.clone(), cx).await,
                    Block::BELL.default_state.id
                );
                let healthy =
                    PoiRegion::load(&folder.poi_folder.join(format!("r.{}.0.mca", cx >> 5)))
                        .unwrap();
                assert_eq!(
                    healthy
                        .get_all()
                        .into_iter()
                        .find(|entry| entry.pos() == BlockPos::new(cx * 16 + 1, 0, 1))
                        .unwrap()
                        .free_tickets,
                    32
                );
                sources[1].set_custom_data("r28", "refused", pumpkin_nbt::tag::NbtTag::Int(1));
                writer.flush().await.unwrap();
                assert_eq!(
                    saved_block::<S>(&folder, config.clone(), bx).await,
                    Block::BELL.default_state.id
                );
            }
        }
    }

    #[tokio::test]
    async fn all_formats_mixed_stale_and_end_keep_hard_error_and_write_healthy_regions() {
        Box::pin(stale_sibling_cannot_hide_end::<AnvilChunkFile<ChunkData>>(
            pumpkin_config::chunk::AnvilChunkConfig::default(),
        ))
        .await;
        Box::pin(stale_sibling_cannot_hide_end::<
            crate::chunk::format::linear::LinearV2File<ChunkData>,
        >(()))
        .await;
        Box::pin(stale_sibling_cannot_hide_end::<
            crate::chunk::format::pump::PumpFile<ChunkData>,
        >(()))
        .await;
    }

    #[allow(
        clippy::too_many_lines,
        reason = "Keep actual completion refusal, old POI bytes and exact receipt retry together."
    )]
    async fn stale_completion_cannot_hide_poi_io<S>(config: S::ChunkConfig)
    where
        S: ChunkSerializer<Data = ChunkData, WriteBackend = PathBuf>,
        S::ChunkConfig: Clone + Send + Sync + 'static,
    {
        use crate::chunk::poi_binding::TestRuntime;
        use crate::poi::PoiRegion;
        use pumpkin_data::poi::PoiType;
        use pumpkin_util::math::position::BlockPos;
        for across_regions in [false, true] {
            for admission_stale in [false, true] {
                let directory = TempDir::new().unwrap();
                let folder = folder(&directory);
                let runtime = TestRuntime::new(folder.poi_folder.clone());
                let writer = ChunkFileManager::<S>::new(config.clone());
                let bx = if across_regions { 32 } else { 1 };
                let positions = [Vector2::new(0, 0), Vector2::new(bx, 0), Vector2::new(64, 0)];
                let sources = [
                    bound_home(&runtime, 0),
                    bound_home(&runtime, bx),
                    bound_home(&runtime, 64),
                ];
                for (pos, source) in positions.iter().zip(&sources) {
                    runtime.chunks.insert(*pos, source.clone());
                }
                let submitted = || {
                    positions
                        .iter()
                        .copied()
                        .zip(sources.iter().cloned())
                        .collect()
                };
                writer.save_chunks(&folder, submitted()).await.unwrap();
                {
                    let mut storage = runtime.runtime.storage.lock().unwrap();
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
                }
                writer.watch_chunks(&folder, &positions).await;
                sources[0].set_block_absolute_y(2, 0, 1, Block::STONE.default_state.id);
                sources[1].set_block_absolute_y(1, 0, 1, Block::BELL.default_state.id);
                sources[2].set_block_absolute_y(1, 0, 1, Block::BELL.default_state.id);
                let home = sources[0].section.get_block_absolute_y(1, 0, 1).unwrap();
                if admission_stale {
                    let (entered, resume) = runtime.runtime.pause_capture_admission(&sources[0]);
                    let mut saving = Box::pin(writer.save_chunks(&folder, submitted()));
                    tokio::select! {
                        result = &mut saving => panic!("capture bypassed admission gate: {result:?}"),
                        result = tokio::time::timeout(std::time::Duration::from_secs(5), entered) => result.unwrap().unwrap(),
                    }
                    sources[0].set_blocks_batch([(1, 0, 1, BlockStateId::AIR), (1, 0, 1, home)]);
                    resume.send(()).unwrap();
                    assert!(matches!(
                        saving.await,
                        Err(ChunkWritingError::StaleSaveCapture)
                    ));
                } else {
                    writer.save_chunks(&folder, submitted()).await.unwrap();
                    sources[0].set_blocks_batch([(1, 0, 1, BlockStateId::AIR), (1, 0, 1, home)]);
                }
                // Real rename refusal at the real POI destination. Preserve the
                // original file beside it and restore it before the retained retry.
                let blocked = folder.poi_folder.join(format!("r.{}.0.mca", bx >> 5));
                let backup = blocked.with_extension("original");
                let original = std::fs::read(&blocked).unwrap();
                std::fs::rename(&blocked, &backup).unwrap();
                std::fs::create_dir(&blocked).unwrap();
                let result = if admission_stale {
                    let (entered, resume) = runtime.runtime.pause_capture_admission(&sources[0]);
                    let mut flushing = Box::pin(writer.flush());
                    tokio::select! {
                        result = &mut flushing => panic!("flush bypassed admission gate: {result:?}"),
                        result = tokio::time::timeout(std::time::Duration::from_secs(5), entered) => result.unwrap().unwrap(),
                    }
                    sources[0].set_block_absolute_y(2, 0, 1, Block::DIRT.default_state.id);
                    resume.send(()).unwrap();
                    flushing.await
                } else {
                    writer.flush().await
                };
                // Covers stale apply_pending + hard flush_serialized as well as
                // stale/hard siblings inside complete_poi_batch and region join_all.
                assert!(matches!(result, Err(ChunkWritingError::IoError(_))));
                assert_eq!(std::fs::read(&backup).unwrap(), original);
                assert!(blocked.is_dir());
                assert!(sources[0].is_dirty());
                let a_loader = writer
                    .get_loader(&folder.region_folder.join(S::get_chunk_key(&positions[0])))
                    .await;
                assert!(Arc::ptr_eq(
                    a_loader.unapplied.lock().await.get(&(0, 0)).unwrap(),
                    &sources[0]
                ));
                let b_loader = writer
                    .get_loader(&folder.region_folder.join(S::get_chunk_key(&positions[1])))
                    .await;
                let cached = b_loader.get().await.unwrap();
                let receipts = cached.read().await.receipts.clone();
                assert!(Arc::ptr_eq(
                    &receipts.lock().await.get(&(bx, 0)).unwrap().0,
                    &sources[1]
                ));
                let old = PoiRegion::load(&backup).unwrap();
                assert_eq!(
                    old.get_all()
                        .into_iter()
                        .find(|entry| entry.pos() == BlockPos::new(bx * 16 + 1, 0, 1))
                        .unwrap()
                        .free_tickets,
                    0
                );
                assert_eq!(
                    saved_block::<S>(&folder, config.clone(), 64).await,
                    Block::BELL.default_state.id
                );
                assert_eq!(
                    PoiRegion::load(&folder.poi_folder.join("r.2.0.mca"))
                        .unwrap()
                        .get_all()[0]
                        .free_tickets,
                    32
                );
                std::fs::remove_dir(&blocked).unwrap();
                std::fs::rename(&backup, &blocked).unwrap();
                writer.flush().await.unwrap();
                let poi = PoiRegion::load(&folder.poi_folder.join("r.0.0.mca")).unwrap();
                assert_eq!(
                    poi.get_all()
                        .into_iter()
                        .find(|entry| entry.pos() == BlockPos::new(1, 0, 1))
                        .unwrap()
                        .free_tickets,
                    1
                );
                assert_eq!(
                    saved_block::<S>(&folder, config.clone(), bx).await,
                    Block::BELL.default_state.id
                );
            }
        }
    }

    #[tokio::test]
    async fn all_formats_mixed_stale_and_poi_io_keep_hard_error_and_complete_healthy_region() {
        Box::pin(stale_completion_cannot_hide_poi_io::<
            AnvilChunkFile<ChunkData>,
        >(pumpkin_config::chunk::AnvilChunkConfig::default()))
        .await;
        Box::pin(stale_completion_cannot_hide_poi_io::<
            crate::chunk::format::linear::LinearV2File<ChunkData>,
        >(()))
        .await;
        Box::pin(stale_completion_cannot_hide_poi_io::<
            crate::chunk::format::pump::PumpFile<ChunkData>,
        >(()))
        .await;
    }

    #[tokio::test]
    #[allow(
        clippy::too_many_lines,
        reason = "Exercise actual Anvil compression refusal mixed with stale and healthy receipts."
    )]
    async fn anvil_mixed_stale_and_compression_keep_hard_error_and_write_healthy_sibling() {
        use crate::chunk::poi_binding::TestRuntime;
        use crate::poi::PoiRegion;
        for across_regions in [false, true] {
            let directory = TempDir::new().unwrap();
            let folder = folder(&directory);
            let runtime = TestRuntime::new(folder.poi_folder.clone());
            let mut config = pumpkin_config::chunk::AnvilChunkConfig::default();
            config.compression.algorithm = pumpkin_config::chunk::Compression::LZ4;
            config.compression.level = 0;
            let mut writer = ChunkFileManager::<AnvilChunkFile<ChunkData>>::new(config.clone());
            let bx = if across_regions { 32 } else { 1 };
            let cx = if across_regions { 64 } else { 2 };
            let positions = [Vector2::new(0, 0), Vector2::new(bx, 0), Vector2::new(cx, 0)];
            let sources = [
                bound_home(&runtime, 0),
                bound_home(&runtime, bx),
                bound_home(&runtime, cx),
            ];
            let home = sources[0].section.get_block_absolute_y(1, 0, 1).unwrap();
            for (pos, source) in positions.iter().zip(&sources) {
                runtime.chunks.insert(*pos, source.clone());
            }
            writer
                .save_chunks(
                    &folder,
                    positions
                        .iter()
                        .copied()
                        .zip(sources.iter().cloned())
                        .collect(),
                )
                .await
                .unwrap();
            writer.watch_chunks(&folder, &positions).await;
            sources[0].set_block_absolute_y(2, 0, 1, Block::STONE.default_state.id);
            sources[2].set_block_absolute_y(1, 0, 1, Block::BELL.default_state.id);
            writer
                .save_chunks(
                    &folder,
                    vec![
                        (positions[0], sources[0].clone()),
                        (positions[2], sources[2].clone()),
                    ],
                )
                .await
                .unwrap();
            sources[0].set_blocks_batch([(1, 0, 1, BlockStateId::AIR), (1, 0, 1, home)]);
            sources[1].set_block_absolute_y(1, 0, 1, Block::BELL.default_state.id);
            writer.chunk_config.compression.level = 16;
            assert!(matches!(
                writer
                    .save_chunks(&folder, vec![(positions[1], sources[1].clone())])
                    .await,
                Err(ChunkWritingError::Compression(_))
            ));
            assert!(matches!(
                writer.flush().await,
                Err(ChunkWritingError::Compression(_))
            ));
            assert!(sources[0].is_dirty());
            assert!(sources[1].is_dirty());
            assert_eq!(
                saved_block::<AnvilChunkFile<ChunkData>>(&folder, config.clone(), bx).await,
                home
            );
            assert_eq!(
                saved_block::<AnvilChunkFile<ChunkData>>(&folder, config.clone(), cx).await,
                Block::BELL.default_state.id
            );
            let poi =
                PoiRegion::load(&folder.poi_folder.join(format!("r.{}.0.mca", cx >> 5))).unwrap();
            assert!(
                poi.get_all()
                    .iter()
                    .any(|entry| entry.pos().x == cx * 16 + 1 && entry.free_tickets == 32)
            );
            writer.chunk_config.compression.level = 0;
            writer.flush().await.unwrap();
            assert_eq!(
                saved_block::<AnvilChunkFile<ChunkData>>(&folder, config, bx).await,
                Block::BELL.default_state.id
            );
        }
    }
}
