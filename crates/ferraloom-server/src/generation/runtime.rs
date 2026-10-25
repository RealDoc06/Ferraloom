//! Reentrant owner of the live chunk pipeline. Backend guards cover pure plans
//! only; owned actions execute after those guards end.
mod live_level;
mod promotion;
mod tasks;
pub(crate) use live_level::LiveLevel;

use crate::{
    properties::ServerProperties,
    world::{EntityIoCompletion, LitChunk, World},
};
use anyhow::Result;
use ferraloom_game::{
    entity::EntityRef,
    level::{
        Level, LevelError,
        acquisition::{
            AcquireDriver, AcquireFailure, AcquireHandle, AcquireProgress, AcquireResult,
            ChunkReference,
        },
        chunk_holder::FullStatusPromotion,
        chunk_map::ChunkMap,
        distance_manager::{DistanceManager, FullChunkStatus},
        generation::{
            ChunkRangeFuture, ChunkStatus, GenerationFutureId, GenerationFutureResult,
            GenerationTaskChange, STATUSES,
        },
        simulation::ChunkLevelChange,
        tickets::TicketStorage,
    },
};
use ferraloom_util::{ChunkPos, SectionPos};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    sync::{Arc, mpsc},
    task::{Wake, Waker},
    thread::{self, ThreadId},
};

struct ProgressWake(mpsc::Sender<()>);
impl Wake for ProgressWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.0.send(());
    }
}

struct StatusPublication {
    pos: ChunkPos,
    epoch: u64,
    confirmed: Option<FullChunkStatus>,
    promotion: Option<FullStatusPromotion>,
}
struct OwnerAcquisition {
    pos: ChunkPos,
    status: ChunkStatus,
    load: bool,
    answer: mpsc::Sender<Result<Option<ChunkReference>, LevelError>>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PhaseOwner {
    Task(tokio::task::Id),
    Thread(ThreadId),
}
impl PhaseOwner {
    fn current() -> Self {
        tokio::task::try_id().map_or_else(|| Self::Thread(thread::current().id()), Self::Task)
    }
    fn is_current(self) -> bool {
        self == Self::current()
    }
}

/// Level solely owns this concrete driver. There is no detached backend during
/// callbacks, no second simulation graph, and no guard crosses a publication.
pub(crate) struct GenerationRuntime {
    world: RefCell<World>,
    chunks: RefCell<ChunkMap>,
    tickets: RefCell<TicketStorage>,
    distance_manager: RefCell<DistanceManager>,
    jobs: RefCell<tasks::GenerationJobs>,
    status_publications: RefCell<VecDeque<StatusPublication>>,
    player_loading_futures: RefCell<VecDeque<AcquireHandle>>,
    properties: Arc<ServerProperties>,
    progress_failures: RefCell<Vec<anyhow::Error>>,
    phase_owner: Cell<PhaseOwner>,
    progress: mpsc::Receiver<()>,
    notify_progress: mpsc::Sender<()>,
    owner_requests: mpsc::Receiver<OwnerAcquisition>,
    send_owner_request: mpsc::Sender<OwnerAcquisition>,
}

impl GenerationRuntime {
    pub(crate) fn new(
        dimension: ferraloom_protocol::Identifier,
        mut world: World,
        chunks: ChunkMap,
        tickets: TicketStorage,
        distance_manager: DistanceManager,
        properties: Arc<ServerProperties>,
    ) -> Result<Self> {
        world.validate_dimension_binding(&dimension)?;
        let (notify, progress) = mpsc::channel();
        let (send_owner_request, owner_requests) = mpsc::channel();
        world.set_progress_waker(Waker::from(Arc::new(ProgressWake(notify.clone()))));
        Ok(Self {
            world: RefCell::new(world),
            chunks: RefCell::new(chunks),
            tickets: RefCell::new(tickets),
            distance_manager: RefCell::new(distance_manager),
            jobs: RefCell::new(tasks::GenerationJobs::default()),
            status_publications: RefCell::new(VecDeque::new()),
            player_loading_futures: RefCell::new(VecDeque::new()),
            properties,
            progress_failures: RefCell::new(Vec::new()),
            phase_owner: Cell::new(PhaseOwner::current()),
            progress,
            notify_progress: notify,
            owner_requests,
            send_owner_request,
        })
    }

