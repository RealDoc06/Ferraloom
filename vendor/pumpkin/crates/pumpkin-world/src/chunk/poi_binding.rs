//! Runtime ownership uses Level's existing publication and IO fences. There is
//! no second publication registry: a binding lives exactly as long as its chunk.

use std::sync::{Arc, Mutex, RwLock, Weak};

use dashmap::{DashMap, mapref::one::Ref};

use super::{ChunkData, ChunkSerializingError, ChunkWritingError, PoiCaptureStamp};
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

/// Private owned encoding. No storage or terrain guard escapes capture.
pub(super) struct EncodedPoiSnapshot {
    pub(super) bytes: bytes::Bytes,
    pub(super) stamp: PoiCaptureStamp,
}

/// Paired with the actual cached backend bytes. It retains the exact source until
/// both terrain and POI completion, including failed IO and cancelled flushes.
pub struct EncodedPoiCommit {
    source: Arc<ChunkData>,
    stamp: PoiCaptureStamp,
    state: CommitState,
}

#[derive(Debug, PartialEq, Eq)]
enum CommitState {
    Captured,
    Applied,
    Complete,
    Recapture,
}

impl std::fmt::Debug for EncodedPoiCommit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncodedPoiCommit")
            .field("position", &(self.source.x, self.source.z))
            .field("stamp", &self.stamp)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl EncodedPoiCommit {
    pub(super) const fn new(source: Arc<ChunkData>, stamp: PoiCaptureStamp) -> Self {
        Self {
            source,
            stamp,
            state: CommitState::Captured,
        }
    }

    /// Called after all fallible backend chunk encoding/compression, before the
    /// backend replaces either its cached bytes or their paired receipt.
    pub(crate) fn validate(&self) -> Result<(), ChunkWritingError> {
        self.source
            .validate_poi_capture(&self.stamp)
            .map_err(|error| ChunkWritingError::ChunkSerializingError(error.to_string()))
    }

    pub(crate) fn needs_recapture(&self) -> bool {
        self.state == CommitState::Recapture
    }
    pub(crate) fn is_complete(&self) -> bool {
        self.state == CommitState::Complete
    }

    fn commit(&mut self) -> Result<(), PoiReconcileError> {
        let runtime = self.source.poi_runtime()?;
        let _lifecycle = runtime
            .lifecycle
            .read()
            .map_err(|_| PoiReconcileError::OwnershipPoisoned)?;
        let chunks = runtime
            .chunks
            .upgrade()
            .ok_or(PoiReconcileError::ObsoletePublication)?;
        // An absent publication is permitted for existing occupied/generated or
        // retired queued IO ownership. A different live Arc is ALWAYS refused.
        let _current = self.source.check_poi_publication(&chunks)?;
        let mut storage = runtime
            .storage
            .lock()
            .map_err(|_| PoiReconcileError::OwnershipPoisoned)?;
        let mut loaded = storage.loaded_poi_region(ChunkPos::new(self.source.x, self.source.z))?;
        let terrain = self.source.read_for_poi();
        terrain.validate_capture(&loaded, &self.stamp)?;
        if self.state == CommitState::Captured {
            terrain.commit_capture(&mut loaded, &self.stamp)?;
            self.state = CommitState::Applied;
        }
        Ok(())
    }

    fn refuse(&mut self, error: &PoiReconcileError) {
        self.source
            .dirty
            .store(true, std::sync::atomic::Ordering::Relaxed);
        if matches!(error, PoiReconcileError::StaleSaveCapture) {
            self.state = CommitState::Recapture;
        }
        // Obsolete publications stay retained until a newer cached source
        // replaces them. Never enqueue the old Arc over its new owner.
    }
}

