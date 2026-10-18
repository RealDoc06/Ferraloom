//! ServerChunkCache's synchronous caller policy. The runtime driver owns worker
//! progress; it must publish canonical state before completing an acquisition.
use super::{Level, LevelError, chunk_holder::ChunkStatus};
use ferraloom_util::ChunkPos;
use std::cell::Cell;

/// Port of ChunkLevel#byStatus for the native 26.3 generation pyramid.
pub const fn ticket_level(status: ChunkStatus) -> i32 {
    match status {
        ChunkStatus::Empty | ChunkStatus::StructureStarts => 44,
        ChunkStatus::StructureReferences | ChunkStatus::Biomes => 36,
        ChunkStatus::Terrain => 35,
        ChunkStatus::Features | ChunkStatus::InitializeLight => 34,
        ChunkStatus::Light | ChunkStatus::Spawn | ChunkStatus::Full => 33,
    }
}

/// Stable identity of a published ChunkAccess, distinct from its future. The
/// publisher pins it until the source cache and outstanding borrows are gone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkReference {
    pub pos: ChunkPos,
    pub generation: u64,
    pub kind: ChunkKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChunkKind {
    Full,
    Proto,
}

/// A status future, including the holder incarnation that owns the request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcquireHandle {
    pub pos: ChunkPos,
    pub status: ChunkStatus,
    pub request_id: u64,
    pub holder_epoch: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcquireResult {
    Pending,
    Complete(Option<ChunkReference>),
}

/// One MainThreadExecutor#pollTask step. Distance-manager changes invalidate
/// the four-entry source cache before the next task or nested world call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcquireProgress {
    DistanceUpdates,
    Task,
    Idle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcquireFailure {
    DriverUnavailable,
    MissingHolder,
    MissingResult,
    InvalidHandle,
    Cancelled,
    Io,
    Generation,
    Lighting,
    Publication,
}

/// Cold runtime seam for ServerChunkCache#getChunk/getChunkFutureMainThread.
///
/// Methods receive no mutable Level. Publication uses disjoint shared stores;
/// no method may cast a suspended Level read into a mutable borrow. Polling must
/// keep reentrant completions reachable, never evict, and never advance a tick.
/// Worker failure/cancellation returns an error instead of an endless Pending.
pub trait AcquireDriver: Send {
    fn is_owner_thread(&self) -> bool;
    fn visible_ticket_level(&self, pos: ChunkPos) -> Option<i32>;
    /// Apply TicketStorage#add_unknown; it refreshes the canonical signed timer.
    fn add_unknown_ticket(&self, pos: ChunkPos, status: ChunkStatus) -> Result<(), LevelError>;
    /// Propagate tickets, promote the visible map and dispatch generation. True
    /// means native runDistanceManagerUpdates would invalidate the source cache.
    fn run_distance_manager_updates(&self, level: &Level) -> Result<bool, LevelError>;
    /// Called only with an eligible visible holder. Reuse its existing request
    /// identity or result; do not add a second ticket or wait for ticking status.
    fn request_status(
        &self,
        level: &Level,
        pos: ChunkPos,
        status: ChunkStatus,
    ) -> Result<AcquireHandle, LevelError>;
    fn acquisition_result(&self, handle: AcquireHandle) -> Result<AcquireResult, LevelError>;
    fn poll_acquisition_step(&self, level: &Level) -> Result<AcquireProgress, LevelError>;
    /// Wakeable wait, only after an idle poll and a second completion check.
    fn wait_for_progress(&self) -> Result<(), LevelError>;
    /// GenerationChunkHolder#getChunkIfPresent checks highestAllowedStatus,
    /// which can differ from ticket eligibility before future updates run.
    fn chunk_if_present(&self, pos: ChunkPos, status: ChunkStatus) -> Option<ChunkReference>;
    /// GenerationChunkHolder#getChunkIfPresentUnchecked: completed future only.
    fn chunk_if_present_unchecked(
        &self,
        pos: ChunkPos,
        status: ChunkStatus,
    ) -> Option<ChunkReference>;
    /// Off-owner getChunk queues the entire owner-side call, including its cache
    /// and ticket semantics, then joins. It must not mutate this caller's cache.
    fn acquire_on_owner(
        &self,
        pos: ChunkPos,
        status: ChunkStatus,
        load_or_generate: bool,
    ) -> Result<Option<ChunkReference>, LevelError>;
}

#[derive(Clone, Copy)]
struct CacheEntry {
    pos: ChunkPos,
    status: ChunkStatus,
    chunk: Option<ChunkReference>,
}

/// Port of ServerChunkCache's four-entry cache and blocking caller contracts.
/// The cache contains stable identities, never references into a growing map.
#[derive(Default)]
pub struct ChunkAcquisition {
    cache: Cell<[Option<CacheEntry>; 4]>,
}