    /// Tokio may move the exclusive server continuation after an await. Bind
    /// ownership before entering a phase, never during a parked shared read.
    pub(crate) fn enter_owner_thread(&self) {
        self.phase_owner.set(PhaseOwner::current());
    }
    pub(crate) fn from_level(level: &Level) -> &Self {
        level
            .acquire_driver()
            .expect("live level has its generation driver")
            .as_any()
            .downcast_ref()
            .expect("live level uses GenerationRuntime")
    }

    /// Cold metadata/IO plans return owned values. The closure must not enter
    /// Level, publish canonical data, await, or call another driver operation.
    pub(crate) fn with_world<T>(&self, plan: impl FnOnce(&World) -> T) -> T {
        plan(&self.world.borrow())
    }
    pub(crate) fn with_world_mut<T>(&self, plan: impl FnOnce(&mut World) -> T) -> T {
        plan(&mut self.world.borrow_mut())
    }
    pub(crate) fn with_chunks<T>(&self, plan: impl FnOnce(&ChunkMap) -> T) -> T {
        plan(&self.chunks.borrow())
    }
    pub(crate) fn with_chunks_mut<T>(&self, plan: impl FnOnce(&mut ChunkMap) -> T) -> T {
        plan(&mut self.chunks.borrow_mut())
    }
    pub(crate) fn with_tickets<T>(&self, plan: impl FnOnce(&TicketStorage) -> T) -> T {
        plan(&self.tickets.borrow())
    }
    pub(crate) fn with_tickets_mut<T>(&self, plan: impl FnOnce(&mut TicketStorage) -> T) -> T {
        plan(&mut self.tickets.borrow_mut())
    }

    pub(crate) fn update_player(&self, player: EntityRef, section: Option<SectionPos>) {
        self.distance_manager.borrow_mut().update_player(
            player,
            section,
            &mut self.tickets.borrow_mut(),
        );
    }

    /// The only simulation graph is Region's tracker. This call is a pure graph
    /// pass; both backend guards and the tracker guard end before its caller
    /// publishes any returned holder or entity transition.
    fn distance_changes(&self, level: &Level) -> Vec<ChunkLevelChange> {
        let mut manager = self.distance_manager.borrow_mut();
        let mut tickets = self.tickets.borrow_mut();
        level.run_distance_manager_updates(&mut manager, &mut tickets)
    }

