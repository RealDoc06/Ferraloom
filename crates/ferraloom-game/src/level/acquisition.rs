//! ServerChunkCache's synchronous caller policy. The runtime driver owns worker
//! progress; it must publish canonical state before completing an acquisition.
use super::{Level, LevelError, chunk_holder::ChunkStatus};
use ferraloom_util::ChunkPos;
use std::{any::Any, cell::Cell, sync::Arc};

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

/// Native main-owner operation. Distance propagation is separate from polling
/// so a loading request applies tickets before looking up its status future.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcquireTaskKind {
    DistanceUpdates,
    Poll,
}

/// Owned main-executor task. The driver's borrow ends before this receives
/// mutable Level; callbacks can still reach the installed driver and queues.
pub trait MainThreadAcquireTask: Send {
    fn run(self: Box<Self>, level: &mut Level) -> Result<AcquireProgress, LevelError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcquireFailure {
    DriverUnavailable,
    DriverAlreadyInstalled,
    MissingHolder,
    MissingResult,
    InvalidHandle,
    Cancelled,
    Io,
    Generation,
    Lighting,
    Publication,
    ReentrantFull,
    NotOwner,
}

/// Cold runtime seam for ServerChunkCache#getChunk/getChunkFutureMainThread.
///
/// Driver methods receive shared Level and may return owned mutable tasks.
/// No method may cast a suspended Level read into a mutable borrow. Polling must
/// keep reentrant completions reachable, never evict, and never advance a tick.
/// Worker failure/cancellation returns an error instead of an endless Pending.
pub trait AcquireDriver: Send + Any {
    /// SectionStorage#unpackChunk: an owned synchronous POI-only IO read.
    /// Implementations release runtime guards before waiting on the independent IO worker.
    fn load_poi_chunk(
        &self,
        _pos: ChunkPos,
    ) -> Result<Option<ferraloom_storage::PoiChunk>, LevelError> {
        Err(LevelError::Poi(super::poi::PoiError::MissingStorage))
    }
    /// Port of ChunkMap#updatePlayerStatus's synchronous DistanceManager boundary.
    /// This mutates the existing ticket owner, with no queued publication.
    fn update_player_tracking_status(
        &self,
        _player: crate::entity::EntityRef,
        _section: Option<ferraloom_util::SectionPos>,
    ) -> Result<(), LevelError> {
        Err(LevelError::UnsupportedEntityTrackingStatus)
    }
    /// Cold access to the sole concrete runtime owner. Callers must finish this
    /// borrow and return owned work before invoking a world callback.
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn is_owner_thread(&self) -> bool;
    fn visible_ticket_level(&self, pos: ChunkPos) -> Option<i32>;
    /// Apply TicketStorage#add_unknown; it refreshes the canonical signed timer.
    fn add_unknown_ticket(&self, pos: ChunkPos, status: ChunkStatus) -> Result<(), LevelError>;
    /// Propagate tickets, promote the visible map and dispatch generation. True
    /// means native runDistanceManagerUpdates would invalidate the source cache.
    /// Clear Level's cache at visible-map publication, before any nested work;
    /// the return value also invalidates the outer acquisition's cache.
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
    /// Visible-map changes clear Level's cache before callbacks, not merely
    /// when DistanceUpdates is returned. Mutating ready tasks stay queued for
    /// the next exclusive slot and are never run through this shared borrow.
    fn poll_acquisition_step(&self, level: &Level) -> Result<AcquireProgress, LevelError>;
    /// Select mutable publication before executing a stream. A driver with no
    /// mutable callbacks may use the shared implementation by returning None.
    fn main_thread_acquisition_task(
        &self,
        _level: &Level,
        _kind: AcquireTaskKind,
    ) -> Result<Option<Box<dyn MainThreadAcquireTask>>, LevelError> {
        Ok(None)
    }
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
    /// Read the acquired proto's retained typed structure version. The Arc
    /// outlives backend guards and later nested publications.
    fn proto_chunk_structures(
        &self,
        chunk: ChunkReference,
    ) -> Result<Arc<ferraloom_worldgen::structure::pipeline::ChunkStructures>, LevelError> {
        Err(LevelError::ChunkAcquisition(
            chunk.pos,
            ChunkStatus::StructureStarts,
            AcquireFailure::MissingResult,
        ))
    }
    /// Read an acquired proto's biome data. FULL reads stay concrete on Level.
    fn proto_noise_biome(
        &self,
        _chunk: ChunkReference,
        _quart: [i32; 3],
    ) -> Result<ferraloom_chunk::BiomeId, LevelError> {
        Err(LevelError::UnsupportedWorldgenEnvironment)
    }
    /// LevelReader#getUncachedNoiseBiome's absent-holder branch, using the
    /// canonical generator's biome source and climate sampler.
    fn uncached_noise_biome(
        &self,
        _quart: [i32; 3],
    ) -> Result<ferraloom_chunk::BiomeId, LevelError> {
        Err(LevelError::UnsupportedWorldgenEnvironment)
    }
}

impl Level {
    /// Single-region runtime installation. Dimension bounds are actual level
    /// bounds, not generator noise bounds. No fallback driver is manufactured.
    pub fn install_acquire_driver(
        &mut self,
        driver: Box<dyn AcquireDriver>,
        build_height: std::ops::Range<i32>,
    ) -> Result<(), LevelError> {
        if self.acquire_driver.is_some() {
            return Err(LevelError::ChunkAcquisition(
                ChunkPos::ZERO,
                ChunkStatus::Full,
                AcquireFailure::DriverAlreadyInstalled,
            ));
        }
        if build_height.start >= build_height.end
            || build_height.start % 16 != 0
            || build_height.end % 16 != 0
        {
            return Err(LevelError::OutsideBuildHeight);
        }
        let region = self.region_mut();
        if let Some(chunk) = region
            .chunks
            .values()
            .find(|chunk| chunk.min_y() != build_height.start || chunk.max_y() != build_height.end)
        {
            return Err(LevelError::WrongChunkHeight(chunk.pos()));
        }
        region.min_y = build_height.start;
        region.max_y = build_height.end;
        self.acquisition.clear_cache();
        self.acquire_driver = Some(driver);
        Ok(())
    }