impl ChunkAcquisition {
    /// Native constructor/end-of-ServerChunkCache#tick invalidation. Also call
    /// when distance propagation or visible-map promotion reports a change.
    pub fn clear_cache(&self) {
        self.cache.set([None; 4]);
    }

    fn cache(&self, pos: ChunkPos, status: ChunkStatus, chunk: Option<ChunkReference>) {
        let mut cache = self.cache.get();
        cache.copy_within(0..3, 1);
        cache[0] = Some(CacheEntry { pos, status, chunk });
        self.cache.set(cache);
    }

    fn eligible(driver: &dyn AcquireDriver, pos: ChunkPos, status: ChunkStatus) -> bool {
        driver
            .visible_ticket_level(pos)
            .is_some_and(|level| level <= ticket_level(status))
    }

    /// Port of ServerChunkCache#getChunk. False avoids creating a loading
    /// ticket, but an eligible holder can still require managed blocking.
    pub fn get_chunk(
        &self,
        driver: &dyn AcquireDriver,
        level: &Level,
        pos: ChunkPos,
        status: ChunkStatus,
        load_or_generate: bool,
    ) -> Result<Option<ChunkReference>, LevelError> {
        if !driver.is_owner_thread() {
            let chunk = driver.acquire_on_owner(pos, status, load_or_generate)?;
            if load_or_generate && chunk.is_none() {
                return Err(LevelError::ChunkAcquisition(
                    pos,
                    status,
                    AcquireFailure::MissingResult,
                ));
            }
            return Ok(chunk);
        }
        for entry in self.cache.get().into_iter().flatten() {
            if entry.pos == pos
                && entry.status == status
                && (entry.chunk.is_some() || !load_or_generate)
            {
                return Ok(entry.chunk);
            }
        }
        if load_or_generate {
            driver.add_unknown_ticket(pos, status)?;
            if !Self::eligible(driver, pos, status) {
                if driver.run_distance_manager_updates(level)? {
                    self.clear_cache();
                }
                if !Self::eligible(driver, pos, status) {
                    return Err(LevelError::ChunkAcquisition(
                        pos,
                        status,
                        AcquireFailure::MissingHolder,
                    ));
                }
            }
        }
        if !Self::eligible(driver, pos, status) {
            self.cache(pos, status, None);
            return Ok(None);
        }
        let handle = driver.request_status(level, pos, status)?;
        if handle.pos != pos || handle.status != status {
            return Err(LevelError::ChunkAcquisition(
                pos,
                status,
                AcquireFailure::InvalidHandle,
            ));
        }
        let chunk = loop {
            if let AcquireResult::Complete(chunk) = driver.acquisition_result(handle)? {
                break chunk;
            }
            match driver.poll_acquisition_step(level)? {
                AcquireProgress::DistanceUpdates => self.clear_cache(),
                AcquireProgress::Task => {}
                AcquireProgress::Idle => {
                    if let AcquireResult::Complete(chunk) = driver.acquisition_result(handle)? {
                        break chunk;
                    }
                    driver.wait_for_progress()?;
                }
            }
        };
        if chunk.is_some_and(|chunk| chunk.pos != pos) {
            return Err(LevelError::ChunkAcquisition(
                pos,
                status,
                AcquireFailure::InvalidHandle,
            ));
        }
        if chunk.is_none() && load_or_generate {
            return Err(LevelError::ChunkAcquisition(
                pos,
                status,
                AcquireFailure::MissingResult,
            ));
        }
        self.cache(pos, status, chunk);
        Ok(chunk)
    }

    /// Port of ServerChunkCache#getChunkNow: no ticket, generation or wait.
    pub fn get_chunk_now(
        &self,
        driver: &dyn AcquireDriver,
        pos: ChunkPos,
    ) -> Option<ChunkReference> {
        if !driver.is_owner_thread() {
            return None;
        }
        for entry in self.cache.get().into_iter().flatten() {
            if entry.pos == pos && entry.status == ChunkStatus::Full {
                return entry.chunk.filter(|chunk| chunk.kind == ChunkKind::Full);
            }
        }
        let chunk = driver.chunk_if_present(pos, ChunkStatus::Full)?;
        self.cache(pos, ChunkStatus::Full, Some(chunk));
        (chunk.kind == ChunkKind::Full).then_some(chunk)
    }

    /// Port of ServerChunkCache#hasChunk: ticket eligibility, not readiness.
    pub fn has_chunk(&self, driver: &dyn AcquireDriver, pos: ChunkPos) -> bool {
        Self::eligible(driver, pos, ChunkStatus::Full)
    }

    /// Port of ServerChunkCache#getChunkForLighting. INITIALIZE_LIGHT's parent
    /// is FEATURES; neither its ticket eligibility nor future completion is driven.
    pub fn get_chunk_for_lighting(
        &self,
        driver: &dyn AcquireDriver,
        pos: ChunkPos,
    ) -> Option<ChunkReference> {
        driver.chunk_if_present_unchecked(pos, ChunkStatus::Features)
    }
}

#[cfg(test)]
mod tests;