    fn run_distance_pass(&self, level: &Level) -> Result<bool, LevelError> {
        let mut changed = false;
        loop {
            let updates = self.distance_changes(level);
            let graph_changed = !updates.is_empty();
            changed |= graph_changed;
            let changes = updates
                .into_iter()
                .filter(|change| {
                    change.level <= 44
                        || self
                            .chunks
                            .borrow()
                            .holders
                            .contains_key(&change.pos.pack())
                })
                .collect::<Vec<_>>();
            // Native first publishes every visible ticket-level change, then
            // updates highestAllowedStatus for all affected holders.
            for change in &changes {
                self.chunks
                    .borrow_mut()
                    .ensure_holder(change.pos)
                    .ticket_level = change.level;
                self.world
                    .borrow_mut()
                    .update_generation_priority(change.pos, change.level);
            }
            if graph_changed {
                level.clear_chunk_source_cache();
            }
            for change in &changes {
                let task = self
                    .chunks
                    .borrow_mut()
                    .update_highest_allowed_status(change.pos);
                self.apply_task_change(change.pos, task);
            }
            for change in changes {
                let publication = {
                    let mut chunks = self.chunks.borrow_mut();
                    let holder = chunks.holders.get_mut(&change.pos.pack()).unwrap();
                    StatusPublication {
                        pos: change.pos,
                        epoch: holder.epoch,
                        confirmed: holder.update_full_status(),
                        promotion: None,
                    }
                };
                self.status_publications.borrow_mut().push_back(publication);
                if change.level <= 33 {
                    self.schedule_status(change.pos, ChunkStatus::Full)?;
                }
            }
            self.publish_status_changes(level)?;
            {
                let mut manager = self.distance_manager.borrow_mut();
                manager.poll_player_loading_tasks(&mut self.tickets.borrow_mut());
            }
            if !self.tickets.borrow().has_pending_source_updates() {
                break;
            }
        }
        let waiters = {
            let mut manager = self.distance_manager.borrow_mut();
            manager.take_player_loading_waiters(&self.tickets.borrow())
        };
        for pos in waiters {
            let handle = self.schedule_status(pos, ChunkStatus::Full)?;
            self.player_loading_futures.borrow_mut().push_back(handle);
        }
        self.confirm_ready_ranges(level)?;
        self.settle_player_loading_futures();
        let requests = level.take_entity_storage_requests();
        self.world
            .borrow_mut()
            .submit_entity_requests(requests, &self.properties)
            .map_err(|_| {
                LevelError::ChunkAcquisition(ChunkPos::ZERO, ChunkStatus::Full, AcquireFailure::Io)
            })?;
        Ok(changed)
    }

    fn publish_status_changes(&self, level: &Level) -> Result<(), LevelError> {
        loop {
            let publication = self.status_publications.borrow_mut().pop_front();
            let Some(publication) = publication else {
                return Ok(());
            };
            let current = {
                let mut chunks = self.chunks.borrow_mut();
                chunks
                    .holders
                    .get_mut(&publication.pos.pack())
                    .filter(|holder| holder.epoch == publication.epoch)
                    .map(|holder| {
                        let confirmed = publication
                            .promotion
                            .map_or(publication.confirmed, |promotion| {
                                holder.confirm_full_status(promotion)
                            });
                        (holder.requested_full_status(), confirmed)
                    })
            };
            let Some((requested, confirmed)) = current else {
                continue;
            };
            if level.chunk(publication.pos).is_some() {
                let ticking = requested >= FullChunkStatus::BlockTicking
                    && self.world.borrow().ready.contains(&publication.pos)
                    && self.range_ready(publication.pos, FullChunkStatus::BlockTicking);
                level.set_chunk_loading_status(publication.pos, requested, ticking)?;
            }
            if let Some(status) = confirmed {
                use ferraloom_game::level::entity::Visibility;
                let visibility = match status {
                    FullChunkStatus::Inaccessible => Visibility::Hidden,
                    FullChunkStatus::Full | FullChunkStatus::BlockTicking => Visibility::Tracked,
                    FullChunkStatus::EntityTicking => Visibility::Ticking,
                };
                level.update_entity_chunk_status_shared(publication.pos, visibility)?;
            }
        }
    }

    fn range_ready(&self, pos: ChunkPos, status: FullChunkStatus) -> bool {
        let chunks = self.chunks.borrow();
        ChunkRangeFuture::for_full_status(pos, status).is_ready(|neighbor| {
            chunks
                .holders
                .get(&neighbor.pack())?
                .generation
                .latest_completed_status()
        })
    }

    fn confirm_ready_ranges(&self, level: &Level) -> Result<(), LevelError> {
        for pos in self.holder_positions() {
            let promotion = {
                let chunks = self.chunks.borrow();
                let holder = &chunks.holders[&pos.pack()];
                holder
                    .pending_full_status()
                    .map(|promotion| (holder.epoch, promotion))
            };
            let Some((epoch, promotion)) = promotion else {
                continue;
            };
            if !self.range_ready(pos, promotion.status)
                || (promotion.status == FullChunkStatus::BlockTicking
                    && !self.world.borrow().ready.contains(&pos))
            {
                continue;
            }
            let mut publications = self.status_publications.borrow_mut();
            if !publications.iter().any(|queued| {
                queued.pos == pos && queued.epoch == epoch && queued.promotion == Some(promotion)
            }) {
                publications.push_back(StatusPublication {
                    pos,
                    epoch,
                    confirmed: None,
                    promotion: Some(promotion),
                });
            }
        }
        self.publish_status_changes(level)
    }

