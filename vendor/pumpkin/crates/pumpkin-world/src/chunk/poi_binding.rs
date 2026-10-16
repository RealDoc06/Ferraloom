//! Runtime ownership uses Level's existing publication and IO fences. There is
//! no second publication registry: a binding lives exactly as long as its chunk.

use std::sync::{Arc, Mutex, MutexGuard, RwLock, Weak};

use dashmap::{DashMap, mapref::one::Ref};

use super::{ChunkData, ChunkSerializingError, PoiReconciliationStamp};
use crate::{
    chunk_system::ChunkPos,
    level::SyncChunk,
    poi::{PoiReconcileError, PoiStorage},
};

pub struct PoiRuntime {
    pub(crate) storage: Arc<Mutex<PoiStorage>>,
    pub(crate) lifecycle: Arc<RwLock<()>>,
    pub(crate) chunks: Weak<DashMap<ChunkPos, SyncChunk>>,
}

#[derive(Clone, Copy)]
pub(super) enum PoiCapturePurpose {
    Save,
    Unload,
}

/// Both managed paths cross the same encoder before publishing captured POI.
/// A save retains this guard through its flush; preparation drops it without IO.
pub(super) struct EncodedPoiSnapshot<'a> {
    pub(super) bytes: bytes::Bytes,
    pub(super) storage: MutexGuard<'a, PoiStorage>,
    pub(super) stamp: Option<PoiReconciliationStamp>,
}

impl ChunkData {
    /// Bind before publication or handing a managed snapshot to the IO worker.
    /// Acknowledged tracker state can never be transferred to another runtime.
    pub(crate) fn bind_poi(&self, runtime: &Arc<PoiRuntime>) -> Result<(), PoiReconcileError> {
        let bound = self.section.poi_binding.get_or_init(|| runtime.clone());
        if Arc::ptr_eq(bound, runtime) {
            Ok(())
        } else {
            Err(PoiReconcileError::WrongStorage)
        }
    }

    pub(crate) fn poi_runtime(&self) -> Result<&Arc<PoiRuntime>, PoiReconcileError> {
        self.section
            .poi_binding
            .get()
            .ok_or(PoiReconcileError::UnboundChunk)
    }

    pub(crate) fn has_poi_binding(&self) -> bool {
        self.section.poi_binding.get().is_some()
    }

    /// Caller holds the lifecycle read side. A present publication must match
    /// exactly. An absent entry is permitted only for scheduler-owned generated,
    /// Proto or retired snapshots: its existing occupied/queued-IO fences prevent
    /// a replacement from overtaking that work. Detached external saves are not
    /// a supported runtime ownership path.
    pub(crate) fn check_poi_publication<'a>(
        &self,
        chunks: &'a DashMap<ChunkPos, SyncChunk>,
    ) -> Result<Option<Ref<'a, ChunkPos, SyncChunk>>, PoiReconcileError> {
        let current = chunks.get(&ChunkPos::new(self.x, self.z));
        if current
            .as_ref()
            .is_some_and(|entry| !std::ptr::eq(entry.value().as_ref(), self))
        {
            return Err(PoiReconcileError::ObsoletePublication);
        }
        Ok(current)
    }

    /// Encode the exact current FULL Arc before publishing its POI in memory.
    /// Preload may read disk, but preparation never writes a POI file. All guards
    /// are released before this stamp returns; actual saving captures again.
    pub(crate) fn prepare_poi(&self) -> Result<PoiReconciliationStamp, PoiReconcileError> {
        let capture = self
            .capture_poi_snapshot(PoiCapturePurpose::Unload)
            .map_err(|error| match error {
                ChunkSerializingError::ErrorSerializingChunk(error) => {
                    PoiReconcileError::TerrainEncoding(error)
                }
                ChunkSerializingError::Poi(error) => error,
                ChunkSerializingError::PoiIo(error) => PoiReconcileError::RegionUnavailable(error),
            })?;
        let stamp = capture.stamp.ok_or(PoiReconcileError::IncompleteTerrain)?;
        // An initial scan can repair missing/stale POI even when loaded terrain
        // itself was clean. Retirement must queue the bound actual-save hook.
        self.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(stamp)
    }
}

#[cfg(test)]
pub struct TestRuntime {
    pub(crate) runtime: Arc<PoiRuntime>,
    pub(crate) chunks: Arc<DashMap<ChunkPos, SyncChunk>>,
}

#[cfg(test)]
impl TestRuntime {
    pub(crate) fn new(folder: std::path::PathBuf) -> Self {
        let chunks = Arc::new(DashMap::new());
        let runtime = Arc::new(PoiRuntime {
            storage: Arc::new(Mutex::new(PoiStorage::new(folder))),
            lifecycle: Arc::new(RwLock::new(())),
            chunks: Arc::downgrade(&chunks),
        });
        Self { runtime, chunks }
    }
}
