use std::{error, sync::Arc};

use bytes::Bytes;
use pumpkin_util::math::vector2::Vector2;

use super::{ChunkReadingError, ChunkWritingError};
use crate::level::LevelFolder;

pub mod file_manager;

pub(crate) async fn run_blocking<T, F>(task: F) -> Result<T, tokio::task::JoinError>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tokio::task::spawn_blocking(task).await
}

/// The result of loading a chunk data.
///
/// It can be the data loaded successfully, the data not found or an error
/// with the chunk coordinates and the error that occurred.
pub enum LoadedData<D: Send, Err: error::Error> {
    /// The chunk data was loaded successfully
    Loaded(D),

    /// The chunk data was not found
    Missing(Vector2<i32>),

    /// An error occurred while loading the chunk data
    Error((Vector2<i32>, Err)),
}

impl<D: Send, E: error::Error> LoadedData<D, E> {
    pub fn map_loaded<D2: Send>(self, map: impl FnOnce(D) -> D2) -> LoadedData<D2, E> {
        match self {
            Self::Loaded(data) => LoadedData::Loaded(map(data)),
            Self::Missing(pos) => LoadedData::Missing(pos),
            Self::Error(err) => LoadedData::Error(err),
        }
    }
}

pub trait Dirtiable {
    fn is_dirty(&self) -> bool;
    fn mark_dirty(&self, flag: bool);
}

/// Trait to handle the IO of chunks
/// for loading and saving chunks data
/// can be implemented for different types of IO
/// or with different optimizations
///
/// The `R` type is the type of the data that will be loaded/saved
/// like `ChunkData` or `EntityData`
pub trait FileIO
where
    Self: Send + Sync,
{
    type Data: Send + Sync + Sized;

    /// Load the chunks data
    fn fetch_chunks<'a>(
        &'a self,
        folder: &'a LevelFolder,
        chunk_coords: &'a [Vector2<i32>],
        stream: tokio::sync::mpsc::Sender<LoadedData<Self::Data, ChunkReadingError>>,
    ) -> impl Future<Output = ()> + Send + 'a;

    /// Persist the chunks data
    fn save_chunks<'a>(
        &'a self,
        folder: &'a LevelFolder,
        chunks_data: Vec<(Vector2<i32>, Self::Data)>,
    ) -> impl Future<Output = Result<(), ChunkWritingError>> + Send + 'a;

    /// Tells the `ChunkIO` that these chunks are currently loaded in memory
    fn watch_chunks<'a>(
        &'a self,
        folder: &'a LevelFolder,
        chunks: &'a [Vector2<i32>],
    ) -> impl Future<Output = ()> + Send + 'a;

    /// Tells the `ChunkIO` that these chunks are no longer loaded in memory
    fn unwatch_chunks<'a>(
        &'a self,
        folder: &'a LevelFolder,
        chunks: &'a [Vector2<i32>],
    ) -> impl Future<Output = ()> + Send + 'a;

    /// Tells the `ChunkIO` that no more chunks are loaded in memory
    fn clear_watched_chunks(&self) -> impl Future<Output = ()> + Send + '_;

    /// Ensure that all ongoing operations are finished
    fn block_and_await_ongoing_tasks(&self) -> impl Future<Output = ()> + Send + '_;

    /// Write cached regions to disk, including regions that still have watchers.
    fn flush(&self) -> impl Future<Output = Result<(), ChunkWritingError>> + Send + '_;
}

pub use super::poi_binding::EncodedPoiCommit;

/// Owned chunk encoding, private until the backend accepts it after compression.
#[derive(Debug)]
pub struct EncodedChunk {
    pub bytes: Bytes,
    pub poi: Option<EncodedPoiCommit>,
}

impl EncodedChunk {
    pub(crate) fn validate(&self) -> Result<(), ChunkWritingError> {
        if let Some(poi) = &self.poi {
            poi.validate()?;
        }
        Ok(())
    }
}

/// File completion includes the containing directory on platforms supporting it.
pub(crate) async fn sync_parent(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        tokio::fs::File::open(parent).await?.sync_all().await
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

/// Trait to serialize and deserialize the chunk data to and from bytes.
///
/// The `Data` type is the type of the data that will be updated or serialized/deserialized
/// like `ChunkData` or `EntityData`
pub trait ChunkSerializer: Send + Sync + Default + 'static {
    type Data: Send + Sync + Sized + Dirtiable + 'static;
    type WriteBackend;

    type ChunkConfig;

    /// Get the key for the chunk (like the file name)
    fn get_chunk_key(chunk: &Vector2<i32>) -> String;

    fn should_write(&self, is_watched: bool) -> bool;

    /// Serialize the data to bytes.
    fn write(
        &self,
        backend: &Self::WriteBackend,
    ) -> impl Future<Output = Result<(), std::io::Error>> + Send;

    /// Create a new instance from bytes
    fn read(r: Bytes) -> Result<Self, ChunkReadingError>;

    /// Replace cached bytes only after encoding/compression succeeds. The returned
    /// receipt belongs to those bytes and must complete only after `write` succeeds.
    fn update_chunk(
        &mut self,
        chunk_data: Arc<Self::Data>,
        chunk_config: &Self::ChunkConfig,
    ) -> impl Future<Output = Result<Option<EncodedPoiCommit>, ChunkWritingError>> + Send;

    /// Get the chunks data from the serializer
    fn get_chunks(
        &self,
        chunks: Vec<Vector2<i32>>,
        stream: tokio::sync::mpsc::Sender<LoadedData<Self::Data, ChunkReadingError>>,
    ) -> impl Future<Output = ()> + Send;
}