    fn settle_player_loading_futures(&self) {
        let mut remaining = VecDeque::new();
        loop {
            let handle = self.player_loading_futures.borrow_mut().pop_front();
            let Some(handle) = handle else {
                break;
            };
            let failed = matches!(
                self.status_result(handle),
                Err(_) | Ok(AcquireResult::Complete(None))
            );
            if failed || self.range_ready(handle.pos, FullChunkStatus::EntityTicking) {
                self.distance_manager
                    .borrow_mut()
                    .complete_player_loading(handle.pos);
            } else {
                remaining.push_back(handle);
            }
        }
        // No callbacks run in this pure pass, so remaining waiters preserve the
        // source primitive-set registration order without stranding nested work.
        *self.player_loading_futures.borrow_mut() = remaining;
    }

    fn publish_generation_statuses(&self) {
        let statuses = self
            .world
            .borrow()
            .generation_statuses
            .iter()
            .map(|(&key, &status)| (ChunkPos::unpack(key), status))
            .collect::<Vec<_>>();
        let mut chunks = self.chunks.borrow_mut();
        for (pos, status) in statuses {
            // FULL is published only after postLoad/finalization and center
            // light, never from the generation worker's status summary.
            let status = status.min(ChunkStatus::Spawn);
            let holder = chunks.ensure_holder(pos);
            let reference = ChunkReference {
                pos,
                generation: holder.epoch,
                kind: ferraloom_game::level::acquisition::ChunkKind::Proto,
            };
            for status in STATUSES[..=status as usize].iter().copied() {
                let existing = holder
                    .generation
                    .future_id(status)
                    .and_then(|future| holder.generation.result(status, future));
                if matches!(existing, None | Some(GenerationFutureResult::Pending)) {
                    holder.generation.acquire_status_bump(status);
                    holder.generation.complete(status, reference);
                }
            }
        }
    }

    /// One owned completion starts at a time. Later completions stay on World
    /// while this one enters callbacks, so reentrant acquisition can reach them.
    fn poll_owned_step(&self, level: &Level) -> Result<bool> {
        let mut progressed = {
            let mut world = self.world.borrow_mut();
            world.refresh_generation_snapshots(level);
            world.poll_generation_progress()?
        };
        self.release_finished_jobs();
        let completion = self.world.borrow_mut().take_generation_completion();
        match completion {
            Ok(Some((task, terrain))) => {
                progressed = true;
                let current = self.jobs.borrow().is_current(&self.chunks.borrow(), task);
                if current {
                    let target = self.jobs.borrow().get(task).unwrap().token.target;
                    if target == ChunkStatus::Full {
                        self.begin_full_promotion(task, terrain)?;
                    } else {
                        self.world.borrow_mut().store_generation_chunk(
                            terrain.pos,
                            terrain.revision,
                            terrain.disk,
                        )?;
                        self.jobs.borrow_mut().complete_integration(task);
                    }
                }
                self.publish_generation_statuses();
                self.publish_generation_skips(level)?;
                self.publish_structure_updates(level)?;
            }
            Ok(None) => {}
            Err(error) => {
                let failed = std::mem::take(&mut self.world.borrow_mut().failed_generation);
                for (pos, task) in failed {
                    if self.jobs.borrow().is_current(&self.chunks.borrow(), task) {
                        self.chunks
                            .borrow_mut()
                            .holders
                            .get_mut(&pos.pack())
                            .unwrap()
                            .generation
                            .fail_pending(AcquireFailure::Generation);
                        self.jobs.borrow_mut().complete_integration(task);
                    }
                }
                self.release_finished_jobs();
                return Err(error);
            }
        }
        // Draining every light result into a local Vec would strand imports
        // across a callback. Poll and publish just this owned actor completion.
        let light = self.take_light_completion()?;
        if let Some(light) = light {
            self.import_light(level, light)?;
            progressed = true;
        }
        if let Some(task) = self.next_full_promotion() {
            self.resume_full_promotion(level, task)?;
            progressed = true;
        }
        progressed |= self.poll_entity_progress(level)?;
        self.confirm_ready_ranges(level)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        self.settle_player_loading_futures();
        self.queue_exclusive_promotions();
        self.release_finished_jobs();
        Ok(progressed)
    }

