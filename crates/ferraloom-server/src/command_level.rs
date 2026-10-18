//! Exclusive adapters for level commands. A required level failure escapes command feedback.
use super::*;
use ferraloom_game::{
    block::{UPDATE_LIMIT, UnportedOperation, UpdateFlags},
    commands::{
        PreparedCommand,
        placement::{self, PlacementAccess, PlacementError},
        reload::{self, ReloadAccess, ReloadRequest},
    },
    gamerules::GameRule,
    level::Level,
};
use ferraloom_nbt::Tag;

#[derive(Debug)]
pub(super) struct CommandLevelFailure(pub LevelError);
impl std::fmt::Display for CommandLevelFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Required command level operation failed: {:?}",
            self.0
        )
    }
}
impl std::error::Error for CommandLevelFailure {}

struct PlacementLevel<'a> {
    level: &'a mut Level,
    max_blocks: i32,
}
impl PlacementAccess for PlacementLevel<'_> {
    fn is_debug(&self) -> bool {
        self.level.rules().debug
    }
    fn max_block_modifications(&self) -> i32 {
        self.max_blocks
    }
    fn has_chunk_at(&self, pos: BlockPos) -> bool {
        self.level.chunk(ChunkPos::containing(pos)).is_some()
    }
    fn in_world_bounds(&self, pos: BlockPos) -> bool {
        self.level.is_in_valid_bounds(pos)
    }
    fn block_state(&mut self, pos: BlockPos) -> Result<ferraloom_data::BlockState, LevelError> {
        self.level.get_block_state(pos)
    }
    fn block_entity_data(&mut self, pos: BlockPos) -> Result<Option<Tag>, LevelError> {
        let state = self.level.get_block_state(pos)?;
        if state.has_block_entity() {
            Err(LevelError::UnsupportedBlockEntity(state.block()))
        } else {
            Ok(None)
        }
    }
    fn set_block(
        &mut self,
        pos: BlockPos,
        state: ferraloom_data::BlockState,
        flags: u32,
    ) -> Result<bool, LevelError> {
        self.level
            .set_block(pos, state, UpdateFlags(flags), UPDATE_LIMIT)
    }
    fn destroy_block(&mut self, pos: BlockPos, drop: bool) -> Result<bool, LevelError> {
        self.level.destroy_block(pos, drop, UPDATE_LIMIT)
    }
    fn update_from_neighbor_shapes(
        &mut self,
        state: ferraloom_data::BlockState,
        pos: BlockPos,
    ) -> Result<ferraloom_data::BlockState, LevelError> {
        self.level.update_from_neighbor_shapes(state, pos)
    }
    fn update_neighbors_on_block_set(
        &mut self,
        pos: BlockPos,
        old: ferraloom_data::BlockState,
    ) -> Result<(), LevelError> {
        self.level.update_neighbors_on_block_set(pos, old)
    }
    fn load_block_entity_data(&mut self, pos: BlockPos, _: &Tag) -> Result<bool, LevelError> {
        let state = self.level.get_block_state(pos)?;
        if state.has_block_entity() {
            Err(LevelError::UnsupportedBlockEntity(state.block()))
        } else {
            Ok(false)
        }
    }
}
struct ReloadHost<'a> {
    server: &'a mut MinecraftServer,
    connection: Option<ConnectionId>,
}
impl ReloadAccess for ReloadHost<'_> {
    type Error = CommandLevelFailure;
    fn send_success(
        &mut self,
        source: &CommandSourceStack,
        feedback: ferraloom_game::commands::builtins::Feedback,
    ) {
        self.server.command_success(
            self.connection,
            source,
            feedback.message,
            feedback.broadcast,
        );
    }
    fn request_reload(&mut self, _: ReloadRequest) -> Result<(), Self::Error> {
        self.server
            .level
            .level
            .unported_operation(UnportedOperation::PackReload)
            .map_err(CommandLevelFailure)
    }
}
impl MinecraftServer {
    /// Runs at the current command queue entry, before another branch can observe the level.
    pub(super) async fn run_level_command(
        &mut self,
        source: Option<ConnectionId>,
        command: &PreparedCommand,
    ) -> Result<bool> {
        match command.command.as_str() {
            "setblock" | "fill" => {
                let result = placement::run(
                    command,
                    &mut PlacementLevel {
                        level: &mut self.level.level,
                        max_blocks: self.game_rules.integer(GameRule::MaxBlockModifications),
                    },
                )
                .map_err(|error| match error {
                    PlacementError::Syntax(error) => anyhow::Error::new(error),
                    PlacementError::Level(error) => anyhow::Error::new(CommandLevelFailure(error)),
                });
                self.apply_level_effects()?;
                self.apply_command_result(source, &command.source, result?);
            }
            "time" => {
                let (result, changed) = self.clocks.command(command, self.level.game_time);
                if let Some(clock) = changed {
                    self.level
                        .level
                        .refresh_environment_attributes(
                            super::command_runtime::environment_clock_snapshot(&self.clocks),
                        )
                        .map_err(CommandLevelFailure)?;
                    self.broadcast(self.clock_packet(Some(&clock)));
                }
                self.apply_command_result(source, &command.source, result?);
            }
            "tick" => {
                let result = ferraloom_game::commands::tick::run(command, self);
                self.apply_tick_rate_events()?;
                self.apply_command_result(source, &command.source, result?);
            }
            "teleport" => {
                let result = self.teleport_command(command)?;
                self.apply_command_result(source, &command.source, result);
            }
            "weather" => {
                let result = ferraloom_game::commands::weather::run(command, &mut self.level.level);
                self.apply_command_result(source, &command.source, result);
            }
            "forceload" => {
                let output =
                    ferraloom_game::commands::forceload::run(command, &mut ForcedChunks(self))
                        .await?;
                self.apply_level_effects()?;
                if let Some(message) = output.failure {
                    self.command_failure(source, &command.source, message);
                }
                self.apply_command_result(source, &command.source, output.result);
            }
            "reload" => {
                let value = reload::run(
                    command.source.clone(),
                    &mut ReloadHost {
                        server: self,
                        connection: source,
                    },
                )?;
                command.source.callback.on_success(value);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

impl MinecraftServer {
    /// Shared completion points for polling and synchronous command chunk requests.
    pub(super) fn integrate_command_terrain(
        &mut self,
        request: u64,
        terrain: crate::world::Terrain,
    ) -> Result<()> {
        let pos = terrain.pos;
        if pos == ChunkPos::containing(self.world.spawn)
            && !self.world.level_data.data.contains_key("spawn")
        {
            self.world.spawn.y = terrain.surface
                [((self.world.spawn.z & 15) * 16 + (self.world.spawn.x & 15)) as usize];
        }
        let chunk = self.world.integrate(request, terrain)?;
        self.level
            .level
            .add_chunk(chunk)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        self.level
            .level
            .prime_chunk_heightmaps(pos)
            .map_err(game_error)?;
        self.level
            .chunks
            .holders
            .get_mut(&pos.pack())
            .unwrap()
            .status = ChunkStatus::InitializeLight;
        Ok(())
    }
    pub(super) fn integrate_command_light(&mut self, light: crate::world::LitChunk) -> Result<()> {
        if let Some((request, pos)) = light.initialized {
            let holder = self
                .level
                .chunks
                .holders
                .get_mut(&pos.pack())
                .context("unknown light job")?;
            ensure!(holder.request_id == request, "stale light completion");
            holder.status = ChunkStatus::Full;
            self.world.ready.insert(pos);
        }
        for (key, data) in light.updates {
            self.level
                .level
                .set_light_snapshot(ChunkPos::unpack(key), data.snapshot());
            self.world.lights.insert(key, data);
        }
        for (key, mask) in light.dirty {
            self.world.light_chunks_to_broadcast.insert(key);
            let dirty = self
                .world
                .pending_light_updates
                .entry(ChunkPos::unpack(key))
                .or_default();
            dirty.sky |= mask.sky;
            dirty.block |= mask.block;
        }
        Ok(())
    }
    async fn command_get_chunk(&mut self, position: ChunkPos) -> Result<()> {
        self.request_area(position, 0)?;
        loop {
            self.integrate_jobs()?;
            if self.world.ready.contains(&position) {
                return Ok(());
            }
            if let Some((&request, _)) = self.world.pending.first_key_value() {
                // Preserve the canonical request order even if a later job finished first.
                let terrain = self.world.pending.remove(&request).unwrap().await??;
                self.integrate_command_terrain(request, terrain)?;
            } else {
                let light = self
                    .world
                    .light_rx
                    .recv()
                    .await
                    .context("chunk light worker stopped")?;
                self.integrate_command_light(light)?;
            }
        }
    }
    pub(super) async fn prepare_command_tickets(&mut self) -> Result<()> {
        self.level.tickets.activate_all_deactivated_tickets();
        let positions = self
            .level
            .get_force_loaded_chunks()
            .iter()
            .copied()
            .map(ChunkPos::unpack)
            .collect::<Vec<_>>();
        for position in positions {
            self.command_get_chunk(position).await?;
        }
        Ok(())
    }
    /// Capture owned save data before constructing IO, keeping the live level's
    /// single-owner lazy caches out of the Send future.
    pub(super) fn save_command_tickets(
        &self,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static {
        let storage =
            ferraloom_storage::SavedDataStorage::new(self.world.access.path().join("data"));
        let mut data = ferraloom_storage::SavedData::new(self.level.tickets.clone());
        async move {
            tokio::task::spawn_blocking(move || {
                storage.save(
                    ferraloom_game::level::tickets::TicketStorage::STORAGE_ID,
                    &mut data,
                )
            })
            .await??;
            Ok(())
        }
    }
}
pub(super) fn load_command_tickets(level: &mut ServerLevel, world: &World) -> Result<()> {
    use ferraloom_game::level::tickets::TicketStorage;
    let storage = ferraloom_storage::SavedDataStorage::new(world.access.path().join("data"));
    if let Some(data) = storage.load::<Tag>(TicketStorage::STORAGE_ID)? {
        let decoded = TicketStorage::decode_partial(data.value());
        if let Some(error) = decoded.error {
            eprintln!("Failed to parse saved data for minecraft:chunk_tickets: {error}");
        }
        level.tickets = decoded.value;
    }
    Ok(())
}
struct ForcedChunks<'a>(&'a mut MinecraftServer);
impl ferraloom_game::commands::forceload::ForceLoadAccess for ForcedChunks<'_> {
    type Error = anyhow::Error;
    fn forced_chunks(&self) -> Vec<ChunkPos> {
        self.0
            .level
            .get_force_loaded_chunks()
            .iter()
            .copied()
            .map(ChunkPos::unpack)
            .collect()
    }
    async fn set_chunk_forced(&mut self, position: ChunkPos, forced: bool) -> Result<bool> {
        let changed = self.0.level.set_chunk_forced(position, forced);
        if forced && changed {
            self.0.command_get_chunk(position).await?;
        }
        Ok(changed)
    }
}

impl ferraloom_game::commands::tick::TickRateAccess for MinecraftServer {
    fn snapshot(&self) -> ferraloom_game::commands::tick::TickRateSnapshot {
        let manager = &self.tick_rate_manager;
        ferraloom_game::commands::tick::TickRateSnapshot {
            rate: manager.tickrate(),
            nanoseconds_per_tick: manager.nanoseconds_per_tick(),
            average_tick_time_nanos: self.average_tick_time_nanos(),
            tick_times_nanos: *self.tick_times_nanos(),
            frozen: manager.is_frozen(),
            stepping: manager.is_stepping_forward(),
            sprinting: manager.is_sprinting(),
        }
    }
    fn set_tick_rate(&mut self, rate: f32) {
        self.tick_rate_manager.set_tick_rate(rate);
    }
    fn set_frozen(&mut self, frozen: bool) {
        self.tick_rate_manager.set_frozen(frozen);
    }
    fn step_game_if_paused(&mut self, ticks: i32) -> bool {
        self.tick_rate_manager.step_game_if_paused(ticks)
    }
    fn stop_stepping(&mut self) -> bool {
        self.tick_rate_manager.stop_stepping()
    }
    fn request_game_to_sprint(&mut self, ticks: i32) -> bool {
        self.tick_rate_manager.request_game_to_sprint(ticks)
    }
    fn stop_sprinting(&mut self) -> bool {
        self.tick_rate_manager.stop_sprinting()
    }
}
