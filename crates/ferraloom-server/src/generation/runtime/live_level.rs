use super::{GenerationAccess, GenerationRuntime};
use crate::{
    properties::ServerProperties,
    world::{ChunkSaveSnapshot, World},
};
use anyhow::{Result, ensure};
use ferraloom_game::level::{
    Level, LevelError, distance_manager::FullChunkStatus, server_level::ServerLevel,
};
use ferraloom_util::ChunkPos;
use std::sync::Arc;

/// Thin single-region ServerLevel shell. Unlike vanilla's nested ServerLevel ->
/// ServerChunkCache -> ChunkMap, Level owns the installed driver so a suspended
/// shared read can still reach its canonical World, holders and ticket queues.
pub(crate) struct LiveLevel {
    pub level: Level,
    pub game_time: i64,
    pub dimension: String,
}

impl LiveLevel {
    pub(crate) fn install(
        world: World,
        bootstrap: ServerLevel,
        properties: Arc<ServerProperties>,
    ) -> Result<Self> {
        let height = world.build_height();
        let ServerLevel {
            mut level,
            chunks,
            tickets,
            distance_manager,
            game_time,
            dimension,
        } = bootstrap;
        let runtime = GenerationRuntime::new(
            dimension.parse()?,
            world,
            chunks,
            tickets,
            distance_manager,
            properties,
        )?;
        level
            .install_acquire_driver(Box::new(runtime), height)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        Ok(Self {
            level,
            game_time,
            dimension,
        })
    }

    pub(crate) fn generation(&self) -> &GenerationRuntime {
        GenerationRuntime::from_level(&self.level)
    }

    /// Native owner progress retains mutable publication capability. Every
    /// owned task ends its driver borrow before entering Level callbacks.
    pub(crate) fn progress_jobs(&mut self) -> Result<()> {
        while self
            .level
            .poll_chunk_acquisition_mut()
            .map_err(|error| anyhow::anyhow!("{error:?}"))?
            != ferraloom_game::level::acquisition::AcquireProgress::Idle
        {}
        Ok(())
    }
    pub(crate) fn update_chunk_tickets(&mut self) -> Result<(), LevelError> {
        GenerationRuntime::run_distance_pass_with_access(&mut GenerationAccess::Mutable(
            &mut self.level,
        ))?;
        Ok(())
    }
    pub(crate) fn request_full(&mut self, pos: ChunkPos) -> Result<(), LevelError> {
        GenerationRuntime::request_full_mut(&mut self.level, pos)
    }

    /// ServerLevel#waitForEntities / PersistentEntitySectionManager#saveAll may
    /// publish pending IO during their explicit owner wait, without a tick.
    pub(crate) fn poll_entity_loads_for_wait(&mut self) -> Result<bool> {
        let progressed = self.generation().poll_entity_io(&self.level)?;
        self.level.process_pending_entity_loads()?;
        let requests = self.level.take_entity_storage_requests();
        let properties = self.generation().properties.clone();
        self.generation()
            .with_world_mut(|world| world.submit_entity_requests(requests, &properties))?;
        Ok(progressed)
    }

    /// Exclusive prepareTickingChunk continuation. The shared driver only queues
    /// these owned tasks and never fabricates a mutable Level during a read.
    pub(crate) fn finish_promotions(&mut self) -> Result<()> {
        self.generation().queue_exclusive_promotions();
        loop {
            let identity = self
                .generation()
                .with_world_mut(|world| world.exclusive_ready_tasks.pop_front());
            let Some(identity) = identity else {
                break;
            };
            let current = self.generation().with_chunks(|chunks| {
                chunks
                    .holders
                    .get(&identity.pos.pack())
                    .is_some_and(|holder| {
                        identity.matches(holder)
                            && holder.requested_full_status() >= FullChunkStatus::BlockTicking
                    })
            });
            if !current
                || self
                    .generation()
                    .with_world(|world| world.ready.contains(&identity.pos))
            {
                continue;
            }
            let result = self
                .level
                .post_process_generation(identity.pos)
                .and_then(|()| self.level.start_ticking_chunk(identity.pos));
            if let Err(error) = result {
                self.generation()
                    .with_world_mut(|world| world.exclusive_ready_tasks.push_front(identity));
                anyhow::bail!("{error:?}");
            }
            self.generation().with_world_mut(|world| {
                world.ready.insert(identity.pos);
            });
            GenerationRuntime::refresh_loading_status_mut(&mut self.level, identity.pos)
                .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        }
        GenerationRuntime::confirm_ready_ranges_mut(&mut self.level)
            .map_err(|error| anyhow::anyhow!("{error:?}"))
    }

    pub(crate) fn capture_chunk_for_save(
        &mut self,
        pos: ChunkPos,
        game_time: i64,
    ) -> Result<ChunkSaveSnapshot> {
        self.level
            .try_mark_chunk_saved(pos)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        let result = (|| {
            self.level
                .prepare_block_entities_for_saving(pos)
                .map_err(anyhow::Error::new)?;
            self.generation()
                .with_world(|world| world.chunk_save_plan(pos, game_time))
                .and_then(|plan| plan.capture(&self.level))
        })();
        if result.is_err() {
            self.level
                .mark_chunk_unsaved(pos)
                .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        }
        result
    }

    pub(crate) fn evict_unloaded_chunks(&mut self) -> Result<()> {
        let completed = self.generation().with_world_mut(World::poll_unloads)?;
        for (pos, epoch, result) in completed {
            if let Err(error) = result {
                self.level
                    .mark_chunk_unsaved(pos)
                    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
                return Err(error);
            }
            let eligible = self.generation().with_chunks(|chunks| {
                chunks.holders.get(&pos.pack()).is_some_and(|holder| {
                    holder.epoch == epoch
                        && holder.ticket_level > 44
                        && holder.generation.generation_references_released()
                        && holder.pending_full_status().is_none()
                        && holder.confirmed_full_status() == FullChunkStatus::Inaccessible
                })
            });
            if eligible
                && self
                    .level
                    .chunk(pos)
                    .is_some_and(|chunk| !chunk.is_unsaved())
            {
                self.level
                    .detach_block_entities_for_unload(pos)
                    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
                ensure!(
                    self.generation()
                        .with_chunks_mut(|chunks| chunks.remove_unloaded_holder(pos).is_some()),
                    "holder changed during block entity unload"
                );
                self.level.remove_chunk(pos);
                self.generation()
                    .with_world_mut(|world| world.finish_unload(pos))?;
            }
        }
        for pos in self.generation().holder_positions() {
            let state = self.generation().with_chunks(|chunks| {
                let holder = &chunks.holders[&pos.pack()];
                (
                    holder.epoch,
                    holder.ticket_level > 44
                        && holder.generation.generation_references_released()
                        && holder.generation.current_task().is_none(),
                )
            });
            if !state.1 {
                continue;
            }
            if self.level.chunk(pos).is_none() {
                if self.generation().with_world(World::proto_saves_complete)
                    && self
                        .generation()
                        .with_chunks_mut(|chunks| chunks.remove_unloaded_holder(pos).is_some())
                {
                    self.generation()
                        .with_world_mut(|world| world.forget_proto_holder(pos));
                }
            } else if self
                .generation()
                .with_world(|world| world.light_ready.contains(&pos) && !world.unload_pending(pos))
            {
                let snapshot = self.capture_chunk_for_save(pos, self.game_time)?;
                self.generation().with_world_mut(|world| {
                    world.schedule_unload_snapshot(pos, state.0, snapshot)
                })?;
            }
        }
        Ok(())
    }
}
