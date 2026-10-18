//! Canonical promotion and lighting completion, shared by ticks and commands.
use super::*;
use crate::world::{LitChunk, Terrain};
use ferraloom_game::level::chunk::LevelChunk;

/// Owned FULL continuation, retained through later publication errors. Entity
/// callbacks commit once; reconstructing a chunk must never replay postLoad.
pub(crate) struct PromotionContinuation {
    pub identity: crate::generation::GenerationIdentity,
    pub input: Option<Terrain>,
    light_revision: crate::generation::GenerationRevision,
    prepared_chunk: Option<LevelChunk>,
    entities_registered: bool,
    entity_failure: Option<String>,
    attached: bool,
    complete: bool,
    center_light_installed: bool,
}

pub(crate) fn integrate_metadata(level: &mut ServerLevel, world: &mut World) -> Result<()> {
    level
        .level
        .record_worldgen_skips(
            std::mem::take(&mut world.generation_skips)
                .into_iter()
                .map(ferraloom_game::block::WorldgenSkip::from)
                .collect(),
        )
        .map_err(game_error)?;
    publish_structure_updates(&level.level, world)?;
    for (&key, &status) in &world.generation_statuses {
        let pos = ChunkPos::unpack(key);
        let attached = level.level.chunk(pos).is_some();
        level.chunks.ensure_holder(pos).status = if status == ChunkStatus::Full && !attached {
            ChunkStatus::Spawn
        } else {
            status
        };
    }
    Ok(())
}
pub(crate) fn publish_structure_updates(
    level: &ferraloom_game::level::Level,
    world: &mut World,
) -> Result<()> {
    while let Some((pos, revision, structures)) = world.next_structure_publication(level) {
        level
            .publish_chunk_structures(pos, revision.0, structures)
            .map_err(game_error)?;
        world.finish_structure_publication(pos, revision);
    }
    Ok(())
}

pub(crate) fn integrate_terrain(
    level: &mut ServerLevel,
    world: &mut World,
    request: u64,
    terrain: Terrain,
) -> Result<()> {
    // A released holder can be recreated at the same position. Neither
    // entity post-load nor FULL attachment may target its newer request.
    if level
        .chunks
        .holders
        .get(&terrain.pos.pack())
        .is_none_or(|holder| holder.request_id != request || !holder.generation_requested)
    {
        return Ok(());
    }
    let identity =
        crate::generation::GenerationIdentity::of(&level.chunks.holders[&terrain.pos.pack()]);
    world
        .full_promotions
        .entry(request)
        .or_insert_with(|| PromotionContinuation {
            identity,
            light_revision: terrain.revision,
            input: Some(terrain),
            prepared_chunk: None,
            entities_registered: false,
            entity_failure: None,
            attached: false,
            complete: false,
            center_light_installed: false,
        });
    resume_terrain(level, world, request)
}

pub(crate) fn resume_terrain(
    level: &mut ServerLevel,
    world: &mut World,
    request: u64,
) -> Result<()> {
    let continuation = world
        .full_promotions
        .get_mut(&request)
        .context("unknown FULL continuation")?;
    let identity = continuation.identity;
    if continuation.complete {
        return Ok(());
    }
    if let Some(failure) = &continuation.entity_failure {
        anyhow::bail!("entity postLoad already failed; original NBT is retained: {failure}");
    }
    ensure!(
        level
            .chunks
            .holders
            .get(&identity.pos.pack())
            .is_some_and(|holder| identity.matches(holder)),
        "stale FULL continuation"
    );
    // The record stays in the backend while its owned action runs. The shared
    // driver will drop its cold guard before executing publication callbacks.
    let mut terrain = continuation
        .input
        .take()
        .context("FULL continuation already executing")?;
    let result = (|| -> Result<()> {
        let pos = identity.pos;
        if !world.full_promotions[&request].attached
            && world.full_promotions[&request].prepared_chunk.is_none()
        {
            let chunk = world.integrate(
                request,
                world.full_promotions[&request].light_revision,
                &mut terrain,
            )?;
            world
                .full_promotions
                .get_mut(&request)
                .unwrap()
                .prepared_chunk = Some(chunk);
        }
        if !world.full_promotions[&request].entities_registered {
            let entities = terrain.disk.entities.take().unwrap_or_default();
            if let Err(error) = level.level.add_worldgen_chunk_entities(pos, entities) {
                terrain.disk.entities = Some(error.entities);
                world.saved.insert(pos.pack(), terrain.disk.clone());
                // The current exclusive API cannot distinguish a factory
                // preflight failure from partial publication. Never replay it.
                world
                    .full_promotions
                    .get_mut(&request)
                    .unwrap()
                    .entity_failure = Some(error.source.to_string());
                return Err(error.source.into());
            }
            world
                .full_promotions
                .get_mut(&request)
                .unwrap()
                .entities_registered = true;
            world.saved.get_mut(&pos.pack()).unwrap().entities = None;
        }
        if !world.full_promotions[&request].attached {
            world.refresh_terrain_structures(&mut terrain);
            let mut chunk = world
                .full_promotions
                .get_mut(&request)
                .unwrap()
                .prepared_chunk
                .take()
                .unwrap();
            if chunk.structures_revision() < terrain.revision.0 {
                chunk
                    .set_structures(terrain.structures.clone(), terrain.revision.0)
                    .map_err(game_error)?;
            }
            level.level.add_chunk(chunk).map_err(game_error)?;
            world.full_promotions.get_mut(&request).unwrap().attached = true;
        }
        if pos == ChunkPos::containing(world.spawn) && !world.level_data.data.contains_key("spawn")
        {
            world.spawn.y =
                terrain.surface[((world.spawn.z & 15) * 16 + (world.spawn.x & 15)) as usize];
        }
        level
            .level
            .prime_chunk_heightmaps(pos)
            .map_err(game_error)?;
        let holder = level
            .chunks
            .holders
            .get_mut(&pos.pack())
            .context("missing FULL holder")?;
        ensure!(identity.matches(holder), "stale FULL publication");
        holder.status = ChunkStatus::Full;
        Ok(())
    })();
    let continuation = world.full_promotions.get_mut(&request).unwrap();
    if result.is_ok() {
        continuation.complete = true;
        if continuation.center_light_installed {
            world.light_ready.insert(identity.pos);
        }
    } else {
        continuation.input = Some(terrain);
    }
    result
}