    /// Cold runtime access; no take/restore operation can detach the driver
    /// while a nested acquisition or callback still needs the canonical owner.
    pub fn acquire_driver(&self) -> Option<&dyn AcquireDriver> {
        self.acquire_driver.as_deref()
    }
    pub fn acquire_driver_mut(&mut self) -> Option<&mut (dyn AcquireDriver + 'static)> {
        self.acquire_driver.as_deref_mut()
    }

    /// Port of Level#getChunk's status/creation overload. The handle identifies
    /// the actual published object; only FULL handles resolve in live chunk storage.
    pub fn get_chunk(
        &self,
        pos: ChunkPos,
        status: ChunkStatus,
        load_or_generate: bool,
    ) -> Result<Option<ChunkReference>, LevelError> {
        self.check_reentrant_full(pos, status)?;
        let result = if let Some(driver) = &self.acquire_driver {
            self.acquisition
                .get_chunk(driver.as_ref(), self, pos, status, load_or_generate)
        } else if let Some(chunk) = self.chunk(pos) {
            Ok(Some(ChunkReference {
                pos,
                generation: chunk.publication_generation,
                kind: ChunkKind::Full,
            }))
        } else if load_or_generate {
            // Legacy attached-only levels cannot drive IO/generation. This is a
            // required error under both policies until a real driver is installed.
            Err(LevelError::MissingChunk(pos))
        } else {
            Ok(None)
        }?;
        self.check_reentrant_full(pos, status)?;
        Ok(result)
    }

