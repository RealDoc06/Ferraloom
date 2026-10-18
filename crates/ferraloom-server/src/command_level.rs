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
                })?;
                self.apply_command_result(source, &command.source, result);
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
