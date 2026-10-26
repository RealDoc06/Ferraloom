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
    fn block_entity_data(&mut self, pos: BlockPos) -> Result<Option<Tag>, PlacementError> {
        placement::block_entity_data(self.level, pos)
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
    fn load_block_entity_data(&mut self, pos: BlockPos, tag: &Tag) -> Result<bool, PlacementError> {
        placement::load_block_entity_data(
            self.level,
            pos,
            tag.as_compound()
                .expect("BlockInput parser supplies a compound"),
        )
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
            "gamerule" => {
                let mut result =
                    ferraloom_game::commands::builtins::gamerule(&mut self.game_rules, command)?;
                if let Some(change) = result.rule_change.take() {
                    let locator = match &change.effect {
                        Some(ferraloom_game::gamerules::GameRuleEffect::LocatorBar { enabled }) => {
                            Some(*enabled)
                        }
                        _ => None,
                    };
                    self.on_game_rule_changed(change);
                    if let Some(enabled) = locator {
                        // MinecraftServer#onGameRuleChanged runs before command feedback.
                        self.level
                            .level
                            .update_locator_bar(enabled)
                            .map_err(CommandLevelFailure)?;
                    }
                }
                self.apply_level_effects()?;
                self.apply_command_result(source, &command.source, result);
            }
            "team" => {
                let result = self.execute_team_command_live(command);
                self.apply_level_effects()?;
                self.apply_command_result(source, &command.source, result?);
            }
            "data" => {
                let result = self.data_command(command)?;
                self.apply_command_result(source, &command.source, result);
            }
            "summon" => {
                let result = self.summon_command(command)?;
                self.apply_command_result(source, &command.source, result);
            }
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
                    PlacementError::Serialization(error) => anyhow::Error::new(error),
                });
                self.apply_level_effects()?;
                self.apply_command_result(source, &command.source, result?);
            }
            "gamemode" => {
                let result = self.game_mode_command(source, command)?;
                self.apply_command_result(source, &command.source, result);
            }
            "defaultgamemode" => {
                let result = self.default_game_mode_command(command)?;
                self.apply_command_result(source, &command.source, result);
            }
            "give" => {
                let result = self.give_command(command)?;
                self.apply_command_result(source, &command.source, result);
            }
            "teleport" => {
                let result = self.teleport_command(command)?;
                self.apply_command_result(source, &command.source, result);
            }
            "time" => {
                let (result, changed) = self.clocks.command(command, self.level.game_time);
                if let Some(clock) = changed {
                    let snapshot = super::command_runtime::environment_clock_snapshot(&self.clocks);
                    for live in self.live_levels_mut() {
                        live.level
                            .refresh_environment_attributes(snapshot.clone())
                            .map_err(CommandLevelFailure)?;
                    }
                    self.broadcast(self.clock_packet(Some(&clock)));
                }
                self.apply_command_result(source, &command.source, result?);
            }
            "tick" => {
                let result = ferraloom_game::commands::tick::run(command, self);
                self.apply_tick_rate_events()?;
                self.apply_command_result(source, &command.source, result?);
            }
            "weather" => {
                let result = ferraloom_game::commands::weather::run(command, &mut self.level.level)
                    .map_err(CommandLevelFailure)?;
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
    async fn command_get_chunk(&mut self, position: ChunkPos) -> Result<()> {
        self.await_generation_chunk(position).await
    }
    pub(super) async fn prepare_command_tickets(&mut self) -> Result<()> {
        self.level
            .generation()
            .with_tickets_mut(|tickets| tickets.activate_all_deactivated_tickets());
        let positions = self.level.generation().with_tickets(|tickets| {
            tickets
                .get_force_loaded_chunks()
                .iter()
                .copied()
                .map(ChunkPos::unpack)
                .collect::<Vec<_>>()
        });
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
        let storage = self.level.generation().with_world(|world| {
            world
                .access
                .dimension(world.dimension_key.as_str())
                .map(|paths| ferraloom_storage::SavedDataStorage::new(paths.data))
        });
        let mut data =
            ferraloom_storage::SavedData::new(self.level.generation().with_tickets(Clone::clone));
        async move {
            let storage = storage?;
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
    let storage = ferraloom_storage::SavedDataStorage::new(
        world.access.dimension(world.dimension_key.as_str())?.data,
    );
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
        self.0.level.generation().with_tickets(|tickets| {
            tickets
                .get_force_loaded_chunks()
                .iter()
                .copied()
                .map(ChunkPos::unpack)
                .collect()
        })
    }
    async fn set_chunk_forced(&mut self, position: ChunkPos, forced: bool) -> Result<bool> {
        let changed = self
            .0
            .level
            .generation()
            .with_tickets_mut(|tickets| tickets.set_chunk_forced(position, forced));
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
