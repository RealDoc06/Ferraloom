//! Reentrant owner of the live chunk pipeline. Backend guards cover pure plans
//! only; owned actions execute after those guards end.
mod tasks;

use crate::{
    properties::ServerProperties,
    world::{EntityIoCompletion, LitChunk, World},
};
use anyhow::Result;
use ferraloom_game::{
    entity::EntityRef,
    level::{
        Level, LevelError,
        acquisition::{AcquireFailure, AcquireHandle, AcquireResult, ChunkReference},
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
    owner_thread: Cell<ThreadId>,
    progress: mpsc::Receiver<()>,
}

impl GenerationRuntime {
    pub(crate) fn new(
        mut world: World,
        chunks: ChunkMap,
        tickets: TicketStorage,
        distance_manager: DistanceManager,
        properties: Arc<ServerProperties>,
    ) -> Self {
        let (notify, progress) = mpsc::channel();
        world.set_progress_waker(Waker::from(Arc::new(ProgressWake(notify))));
        Self {
            world: RefCell::new(world),
            chunks: RefCell::new(chunks),
            tickets: RefCell::new(tickets),
            distance_manager: RefCell::new(distance_manager),
            jobs: RefCell::new(tasks::GenerationJobs::default()),
            status_publications: RefCell::new(VecDeque::new()),
            player_loading_futures: RefCell::new(VecDeque::new()),
            properties,
            owner_thread: Cell::new(thread::current().id()),
            progress,
        }
    }

    /// Tokio may move the exclusive server continuation after an await. Bind
    /// ownership before entering a phase, never during a parked shared read.
    pub(crate) fn enter_owner_thread(&self) {
        self.owner_thread.set(thread::current().id());
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
            let changes = self.distance_changes(level);
            changed |= !changes.is_empty();
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
            if !changes.is_empty() {
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
                let chunks = self.chunks.borrow();
                chunks
                    .holders
                    .get(&publication.pos.pack())
                    .filter(|holder| holder.epoch == publication.epoch)
                    .map(|holder| holder.requested_full_status())
            };
            let Some(requested) = current else {
                continue;
            };
            if level.chunk(publication.pos).is_some() {
                let ticking = requested >= FullChunkStatus::BlockTicking
                    && self.world.borrow().ready.contains(&publication.pos)
                    && self.range_ready(publication.pos, FullChunkStatus::BlockTicking);
                level.set_chunk_loading_status(publication.pos, requested, ticking)?;
            }
            if let Some(status) = publication.confirmed {
                use ferraloom_game::level::entity::Visibility;
                let visibility = match status {
                    FullChunkStatus::Inaccessible => Visibility::Hidden,
                    FullChunkStatus::Full | FullChunkStatus::BlockTicking => Visibility::Tracked,
                    FullChunkStatus::EntityTicking => Visibility::Ticking,
                };
                level.update_entity_chunk_status(publication.pos, visibility)?;
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
            let confirmed = {
                let mut chunks = self.chunks.borrow_mut();
                chunks
                    .holders
                    .get_mut(&pos.pack())
                    .filter(|holder| holder.epoch == epoch)
                    .and_then(|holder| holder.confirm_full_status(promotion))
            };
            if confirmed.is_some() {
                self.status_publications
                    .borrow_mut()
                    .push_back(StatusPublication {
                        pos,
                        epoch,
                        confirmed,
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

    fn wait_for_worker(&self) -> Result<()> {
        self.progress.recv()?;
        Ok(())
    }
}