/// Called only after successful terrain-file write. Group receipts by their
/// actual storage/region; apply eligible sources, then write one owned snapshot.
/// All lifecycle/store/terrain guards are released before POI encoding or IO.
pub fn complete_poi_batch<'a>(
    receipts: impl Iterator<Item = &'a mut EncodedPoiCommit>,
) -> Result<(), ChunkWritingError> {
    let mut groups = std::collections::BTreeMap::<_, Vec<_>>::new();
    for receipt in receipts {
        let runtime = receipt
            .source
            .poi_runtime()
            .map_err(|error| ChunkWritingError::ChunkSerializingError(error.to_string()))?;
        groups
            .entry((
                Arc::as_ptr(&runtime.storage) as usize,
                receipt.source.x >> 5,
                receipt.source.z >> 5,
            ))
            .or_default()
            .push(receipt);
    }
    let mut failure = None;
    for (_, group) in groups {
        let mut eligible = Vec::new();
        for receipt in group {
            match receipt.commit() {
                Ok(()) => eligible.push(receipt),
                Err(error) => {
                    receipt.refuse(&error);
                    failure
                        .get_or_insert(ChunkWritingError::ChunkSerializingError(error.to_string()));
                }
            }
        }
        let Some(first) = eligible.first() else {
            continue;
        };
        let runtime = first
            .source
            .poi_runtime()
            .map_err(|error| ChunkWritingError::ChunkSerializingError(error.to_string()))?
            .clone();
        let pos = ChunkPos::new(first.source.x, first.source.z);
        let write = (|| {
            let snapshot = runtime
                .storage
                .lock()
                .map_err(|_| std::io::Error::other(PoiReconcileError::OwnershipPoisoned))?
                .begin_region_save(pos)?;
            if let Some(mut snapshot) = snapshot {
                snapshot.write()?;
                runtime
                    .storage
                    .lock()
                    .map_err(|_| std::io::Error::other(PoiReconcileError::OwnershipPoisoned))?
                    .finish_region_save(&snapshot);
            }
            Ok::<_, std::io::Error>(())
        })();
        if let Err(error) = write {
            failure.get_or_insert(ChunkWritingError::IoError(error));
            continue;
        }
        for receipt in eligible {
            match receipt.source.validate_poi_capture(&receipt.stamp) {
                Ok(()) => receipt.state = CommitState::Complete,
                Err(error) => {
                    receipt.refuse(&error);
                    failure
                        .get_or_insert(ChunkWritingError::ChunkSerializingError(error.to_string()));
                }
            }
        }
    }
    failure.map_or(Ok(()), Err)
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

    /// Revalidate with the same lifecycle -> publication -> store -> terrain
    /// order at post-encoding admission and again at actual file completion.
    pub(crate) fn validate_poi_capture(
        &self,
        stamp: &PoiCaptureStamp,
    ) -> Result<(), PoiReconcileError> {
        let runtime = self.poi_runtime()?;
        let _lifecycle = runtime
            .lifecycle
            .read()
            .map_err(|_| PoiReconcileError::OwnershipPoisoned)?;
        let chunks = runtime
            .chunks
            .upgrade()
            .ok_or(PoiReconcileError::ObsoletePublication)?;
        let _current = self.check_poi_publication(&chunks)?;
        let mut storage = runtime
            .storage
            .lock()
            .map_err(|_| PoiReconcileError::OwnershipPoisoned)?;
        let loaded = storage.loaded_poi_region(ChunkPos::new(self.x, self.z))?;
        self.read_for_poi().validate_capture(&loaded, stamp)
    }

    /// Validate and encode privately. A capture stamp authorizes only the final
    /// exact-Arc/revision retirement fence; `FileManager` retains actual save work.
    pub(crate) fn prepare_poi(&self) -> Result<PoiCaptureStamp, PoiReconcileError> {
        let capture = self
            .capture_poi_snapshot(PoiCapturePurpose::Unload)
            .map_err(|error| match error {
                ChunkSerializingError::ErrorSerializingChunk(error) => {
                    PoiReconcileError::TerrainEncoding(error)
                }
                ChunkSerializingError::Poi(error) => error,
                ChunkSerializingError::PoiIo(error) => PoiReconcileError::RegionUnavailable(error),
            })?;
        // Even clean terrain may need an initial POI repair. Only completion of
        // the actual queued save can release this source, never preparation.
        self.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(capture.stamp)
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