    fn publish_generation_skips(&self, level: &Level) -> Result<()> {
        let skips = std::mem::take(&mut self.world.borrow_mut().generation_skips)
            .into_iter()
            .map(ferraloom_game::block::WorldgenSkip::from)
            .collect();
        level
            .record_worldgen_skips(skips)
            .map_err(|error| anyhow::anyhow!("{error:?}"))
    }

    fn queue_exclusive_promotions(&self) {
        for pos in self.holder_positions() {
            let identity = {
                let chunks = self.chunks.borrow();
                let holder = &chunks.holders[&pos.pack()];
                if holder.requested_full_status() < FullChunkStatus::BlockTicking
                    || holder
                        .generation
                        .chunk_if_present_unchecked(ChunkStatus::Full)
                        .is_none()
                {
                    continue;
                }
                super::GenerationIdentity::of(holder)
            };
            if self.range_ready(pos, FullChunkStatus::BlockTicking) {
                let mut world = self.world.borrow_mut();
                if !world.ready.contains(&pos) && !world.exclusive_ready_tasks.contains(&identity) {
                    world.exclusive_ready_tasks.push_back(identity);
                }
            }
        }
    }
    pub(super) fn refresh_loading_status(
        &self,
        level: &Level,
        pos: ChunkPos,
    ) -> Result<(), LevelError> {
        let epoch = self
            .chunks
            .borrow()
            .holders
            .get(&pos.pack())
            .ok_or(LevelError::MissingChunk(pos))?
            .epoch;
        self.status_publications
            .borrow_mut()
            .push_back(StatusPublication {
                pos,
                epoch,
                confirmed: None,
                promotion: None,
            });
        self.publish_status_changes(level)
    }

    /// Controlled transport fixture: the normal worker has already returned the
    /// supplied owned result. Scheduling, FULL/light and callbacks stay real.
    #[cfg(test)]
    pub(crate) fn accept_test_terrain(
        &self,
        level: &Level,
        task: u64,
        terrain: crate::world::Terrain,
    ) -> Result<()> {
        let pos = terrain.pos;
        let token = self.chunks.borrow().holders[&pos.pack()]
            .generation
            .current_task()
            .filter(|token| token.id == task)
            .expect("fixture has a scheduled task");
        self.jobs
            .borrow_mut()
            .claim(&mut self.chunks.borrow_mut(), pos, token);
        self.jobs.borrow_mut().worker_settled(task);
        self.begin_full_promotion(task, terrain)?;
        self.publish_generation_statuses();
        self.publish_generation_skips(level)?;
        self.publish_structure_updates(level)?;
        Ok(())
    }

    /// Exact error is retained for diagnostics; status futures carry the typed
    /// failure category rather than losing ownership of a failed continuation.
    fn retain_progress_failure(&self, error: anyhow::Error) -> LevelError {
        eprintln!("Chunk pipeline failure: {error:?}");
        self.progress_failures.borrow_mut().push(error);
        for holder in self.chunks.borrow_mut().holders.values_mut() {
            holder.generation.fail_pending(AcquireFailure::Generation);
        }
        LevelError::ChunkAcquisition(
            ChunkPos::ZERO,
            ChunkStatus::Full,
            AcquireFailure::Generation,
        )
    }

