//! ChunkStatusTasks#full's main-owner continuation. No backend guard crosses a
//! Level callback; nested polls can execute later, still-reachable work.
use super::{GenerationAccess, GenerationRuntime};
use crate::{
    generation::GenerationIdentity,
    server::generation_completion::PromotionContinuation,
    world::{LitChunk, Terrain},
};
use anyhow::{Context, Result, ensure};
use ferraloom_game::level::{
    Level, LevelError,
    acquisition::{AcquireFailure, ChunkKind},
    generation::ChunkStatus,
    publication::PreparedFullChunk,
};
use ferraloom_util::ChunkPos;

impl GenerationRuntime {
    pub(super) fn begin_full_promotion(&self, task: u64, mut terrain: Terrain) -> Result<()> {
        let identity = {
            let chunks = self.chunks.borrow();
            ensure!(
                self.jobs.borrow().is_current(&chunks, task),
                "stale FULL task"
            );
            GenerationIdentity::of(&chunks.holders[&terrain.pos.pack()])
        };
        let mut world = self.world.borrow_mut();
        ensure!(
            !world.full_promotions.contains_key(&task),
            "duplicate FULL promotion"
        );
        let light_revision = terrain.revision;
        let chunk = world.integrate(task, light_revision, &mut terrain);
        let (prepared_chunk, error) = match chunk {
            Ok(chunk) => (Some(chunk), None),
            Err(error) => (None, Some(error)),
        };
        world.full_promotions.insert(
            task,
            PromotionContinuation {
                identity,
                input: Some(terrain),
                light_revision,
                prepared_chunk,
                entities_registered: false,
                entity_failure: None,
                complete: false,
                center_light_installed: false,
                stage: None,
                finalized: false,
                post_load_active: false,
                promotion_active: false,
                publication_failure: None,
                finalization_failure: None,
            },
        );
        error.map_or(Ok(()), Err)
    }

    pub(super) fn reject_reentrant_full(
        &self,
        pos: ChunkPos,
        status: ChunkStatus,
    ) -> Result<(), LevelError> {
        if status == ChunkStatus::Full
            && self
                .world
                .borrow()
                .full_promotions
                .values()
                .any(|continuation| {
                    continuation.identity.pos == pos && continuation.post_load_active
                })
        {
            return Err(LevelError::ChunkAcquisition(
                pos,
                status,
                AcquireFailure::ReentrantFull,
            ));
        }
        Ok(())
    }

    /// Center light is canonical before stage/publication. Deltas retain their
    /// own flow and never initialize a different request or holder incarnation.
    pub(super) fn import_light(&self, level: &Level, mut light: LitChunk) -> Result<()> {
        let initialized = light.initialized.take().filter(|initialized| {
            let world = self.world.borrow();
            let chunks = self.chunks.borrow();
            world
                .full_promotions
                .get(&initialized.request)
                .is_some_and(|continuation| {
                    continuation.identity.pos == initialized.pos
                        && chunks
                            .holders
                            .get(&initialized.pos.pack())
                            .is_some_and(|holder| {
                                holder.epoch == continuation.identity.holder_epoch
                            })
                })
        });
        if let Some(initialized) = &initialized {
            let world = self.world.borrow();
            let continuation = &world.full_promotions[&initialized.request];
            ensure!(
                initialized.revision >= continuation.light_revision,
                "older center light import"
            );
            ensure!(
                light.updates.contains_key(&initialized.pos.pack()),
                "center light import missing snapshot"
            );
        }
        for (key, data) in light.updates {
            level.publish_light_snapshot(ChunkPos::unpack(key), data.snapshot());
            self.world.borrow_mut().lights.insert(key, data);
        }
        let mut world = self.world.borrow_mut();
        if let Some(initialized) = initialized {
            let continuation = world.full_promotions.get_mut(&initialized.request).unwrap();
            continuation.center_light_installed = true;
            if continuation.complete {
                world.light_ready.insert(initialized.pos);
            }
        }
        for (key, mask) in light.dirty {
            world.light_chunks_to_broadcast.insert(key);
            let dirty = world
                .pending_light_updates
                .entry(ChunkPos::unpack(key))
                .or_default();
            dirty.sky |= mask.sky;
            dirty.block |= mask.block;
        }
        Ok(())
    }

    pub(super) fn next_full_promotion(&self) -> Option<u64> {
        self.world
            .borrow()
            .full_promotions
            .iter()
            .find_map(|(&task, continuation)| {
                (!continuation.complete
                    && !continuation.promotion_active
                    && !continuation.post_load_active
                    && continuation.center_light_installed
                    && continuation.entity_failure.is_none()
                    && continuation.publication_failure.is_none()
                    && continuation.finalization_failure.is_none())
                .then_some(task)
            })
    }