pub(crate) fn integrate_light(
    level: &mut ServerLevel,
    world: &mut World,
    light: LitChunk,
) -> Result<()> {
    let initialized = light.initialized.filter(|initialized| {
        level
            .chunks
            .holders
            .get(&initialized.pos.pack())
            .is_some_and(|holder| {
                world
                    .full_promotions
                    .get(&initialized.request)
                    .is_some_and(|continuation| {
                        continuation.identity.matches(holder)
                            && continuation.identity.pos == initialized.pos
                    })
            })
    });
    if let Some(initialized) = initialized {
        let continuation = &world.full_promotions[&initialized.request];
        ensure!(
            initialized.revision >= continuation.light_revision,
            "older center light import"
        );
        ensure!(
            light.updates.contains_key(&initialized.pos.pack()),
            "center light import missing its snapshot"
        );
    }
    for (key, data) in light.updates {
        level
            .level
            .set_light_snapshot(ChunkPos::unpack(key), data.snapshot());
        world.lights.insert(key, data);
    }
    // Publish completion only after the canonical center snapshot is installed.
    // LIGHT deltas cannot mark an unfinished or failed FULL publication ready.
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

/// Queue prepareTickingChunk continuations without mutating Level. A managed
/// shared read may progress FULL without executing these exclusive callbacks.
pub(crate) fn queue_ready_promotions(level: &ServerLevel, world: &mut World) {
    let positions = level.chunks.holder_positions().collect::<Vec<_>>();
    for pos in positions {
        if level.chunks.holders[&pos.pack()].requested_full_status()
            >= FullChunkStatus::BlockTicking
            && !world.ready.contains(&pos)
            && world.light_ready.contains(&pos)
            && full_chunk_range_ready(&world.light_ready, pos, 1)
        {
            let identity =
                crate::generation::GenerationIdentity::of(&level.chunks.holders[&pos.pack()]);
            if !world.exclusive_ready_tasks.contains(&identity) {
                world.exclusive_ready_tasks.push_back(identity);
            }
        }
    }
}

/// Exclusive prepareTickingChunk slot. Never called by the shared managed pump;
/// the documented single-region deviation ends at phase-5 continuations.
pub(crate) fn finish_promotions(level: &mut ServerLevel, world: &mut World) -> Result<()> {
    queue_ready_promotions(level, world);
    while let Some(identity) = world.exclusive_ready_tasks.pop_front() {
        let current = level
            .chunks
            .holders
            .get(&identity.pos.pack())
            .is_some_and(|holder| {
                identity.matches(holder)
                    && holder.requested_full_status() >= FullChunkStatus::BlockTicking
            });
        if !current || world.ready.contains(&identity.pos) {
            continue;
        }
        if let Err(error) = level.level.post_process_generation(identity.pos) {
            world.exclusive_ready_tasks.push_front(identity);
            return Err(game_error(error));
        }
        if let Err(error) = level.level.start_ticking_chunk(identity.pos) {
            world.exclusive_ready_tasks.push_front(identity);
            return Err(game_error(error));
        }
        world.ready.insert(identity.pos);
    }
    Ok(())
}

impl MinecraftServer {
    pub(super) fn integrate_generation_terrain(
        &mut self,
        request: u64,
        terrain: Terrain,
    ) -> Result<()> {
        integrate_terrain(&mut self.level, &mut self.world, request, terrain)
    }
    pub(super) fn integrate_generation_light(&mut self, light: LitChunk) -> Result<()> {
        integrate_light(&mut self.level, &mut self.world, light)
    }
    pub(super) async fn await_generation_chunk(&mut self, position: ChunkPos) -> Result<()> {
        // Apply native forced tickets before deciding post-processing eligibility.
        self.update_chunk_tickets()?;
        self.request_area(position, 0)?;
        loop {
            self.progress_chunk_jobs()?;
            // This command caller owns exclusive Level access. A parked shared
            // acquisition must only pump progress and must never run this slot.
            finish_promotions(&mut self.level, &mut self.world)?;
            self.update_chunk_tickets()?;
            if self.world.ready.contains(&position) {
                return Ok(());
            }
            // Park between owned worker polls without advancing simulation ticks.
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
}