    /// ServerChunkCache#getChunk when its owner has exclusive Level access.
    /// Uses the same caller policy as shared reads; main tasks retain the
    /// mutable capability instead of erasing it before entity publication.
    pub fn get_chunk_mut(
        &mut self,
        pos: ChunkPos,
        status: ChunkStatus,
        load_or_generate: bool,
    ) -> Result<Option<ChunkReference>, LevelError> {
        if self.acquire_driver.is_none() {
            return self.get_chunk(pos, status, load_or_generate);
        }
        self.check_reentrant_full(pos, status)?;
        let result = AcquisitionAccess::Mutable(self).get_chunk(pos, status, load_or_generate)?;
        self.check_reentrant_full(pos, status)?;
        Ok(result)
    }

    /// One managed poll of already requested work. SPAWN_SEARCH uses this after
    /// installing its own ticket/future: no cache lookup, UNKNOWN or new request.
    pub fn poll_chunk_acquisition_mut(&mut self) -> Result<AcquireProgress, LevelError> {
        self.run_chunk_acquisition_task(AcquireTaskKind::Poll)
    }

    fn check_reentrant_full(&self, pos: ChunkPos, status: ChunkStatus) -> Result<(), LevelError> {
        if status == ChunkStatus::Full
            && self
                .chunk(pos)
                .is_some_and(|chunk| !chunk.full_publication_complete())
        {
            return Err(LevelError::ChunkAcquisition(
                pos,
                status,
                AcquireFailure::ReentrantFull,
            ));
        }
        Ok(())
    }

    fn run_chunk_acquisition_task(
        &mut self,
        kind: AcquireTaskKind,
    ) -> Result<AcquireProgress, LevelError> {
        let driver = self
            .acquire_driver
            .as_deref()
            .ok_or(LevelError::ChunkAcquisition(
                ChunkPos::ZERO,
                ChunkStatus::Full,
                AcquireFailure::DriverUnavailable,
            ))?;
        if !driver.is_owner_thread() {
            return Err(LevelError::ChunkAcquisition(
                ChunkPos::ZERO,
                ChunkStatus::Full,
                AcquireFailure::NotOwner,
            ));
        }
        let task = driver.main_thread_acquisition_task(self, kind)?;
        let progress = if let Some(task) = task {
            task.run(self)?
        } else {
            let driver = self.acquire_driver.as_deref().unwrap();
            shared_acquisition_step(driver, self, kind)?
        };
        if progress == AcquireProgress::DistanceUpdates {
            self.clear_chunk_source_cache();
        }
        Ok(progress)
    }

    pub fn get_full_chunk(&self, pos: ChunkPos) -> Result<&super::chunk::LevelChunk, LevelError> {
        let reference =
            self.get_chunk(pos, ChunkStatus::Full, true)?
                .ok_or(LevelError::ChunkAcquisition(
                    pos,
                    ChunkStatus::Full,
                    AcquireFailure::MissingResult,
                ))?;
        self.resolve_full_chunk(reference)
    }

    pub(super) fn resolve_full_chunk(
        &self,
        reference: ChunkReference,
    ) -> Result<&super::chunk::LevelChunk, LevelError> {
        self.chunk(reference.pos)
            .filter(|chunk| {
                reference.kind == ChunkKind::Full
                    && chunk.publication_generation == reference.generation
            })
            .ok_or(LevelError::ChunkAcquisition(
                reference.pos,
                ChunkStatus::Full,
                AcquireFailure::Publication,
            ))
    }

    /// Port of ServerChunkCache#getChunkNow; does not invoke the acquisition loop.
    pub fn get_chunk_now(
        &self,
        pos: ChunkPos,
    ) -> Result<Option<&super::chunk::LevelChunk>, LevelError> {
        if self
            .chunk(pos)
            .is_some_and(|chunk| !chunk.full_publication_complete())
        {
            return Ok(None);
        }
        if let Some(driver) = &self.acquire_driver {
            self.acquisition
                .get_chunk_now(driver.as_ref(), pos)
                .map(|reference| self.resolve_full_chunk(reference))
                .transpose()
        } else {
            Ok(self.chunk(pos))
        }
    }