    fn apply_task_change(&self, pos: ChunkPos, change: GenerationTaskChange) {
        if let Some(cancel) = change.cancel {
            self.jobs.borrow_mut().cancel(cancel.id);
            self.world
                .borrow_mut()
                .cancel_generation_task(pos, cancel.id);
        }
        if let Some(schedule) = change.schedule {
            self.jobs
                .borrow_mut()
                .claim(&mut self.chunks.borrow_mut(), pos, schedule);
            self.world
                .borrow_mut()
                .request_status(schedule.id, pos, schedule.target);
        }
        self.release_finished_jobs();
    }

    /// The per-status future belongs to the holder; task replacement never
    /// changes the handle already returned to an acquiring caller.
    fn schedule_status(
        &self,
        pos: ChunkPos,
        status: ChunkStatus,
    ) -> Result<AcquireHandle, LevelError> {
        let (epoch, schedule) = {
            let mut chunks = self.chunks.borrow_mut();
            let epoch = chunks
                .holders
                .get(&pos.pack())
                .ok_or(LevelError::ChunkAcquisition(
                    pos,
                    status,
                    AcquireFailure::MissingHolder,
                ))?
                .epoch;
            (epoch, chunks.schedule_generation_task(pos, status))
        };
        self.apply_task_change(pos, schedule.task);
        Ok(AcquireHandle {
            pos,
            status,
            request_id: schedule.future.0,
            holder_epoch: epoch,
        })
    }

    fn status_result(&self, handle: AcquireHandle) -> Result<AcquireResult, LevelError> {
        let chunks = self.chunks.borrow();
        let holder = chunks
            .holders
            .get(&handle.pos.pack())
            .filter(|holder| holder.epoch == handle.holder_epoch)
            .ok_or(LevelError::ChunkAcquisition(
                handle.pos,
                handle.status,
                AcquireFailure::Cancelled,
            ))?;
        match holder
            .generation
            .result(handle.status, GenerationFutureId(handle.request_id))
        {
            Some(GenerationFutureResult::Pending) => Ok(AcquireResult::Pending),
            Some(GenerationFutureResult::Complete(chunk)) => Ok(AcquireResult::Complete(chunk)),
            Some(GenerationFutureResult::Failed(failure)) => Err(LevelError::ChunkAcquisition(
                handle.pos,
                handle.status,
                failure,
            )),
            None => Err(LevelError::ChunkAcquisition(
                handle.pos,
                handle.status,
                AcquireFailure::InvalidHandle,
            )),
        }
    }

    fn proto_biome(
        &self,
        chunk: ChunkReference,
        quart: [i32; 3],
    ) -> Result<ferraloom_chunk::BiomeId, LevelError> {
        let current = self
            .chunks
            .borrow()
            .holders
            .get(&chunk.pos.pack())
            .is_some_and(|holder| holder.epoch == chunk.generation);
        if !current {
            return Err(LevelError::ChunkAcquisition(
                chunk.pos,
                ChunkStatus::Biomes,
                AcquireFailure::InvalidHandle,
            ));
        }
        self.world
            .borrow()
            .proto_noise_biome(chunk.pos, quart)
            .ok_or(LevelError::ChunkAcquisition(
                chunk.pos,
                ChunkStatus::Biomes,
                AcquireFailure::MissingResult,
            ))
    }

    fn sample_uncached_biome(
        &self,
        quart: [i32; 3],
    ) -> Result<ferraloom_chunk::BiomeId, LevelError> {
        use ferraloom_worldgen::{
            climate::SearchContext,
            density::SamplerContext,
            generator::{ChunkGenerator, ChunkGeneratorAccess},
        };
        let generator = self.world.borrow().generator();
        let biome = match generator.as_ref() {
            ChunkGenerator::Noise(noise) => noise.source.sample(
                &noise.random.compiler,
                noise.random.router,
                &mut SamplerContext::cached(),
                &mut SearchContext::default(),
                quart,
            ),
            ChunkGenerator::Flat(flat) => flat.settings.biome.clone(),
            ChunkGenerator::Debug(_) => {
                "minecraft:plains".parse().expect("static biome identifier")
            }
        };
        generator
            .data()
            .biome_ids
            .get(&biome)
            .copied()
            .ok_or(LevelError::UnsupportedWorldgenEnvironment)
    }