    pub(super) fn resume_full_promotion_with_access(
        access: &mut GenerationAccess<'_>,
        task: u64,
    ) -> Result<()> {
        access.read(|runtime, _| -> Result<()> {
            let mut world = runtime.world.borrow_mut();
            let continuation = world.full_promotions.get_mut(&task).unwrap();
            ensure!(!continuation.promotion_active, "recursive FULL task");
            continuation.promotion_active = true;
            runtime.jobs.borrow_mut().begin_integration(task);
            Ok(())
        })?;
        let result = Self::run_full_promotion_with_access(access, task);
        access.read(|runtime, _| runtime.finish_full_promotion_step(task, result.is_err()));
        result
    }
    fn finish_full_promotion_step(&self, task: u64, failed: bool) {
        self.world
            .borrow_mut()
            .full_promotions
            .get_mut(&task)
            .unwrap()
            .promotion_active = false;
        if self.world.borrow().full_promotions[&task].complete {
            self.jobs.borrow_mut().complete_integration(task);
        } else if failed {
            let identity = self.world.borrow().full_promotions[&task].identity;
            if let Some(holder) = self
                .chunks
                .borrow_mut()
                .holders
                .get_mut(&identity.pos.pack())
                && holder.epoch == identity.holder_epoch
            {
                holder.generation.fail_pending(AcquireFailure::Publication);
            }
            self.jobs.borrow_mut().complete_integration(task);
        } else {
            self.jobs.borrow_mut().suspend_integration(task);
        }
        self.release_finished_jobs();
    }

    pub(super) fn stage_full_promotion(
        &self,
        level: &Level,
        task: u64,
    ) -> Result<GenerationIdentity> {
        let (identity, stage) = {
            let world = self.world.borrow();
            let continuation = &world.full_promotions[&task];
            ensure!(!continuation.complete, "FULL promotion already completed");
            ensure!(!continuation.post_load_active, "recursive FULL promotion");
            (continuation.identity, continuation.stage)
        };
        if stage.is_none() {
            let (prepared, requested) = {
                let mut world = self.world.borrow_mut();
                world.refresh_promotion_structures(task)?;
                let chunks = self.chunks.borrow();
                let holder = chunks
                    .holders
                    .get(&identity.pos.pack())
                    .context("missing FULL holder")?;
                ensure!(identity.matches(holder), "stale unstaged FULL continuation");
                let light = world
                    .lights
                    .get(&identity.pos.pack())
                    .context("center light absent after initialization")?
                    .snapshot();
                let continuation = world.full_promotions.get_mut(&task).unwrap();
                let terrain = continuation
                    .input
                    .as_mut()
                    .context("missing FULL terrain")?;
                let mut chunk = continuation
                    .prepared_chunk
                    .take()
                    .context("missing prepared FULL chunk")?;
                if chunk.structures_revision() < terrain.revision.0 {
                    chunk
                        .set_structures(terrain.structures.clone(), terrain.revision.0)
                        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
                }
                (
                    PreparedFullChunk {
                        chunk,
                        light,
                        worldgen_entities: terrain.disk.entities.take().unwrap_or_default(),
                    },
                    holder.requested_full_status(),
                )
            };
            let staged = match level.stage_full_chunk(prepared) {
                Ok(staged) => staged,
                Err(error) => {
                    self.world
                        .borrow_mut()
                        .full_promotions
                        .get_mut(&task)
                        .unwrap()
                        .publication_failure = Some(error);
                    anyhow::bail!("FULL staging failed; prepared payload retained");
                }
            };
            let reference = staged.token.reference();
            {
                let mut world = self.world.borrow_mut();
                let continuation = world.full_promotions.get_mut(&task).unwrap();
                continuation.stage = Some(staged.token);
                continuation.input.as_mut().unwrap().disk.entities = Some(staged.worldgen_entities);
            }
            {
                let mut chunks = self.chunks.borrow_mut();
                let holder = chunks.holders.get_mut(&identity.pos.pack()).unwrap();
                ensure!(
                    holder.epoch == identity.holder_epoch,
                    "FULL holder recreated during staging"
                );
                holder.generation.replace_proto_chunk(reference);
            }
            // Native sets the full-status supplier before runPostLoad. This
            // shared metadata operation does not register/unpack tick queues.
            level
                .set_chunk_loading_status(identity.pos, requested, false)
                .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        }

        Ok(identity)
    }