    pub fn has_chunk(&self, pos: ChunkPos) -> bool {
        self.acquire_driver.as_ref().map_or_else(
            || self.chunk(pos).is_some(),
            |driver| self.acquisition.has_chunk(driver.as_ref(), pos),
        )
    }
    pub fn has_chunk_at(&self, pos: ferraloom_util::BlockPos) -> bool {
        self.has_chunk(ChunkPos::containing(pos))
    }
    pub fn is_loaded(&self, pos: ferraloom_util::BlockPos) -> bool {
        self.is_in_valid_bounds(pos) && self.has_chunk_at(pos)
    }
    pub fn get_chunk_for_collisions(
        &self,
        pos: ChunkPos,
    ) -> Result<Option<&super::chunk::LevelChunk>, LevelError> {
        self.get_chunk(pos, ChunkStatus::Full, false)?
            .map(|reference| self.resolve_full_chunk(reference))
            .transpose()
    }
    /// Port of Level#loadedAndEntityCanStandOnFace, with the no-ticket FULL
    /// query before capturing the entity's actual collision context.
    pub fn loaded_and_entity_can_stand_on_face(
        &self,
        pos: ferraloom_util::BlockPos,
        entity: crate::entity::EntityRef,
        face: ferraloom_util::Direction,
    ) -> Result<bool, LevelError> {
        if !self.is_in_valid_bounds(pos) {
            return Ok(false);
        }
        let Some(chunk) = self.get_chunk_for_collisions(ChunkPos::containing(pos))? else {
            return Ok(false);
        };
        let state = chunk.get_block_state(pos);
        let context = crate::block::BlockCollisionContext::Entity(
            crate::entity::collision_context::EntityCollisionView::capture(entity, self, false)?,
        );
        let shape =
            crate::block::LevelReaderRef::Server(self).collision_shape(state, pos, &context)?;
        Ok(ferraloom_util::shapes::is_face_full(&shape, face))
    }
    pub fn get_chunk_for_lighting(&self, pos: ChunkPos) -> Option<ChunkReference> {
        self.acquire_driver.as_ref().map_or_else(
            || {
                self.chunk(pos).map(|chunk| ChunkReference {
                    pos,
                    generation: chunk.publication_generation,
                    kind: ChunkKind::Full,
                })
            },
            |driver| {
                self.acquisition
                    .get_chunk_for_lighting(driver.as_ref(), pos)
            },
        )
    }
    /// Call after a successful distance/visible-map update outside managedBlock.
    pub fn clear_chunk_source_cache(&self) {
        self.acquisition.clear_cache();
    }
    /// Native end-of-ServerChunkCache#tick cache reset, with exclusive retirement
    /// of light snapshots whose shared readers have all ended.
    pub fn finish_chunk_source_tick(&mut self) {
        self.acquisition.clear_cache();
        self.prune_light_snapshots();
    }
}

fn shared_acquisition_step(
    driver: &dyn AcquireDriver,
    level: &Level,
    kind: AcquireTaskKind,
) -> Result<AcquireProgress, LevelError> {
    match kind {
        AcquireTaskKind::DistanceUpdates => {
            driver.run_distance_manager_updates(level).map(|changed| {
                if changed {
                    AcquireProgress::DistanceUpdates
                } else {
                    AcquireProgress::Idle
                }
            })
        }
        AcquireTaskKind::Poll => driver.poll_acquisition_step(level),
    }
}

/// Capability differs only at task execution. Each read returns owned state,
/// ending cache/driver borrows before a mutable task or nested callback runs.
enum AcquisitionAccess<'a> {
    Shared {
        acquisition: &'a ChunkAcquisition,
        driver: &'a dyn AcquireDriver,
        level: &'a Level,
    },
    Mutable(&'a mut Level),
}
impl AcquisitionAccess<'_> {
    fn read<T>(&self, read: impl FnOnce(&ChunkAcquisition, &dyn AcquireDriver, &Level) -> T) -> T {
        match self {
            Self::Shared {
                acquisition,
                driver,
                level,
            } => read(acquisition, *driver, level),
            Self::Mutable(level) => read(
                &level.acquisition,
                level.acquire_driver.as_deref().unwrap(),
                level,
            ),
        }
    }
    fn poll(&mut self, kind: AcquireTaskKind) -> Result<AcquireProgress, LevelError> {
        match self {
            Self::Shared { driver, level, .. } => shared_acquisition_step(*driver, level, kind),
            Self::Mutable(level) => level.run_chunk_acquisition_task(kind),
        }
    }
    fn get_chunk(
        &mut self,
        pos: ChunkPos,
        status: ChunkStatus,
        load_or_generate: bool,
    ) -> Result<Option<ChunkReference>, LevelError> {
        if !self.read(|_, driver, _| driver.is_owner_thread()) {
            let chunk =
                self.read(|_, driver, _| driver.acquire_on_owner(pos, status, load_or_generate))?;
            if load_or_generate && chunk.is_none() {
                return Err(LevelError::ChunkAcquisition(
                    pos,
                    status,
                    AcquireFailure::MissingResult,
                ));
            }
            return Ok(chunk);
        }
        let cached = self.read(|acquisition, _, _| {
            acquisition
                .cache
                .get()
                .into_iter()
                .flatten()
                .find(|entry| {
                    entry.pos == pos
                        && entry.status == status
                        && (entry.chunk.is_some() || !load_or_generate)
                })
                .map(|entry| entry.chunk)
        });
        if let Some(chunk) = cached {
            return Ok(chunk);
        }
        if load_or_generate {
            self.read(|_, driver, _| driver.add_unknown_ticket(pos, status))?;
            if !self.read(|_, driver, _| ChunkAcquisition::eligible(driver, pos, status)) {
                if self.poll(AcquireTaskKind::DistanceUpdates)? == AcquireProgress::DistanceUpdates
                {
                    self.read(|acquisition, _, _| acquisition.clear_cache());
                }
                if !self.read(|_, driver, _| ChunkAcquisition::eligible(driver, pos, status)) {
                    return Err(LevelError::ChunkAcquisition(
                        pos,
                        status,
                        AcquireFailure::MissingHolder,
                    ));
                }
            }
        }
        if !self.read(|_, driver, _| ChunkAcquisition::eligible(driver, pos, status)) {
            return self.finish(pos, status, load_or_generate, None);
        }
        let handle = self.read(|_, driver, level| driver.request_status(level, pos, status))?;
        if handle.pos != pos || handle.status != status {
            return Err(LevelError::ChunkAcquisition(
                pos,
                status,
                AcquireFailure::InvalidHandle,
            ));
        }
        let chunk = loop {
            if let AcquireResult::Complete(chunk) =
                self.read(|_, driver, _| driver.acquisition_result(handle))?
            {
                break chunk;
            }
            match self.poll(AcquireTaskKind::Poll)? {
                AcquireProgress::DistanceUpdates => {
                    self.read(|acquisition, _, _| acquisition.clear_cache())
                }
                AcquireProgress::Task => (),
                AcquireProgress::Idle => {
                    if let AcquireResult::Complete(chunk) =
                        self.read(|_, driver, _| driver.acquisition_result(handle))?
                    {
                        break chunk;
                    }
                    self.read(|_, driver, _| driver.wait_for_progress())?;
                }
            }
        };
        self.finish(pos, status, load_or_generate, chunk)
    }
    fn finish(
        &self,
        pos: ChunkPos,
        status: ChunkStatus,
        load_or_generate: bool,
        chunk: Option<ChunkReference>,
    ) -> Result<Option<ChunkReference>, LevelError> {
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
        self.read(|acquisition, _, _| acquisition.cache(pos, status, chunk));
        Ok(chunk)
    }
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
        AcquisitionAccess::Shared {
            acquisition: self,
            driver,
            level,
        }
        .get_chunk(pos, status, load_or_generate)
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