    fn release_finished_jobs(&self) {
        for task in self.world.borrow_mut().take_settled_generation() {
            self.jobs.borrow_mut().worker_settled(task);
        }
        self.jobs
            .borrow_mut()
            .release_finished(&mut self.chunks.borrow_mut());
    }

    /// Pop one owned IO result, release World, then enter entity publication.
    /// A nested acquire can still reach every later IO result in the backend.
    pub(crate) fn poll_entity_progress(&self, level: &Level) -> Result<bool> {
        let completion = self
            .world
            .borrow_mut()
            .take_entity_io_completion(&self.properties)?;
        let progressed = completion.is_some();
        match completion {
            Some(EntityIoCompletion::Load(id, result)) => {
                level.complete_entity_chunk_load(id, result)?;
            }
            Some(EntityIoCompletion::Store(result)) => result?,
            None => {}
        }
        level.process_pending_entity_loads_shared()?;
        let requests = level.take_entity_storage_requests();
        self.world
            .borrow_mut()
            .submit_entity_requests(requests, &self.properties)?;
        Ok(progressed)
    }

    pub(crate) fn synchronize_entities(
        &self,
        flush: bool,
    ) -> Result<ferraloom_storage::IoTask<()>> {
        self.world
            .borrow_mut()
            .synchronize_entities(&self.properties, flush)
    }

    pub(crate) fn take_light_completion(&self) -> Result<Option<LitChunk>> {
        self.world.borrow_mut().take_light_completion()
    }

    pub(crate) fn publish_structure_updates(&self, level: &Level) -> Result<()> {
        loop {
            let update = self.world.borrow().next_structure_publication(level);
            let Some((pos, revision, structures)) = update else {
                return Ok(());
            };
            level
                .publish_chunk_structures(pos, revision.0, structures)
                .map_err(|error| anyhow::anyhow!("{error:?}"))?;
            self.world
                .borrow_mut()
                .finish_structure_publication(pos, revision);
        }
    }

    pub(crate) fn holder_positions(&self) -> Vec<ChunkPos> {
        self.chunks.borrow().holder_positions().collect()
    }
    pub(crate) fn progress_jobs(&self, level: &Level) -> Result<()> {
        while self
            .poll_acquisition_step(level)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?
            != AcquireProgress::Idle
        {}
        Ok(())
    }

    pub(crate) fn request_full(&self, level: &Level, pos: ChunkPos) -> Result<(), LevelError> {
        self.run_distance_pass(level)?;
        if self
            .visible_ticket_level(pos)
            .is_some_and(|level| level <= 33)
        {
            self.request_status(level, pos, ChunkStatus::Full)?;
        }
        Ok(())
    }

    pub(crate) fn natural_spawn_chunks(&self) -> Vec<ChunkPos> {
        self.distance_manager
            .borrow()
            .natural_spawn
            .chunks()
            .collect()
    }

    pub(crate) fn purge_stale_tickets(&self, tick: u64) {
        let chunks = self.chunks.borrow();
        let world = self.world.borrow();
        self.tickets
            .borrow_mut()
            .purge_stale_with_readiness(tick, |pos| {
                chunks
                    .holders
                    .get(&pos.pack())
                    .is_none_or(|holder| world.chunk_ready_for_saving(holder))
            });
    }