    fn take_post_load(
        &self,
        task: u64,
    ) -> Result<(GenerationIdentity, Option<Vec<ferraloom_nbt::CompoundTag>>)> {
        let identity = self.world.borrow().full_promotions[&task].identity;
        let entities = {
            let mut world = self.world.borrow_mut();
            let continuation = world.full_promotions.get_mut(&task).unwrap();
            ensure!(
                continuation.entity_failure.is_none(),
                "failed entity postLoad cannot replay"
            );
            if continuation.entities_registered {
                None
            } else {
                continuation.post_load_active = true;
                Some(
                    continuation
                        .input
                        .as_mut()
                        .unwrap()
                        .disk
                        .entities
                        .take()
                        .unwrap_or_default(),
                )
            }
        };
        Ok((identity, entities))
    }
    fn record_post_load(
        &self,
        task: u64,
        result: Result<(), ferraloom_game::level::entity::WorldgenEntityLoadError>,
    ) -> Result<()> {
        let identity = self.world.borrow().full_promotions[&task].identity;
        let mut world = self.world.borrow_mut();
        let continuation = world.full_promotions.get_mut(&task).unwrap();
        continuation.post_load_active = false;
        match result {
            Ok(()) => {
                continuation.entities_registered = true;
                if let Some(disk) = world.saved.get_mut(&identity.pos.pack()) {
                    disk.entities = None;
                }
            }
            Err(error) => {
                let message = error.to_string();
                let original = error.entities.clone();
                continuation.input.as_mut().unwrap().disk.entities = Some(original.clone());
                continuation.entity_failure = Some(error);
                if let Some(disk) = world.saved.get_mut(&identity.pos.pack()) {
                    disk.entities = Some(original);
                }
                anyhow::bail!("{message}; owned entity load error retained, no automatic replay");
            }
        }
        Ok(())
    }
    pub(super) fn run_post_load_with_access(
        access: &mut GenerationAccess<'_>,
        task: u64,
    ) -> Result<()> {
        let (identity, entities) = access.read(|runtime, _| runtime.take_post_load(task))?;
        if let Some(entities) = entities {
            // Capability is already selected. End every runtime/guard borrow
            // before constructors, tracking callbacks or nested chunk reads.
            let result = access.post_load(identity.pos, entities);
            access.read(|runtime, _| runtime.record_post_load(task, result))?;
        }
        Ok(())
    }

    fn run_full_promotion_with_access(access: &mut GenerationAccess<'_>, task: u64) -> Result<()> {
        let identity = access.read(|runtime, level| runtime.stage_full_promotion(level, task))?;
        Self::run_post_load_with_access(access, task)?;
        access.read(|runtime, level| runtime.publish_structure_updates(level))?;
        let (token, finalized) = access.read(|runtime, _| {
            let world = runtime.world.borrow();
            let continuation = &world.full_promotions[&task];
            ensure!(
                continuation.entities_registered,
                "FULL finalization before postLoad"
            );
            Ok::<_, anyhow::Error>((continuation.stage.unwrap(), continuation.finalized))
        })?;
        let reference = if finalized {
            token.reference()
        } else {
            match access.level().finalize_full_chunk(token) {
                Ok(reference) => reference,
                Err(error) => {
                    access.read(|runtime, _| {
                        runtime
                            .world
                            .borrow_mut()
                            .full_promotions
                            .get_mut(&task)
                            .unwrap()
                            .finalization_failure = Some(error);
                    });
                    anyhow::bail!("FULL finalization failed; canonical stage/checkpoint retained");
                }
            }
        };
        ensure!(
            reference.kind == ChunkKind::Full && reference == token.reference(),
            "FULL finalization changed its staged identity"
        );
        access.read(|runtime, _| -> Result<()> {
            // World::open installs the spawn tag before returning, so absent-spawn
            // refinement is already resolved before any FULL promotion.
            {
                let mut world = runtime.world.borrow_mut();
                let continuation = world.full_promotions.get_mut(&task).unwrap();
                continuation.finalized = true;
                continuation.finalization_failure = None;
            }
            {
                let mut chunks = runtime.chunks.borrow_mut();
                let holder = chunks
                    .holders
                    .get_mut(&identity.pos.pack())
                    .context("missing finalized holder")?;
                ensure!(
                    holder.epoch == identity.holder_epoch,
                    "finalized holder recreated"
                );
                if let Some(completed) = holder
                    .generation
                    .chunk_if_present_unchecked(ChunkStatus::Full)
                {
                    ensure!(
                        completed == reference,
                        "FULL future changed its staged identity"
                    );
                } else {
                    holder.generation.acquire_status_bump(ChunkStatus::Full);
                    holder.generation.complete(ChunkStatus::Full, reference);
                }
            }
            {
                let mut world = runtime.world.borrow_mut();
                let continuation = world.full_promotions.get_mut(&task).unwrap();
                continuation.complete = true;
                continuation.input = None;
                world.light_ready.insert(identity.pos);
            }
            Ok(())
        })?;
        Self::confirm_ready_ranges_with_access(access)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        Ok(())
    }
}
