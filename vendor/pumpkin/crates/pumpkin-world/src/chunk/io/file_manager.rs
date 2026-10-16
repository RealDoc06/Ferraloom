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

use super::{ChunkSerializer, FileIO, LoadedData, run_blocking};

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
/// map and region locks.
pub struct ChunkFileManager<S: ChunkSerializer<WriteBackend = PathBuf>> {
    file_locks: RwLock<BTreeMap<PathBuf, Arc<ChunkSerializerLazyLoader<S>>>>,
    watchers: RwLock<BTreeMap<PathBuf, usize>>,
    chunk_config: S::ChunkConfig,
}

pub(crate) trait PathFromLevelFolder {
    fn file_path(folder: &LevelFolder, file_name: &str) -> PathBuf;
}

type PendingChunks<T> = BTreeMap<(i32, i32), Arc<T>>;

struct ChunkSerializerLazyLoader<S: ChunkSerializer<WriteBackend = PathBuf>> {
    path: PathBuf,
    // Retain the newest source chunk even when opening the region fails.
    unapplied: Mutex<PendingChunks<S::Data>>,
    /// Initialised at most once; subsequent calls reuse the same Arc.
    internal: OnceCell<Arc<RwLock<CachedRegion<S>>>>,
}

struct CachedRegion<S: ChunkSerializer<WriteBackend = PathBuf>> {
    serializer: S,
    needs_write: bool,
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
        Ok(())
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
            Arc::strong_count(arc) == 1 && arc.try_read().is_ok_and(|cached| !cached.needs_write)
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
                })))
            })
            .await
            .cloned()
    }

    async fn apply_pending(&self, config: &S::ChunkConfig) -> Result<(), ChunkWritingError> {
        let mut pending = self.unapplied.lock().await;
        if pending.is_empty() {
            return Ok(());
        }
        let cached = self.get().await.map_err(|error| {
            ChunkWritingError::IoError(std::io::Error::other(error.to_string()))
        })?;
        let mut region = cached.write().await;
        let mut applied = Vec::new();
        let mut failure = None;
        for (position, chunk) in pending.iter() {
            if !chunk.is_dirty() {
                applied.push(*position);
                continue;
            }
            chunk.mark_dirty(false);
            match region.serializer.update_chunk(chunk.clone(), config).await {
                Ok(()) => {
                    region.needs_write = true;
                    applied.push(*position);
                }
                Err(error) => {
                    chunk.mark_dirty(true);
                    failure.get_or_insert(error);
                }
            }
        }
        for position in applied {
            pending.remove(&position);
        }
        failure.map_or(Ok(()), Err)
    }

    async fn flush_serialized(&self) -> Result<(), ChunkWritingError> {
        if let Some(cached) = self.internal.get() {
            cached.write().await.flush(&self.path).await?;
        }
        Ok(())
    }

    async fn flush(&self, config: &S::ChunkConfig) -> Result<(), ChunkWritingError> {
        let applied = self.apply_pending(config).await;
        let written = self.flush_serialized().await;
        applied.and(written)
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
                    let mut retained = Vec::new();
                    let mut remaining = Vec::new();
                    for position in chunks {
                        if let Some(chunk) = pending.get(&(position.x, position.y)) {
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
                    applied.and(written)?;
                    drop(loader);
                    self.maybe_evict(&path).await;
                }

                Ok(())
            });

        // Collect all region results; surface the first error encountered.
        let results: Vec<Result<(), ChunkWritingError>> = join_all(tasks).await;
        results.into_iter().find(Result::is_err).unwrap_or(Ok(()))
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
                let _guard = serializer_arc.write().await;
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
        results.into_iter().find(Result::is_err).unwrap_or(Ok(()))
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
}