    fn wait_for_worker(&self) -> Result<()> {
        self.progress.recv()?;
        Ok(())
    }
}

impl AcquireDriver for GenerationRuntime {
    /// Port of ChunkMap#updatePlayerStatus: use the canonical DistanceManager.
    fn update_player_tracking_status(
        &self,
        player: EntityRef,
        section: Option<SectionPos>,
    ) -> Result<(), LevelError> {
        self.update_player(player, section);
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn is_owner_thread(&self) -> bool {
        // The sole server Tokio task may migrate between executor threads after
        // an await. Its identity remains the owner; arbitrary tasks do not.
        self.phase_owner.get().is_current()
    }
    fn visible_ticket_level(&self, pos: ChunkPos) -> Option<i32> {
        self.chunks
            .borrow()
            .holders
            .get(&pos.pack())
            .map(|holder| holder.ticket_level)
    }
    fn add_unknown_ticket(&self, pos: ChunkPos, status: ChunkStatus) -> Result<(), LevelError> {
        self.tickets.borrow_mut().add_unknown(pos, status);
        Ok(())
    }
    fn run_distance_manager_updates(&self, level: &Level) -> Result<bool, LevelError> {
        self.run_distance_pass(level)
    }
    fn request_status(
        &self,
        _level: &Level,
        pos: ChunkPos,
        status: ChunkStatus,
    ) -> Result<AcquireHandle, LevelError> {
        self.reject_reentrant_full(pos, status)?;
        self.schedule_status(pos, status)
    }
    fn acquisition_result(&self, handle: AcquireHandle) -> Result<AcquireResult, LevelError> {
        self.reject_reentrant_full(handle.pos, handle.status)?;
        self.status_result(handle)
    }
    fn poll_acquisition_step(&self, level: &Level) -> Result<AcquireProgress, LevelError> {
        if !self.progress_failures.borrow().is_empty() {
            return Err(LevelError::ChunkAcquisition(
                ChunkPos::ZERO,
                ChunkStatus::Full,
                AcquireFailure::Generation,
            ));
        }
        if self.run_distance_pass(level)? {
            return Ok(AcquireProgress::DistanceUpdates);
        }
        if let Ok(request) = self.owner_requests.try_recv() {
            let result = level.get_chunk(request.pos, request.status, request.load);
            let _ = request.answer.send(result);
            return Ok(AcquireProgress::Task);
        }
        self.poll_owned_step(level)
            .map(|progressed| {
                if progressed {
                    AcquireProgress::Task
                } else {
                    AcquireProgress::Idle
                }
            })
            .map_err(|error| self.retain_progress_failure(error))
    }
    fn wait_for_progress(&self) -> Result<(), LevelError> {
        self.wait_for_worker()
            .map_err(|error| self.retain_progress_failure(error))
    }
    fn chunk_if_present(&self, pos: ChunkPos, status: ChunkStatus) -> Option<ChunkReference> {
        self.chunks
            .borrow()
            .holders
            .get(&pos.pack())?
            .generation
            .chunk_if_present(status)
    }
    fn chunk_if_present_unchecked(
        &self,
        pos: ChunkPos,
        status: ChunkStatus,
    ) -> Option<ChunkReference> {
        self.chunks
            .borrow()
            .holders
            .get(&pos.pack())?
            .generation
            .chunk_if_present_unchecked(status)
    }
    fn acquire_on_owner(
        &self,
        pos: ChunkPos,
        status: ChunkStatus,
        load: bool,
    ) -> Result<Option<ChunkReference>, LevelError> {
        let (answer, result) = mpsc::channel();
        self.send_owner_request
            .send(OwnerAcquisition {
                pos,
                status,
                load,
                answer,
            })
            .map_err(|_| LevelError::ChunkAcquisition(pos, status, AcquireFailure::Cancelled))?;
        let _ = self.notify_progress.send(());
        result
            .recv()
            .map_err(|_| LevelError::ChunkAcquisition(pos, status, AcquireFailure::Cancelled))?
    }
    fn proto_noise_biome(
        &self,
        chunk: ChunkReference,
        quart: [i32; 3],
    ) -> Result<ferraloom_chunk::BiomeId, LevelError> {
        self.proto_biome(chunk, quart)
    }
    fn uncached_noise_biome(
        &self,
        quart: [i32; 3],
    ) -> Result<ferraloom_chunk::BiomeId, LevelError> {
        self.sample_uncached_biome(quart)
    }
}

#[cfg(test)]
mod tests;
