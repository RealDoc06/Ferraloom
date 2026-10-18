//! Canonical promotion and lighting completion, shared by ticks and commands.
use super::*;
use crate::world::{LitChunk, Terrain};

pub(crate) fn integrate_metadata(level: &mut ServerLevel, world: &mut World) -> Result<()> {
    level
        .level
        .record_worldgen_skips(std::mem::take(&mut world.generation_skips))
        .map_err(game_error)?;
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

pub(crate) fn integrate_terrain(
    level: &mut ServerLevel,
    world: &mut World,
    request: u64,
    mut terrain: Terrain,
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
    let entities = terrain.disk.entities.take().unwrap_or_default();
    if let Err(error) = level
        .level
        .add_worldgen_chunk_entities(terrain.pos, entities)
    {
        terrain.disk.entities = Some(error.entities);
        world.saved.insert(terrain.pos.pack(), terrain.disk);
        return Err(error.source.into());
    }
    let pos = terrain.pos;
    if pos == ChunkPos::containing(world.spawn) && !world.level_data.data.contains_key("spawn") {
        world.spawn.y =
            terrain.surface[((world.spawn.z & 15) * 16 + (world.spawn.x & 15)) as usize];
    }
    let chunk = world.integrate(request, terrain)?;
    level
        .level
        .add_chunk(chunk)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    level
        .level
        .prime_chunk_heightmaps(pos)
        .map_err(game_error)?;
    level.chunks.holders.get_mut(&pos.pack()).unwrap().status = ChunkStatus::Full;
    Ok(())
}

fn integrate_light(level: &mut ServerLevel, world: &mut World, light: LitChunk) -> Result<()> {
    if let Some((request, pos)) = light.initialized {
        let holder = level
            .chunks
            .holders
            .get_mut(&pos.pack())
            .context("unknown light job")?;
        ensure!(holder.request_id == request, "stale light completion");
        holder.status = ChunkStatus::Full;
        world.light_ready.insert(pos);
    }
    for (key, data) in light.updates {
        level
            .level
            .set_light_snapshot(ChunkPos::unpack(key), data.snapshot());
        world.lights.insert(key, data);
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

pub(crate) fn finish_promotions(level: &mut ServerLevel, world: &mut World) -> Result<()> {
    let positions = level.chunks.holder_positions().collect::<Vec<_>>();
    for pos in positions {
        if level.chunks.holders[&pos.pack()].requested_full_status()
            >= FullChunkStatus::BlockTicking
            && !world.ready.contains(&pos)
            && world.light_ready.contains(&pos)
            && full_chunk_range_ready(&world.light_ready, pos, 1)
        {
            level
                .level
                .post_process_generation(pos)
                .map_err(game_error)?;
            level.level.start_ticking_chunk(pos).map_err(game_error)?;
            world.ready.insert(pos);
        }
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
            self.integrate_jobs()?;
            self.update_chunk_tickets()?;
            if self.world.ready.contains(&position) {
                return Ok(());
            }
            // Park between owned worker polls without advancing simulation ticks.
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
}
