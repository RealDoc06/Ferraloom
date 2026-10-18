//! SetBlockCommand, FillCommand and BlockInput#place; the host owns canonical level access.
use super::{
    PreparedCommand,
    arguments::{ArgumentValue, blocks::BlockInput},
    builtins::{self, CommandResult},
    exceptions::error,
};
use crate::level::LevelError;
use ferraloom_brigadier::CommandSyntaxException;
use ferraloom_data::{Block, BlockState};
use ferraloom_nbt::Tag;
use ferraloom_protocol::chat::Component;
use ferraloom_util::BlockPos;
use std::fmt;

#[derive(Debug)]
pub enum PlacementError {
    Syntax(CommandSyntaxException),
    Level(LevelError),
}
impl fmt::Display for PlacementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(error) => error.fmt(f),
            Self::Level(error) => write!(f, "{error:?}"),
        }
    }
}
impl std::error::Error for PlacementError {}
impl From<LevelError> for PlacementError {
    fn from(error: LevelError) -> Self {
        Self::Level(error)
    }
}
impl From<CommandSyntaxException> for PlacementError {
    fn from(error: CommandSyntaxException) -> Self {
        Self::Syntax(error)
    }
}
pub trait PlacementAccess {
    fn is_debug(&self) -> bool;
    fn max_block_modifications(&self) -> i32;
    fn has_chunk_at(&self, pos: BlockPos) -> bool;
    fn in_world_bounds(&self, pos: BlockPos) -> bool;
    fn block_state(&mut self, pos: BlockPos) -> Result<BlockState, LevelError>;
    fn block_entity_data(&mut self, pos: BlockPos) -> Result<Option<Tag>, LevelError>;
    fn set_block(
        &mut self,
        pos: BlockPos,
        state: BlockState,
        flags: u32,
    ) -> Result<bool, LevelError>;
    fn destroy_block(&mut self, pos: BlockPos, drop: bool) -> Result<bool, LevelError>;
    fn update_from_neighbor_shapes(
        &mut self,
        state: BlockState,
        pos: BlockPos,
    ) -> Result<BlockState, LevelError>;
    fn update_neighbors_on_block_set(
        &mut self,
        pos: BlockPos,
        old: BlockState,
    ) -> Result<(), LevelError>;
    /// Load components, compare saveWithoutMetadata before/after, and mark changed on mutation.
    fn load_block_entity_data(&mut self, pos: BlockPos, tag: &Tag) -> Result<bool, LevelError>;
}
impl BlockInput {
    /// Port of BlockInput#place, including defined-property restoration and NBT-only changes.
    pub fn place<A: PlacementAccess>(
        &self,
        access: &mut A,
        pos: BlockPos,
        flags: u32,
    ) -> Result<bool, LevelError> {
        let mut state = if flags & 16 != 0 {
            self.state
        } else {
            access.update_from_neighbor_shapes(self.state, pos)?
        };
        if state.is_air() {
            state = self.state;
        }
        if state != self.state {
            let properties = state
                .properties()
                .map(|(name, value)| {
                    (
                        name.to_owned(),
                        self.properties
                            .get(name)
                            .cloned()
                            .unwrap_or_else(|| value.to_owned()),
                    )
                })
                .collect::<Vec<_>>();
            state = BlockState::from_properties(
                state.block(),
                properties
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_str())),
            )
            .expect("defined properties remain valid after shape updates");
        }
        let mut affected = access.set_block(pos, state, flags)?;
        if let Some(tag) = &self.nbt {
            affected |= access.load_block_entity_data(pos, tag)?;
        }
        Ok(affected)
    }
}
fn position<A: PlacementAccess>(
    command: &PreparedCommand,
    name: &str,
    access: &A,
) -> Result<BlockPos, CommandSyntaxException> {
    let value = command
        .coordinates(name)
        .expect("block position argument")
        .position(&command.source, None);
    let pos = BlockPos::new(
        value.x.floor() as i32,
        value.y.floor() as i32,
        value.z.floor() as i32,
    );
    if !access.has_chunk_at(pos) {
        Err(error("argument.pos.unloaded", &[]))
    } else if !access.in_world_bounds(pos) {
        Err(error("argument.pos.outofworld", &[]))
    } else {
        Ok(pos)
    }
}
pub fn run<A: PlacementAccess>(
    command: &PreparedCommand,
    access: &mut A,
) -> Result<CommandResult, PlacementError> {
    let Some(ArgumentValue::Block(block)) = command.argument("block") else {
        unreachable!()
    };
    if command.command == "setblock" {
        set_block(command, block, access)
    } else {
        fill(command, block, access)
    }
}
fn set_block<A: PlacementAccess>(
    command: &PreparedCommand,
    block: &BlockInput,
    access: &mut A,
) -> Result<CommandResult, PlacementError> {
    let pos = position(command, "pos", access)?;
    if access.is_debug() {
        return Err(error("commands.setblock.failed", &[]).into());
    }
    let mode = command
        .literals
        .get(1)
        .map(String::as_str)
        .unwrap_or("replace");
    if mode == "keep" && !access.block_state(pos)?.is_air() {
        return Err(error("commands.setblock.failed", &[]).into());
    }
    let needed = if mode == "destroy" {
        access.destroy_block(pos, true)?;
        !block.state.is_air() || !access.block_state(pos)?.is_air()
    } else {
        true
    };
    let old = access.block_state(pos)?;
    let strict = mode == "strict";
    if needed && !block.place(access, pos, if strict { 818 } else { 258 })? {
        return Err(error("commands.setblock.failed", &[]).into());
    }
    if !strict {
        access.update_neighbors_on_block_set(pos, old)?;
    }
    Ok(CommandResult::success(
        1,
        Component::translatable(
            "commands.setblock.success",
            vec![
                builtins::integer(pos.x),
                builtins::integer(pos.y),
                builtins::integer(pos.z),
            ],
        ),
        true,
    ))
}
fn fill<A: PlacementAccess>(
    command: &PreparedCommand,
    block: &BlockInput,
    access: &mut A,
) -> Result<CommandResult, PlacementError> {
    let from = position(command, "from", access)?;
    let to = position(command, "to", access)?;
    let min = BlockPos::new(from.x.min(to.x), from.y.min(to.y), from.z.min(to.z));
    let max = BlockPos::new(from.x.max(to.x), from.y.max(to.y), from.z.max(to.z));
    let area = i64::from(max.x - min.x + 1)
        .wrapping_mul(i64::from(max.y - min.y + 1))
        .wrapping_mul(i64::from(max.z - min.z + 1));
    let limit = access.max_block_modifications();
    if area > i64::from(limit) {
        return Err(error(
            "commands.fill.toobig",
            &[limit.to_string(), area.to_string()],
        )
        .into());
    }
    if access.is_debug() {
        return Err(error("commands.fill.failed", &[]).into());
    }
    let strict = command.literals.iter().any(|literal| literal == "strict");
    let destroy = command.literals.iter().any(|literal| literal == "destroy");
    let outline = command.literals.iter().any(|literal| literal == "outline");
    let hollow = command.literals.iter().any(|literal| literal == "hollow");
    let keep = command.literals.iter().any(|literal| literal == "keep");
    let filter = match command.argument("filter") {
        Some(ArgumentValue::BlockPredicate(filter)) => Some(filter),
        _ => None,
    };
    let air = BlockInput {
        state: Block::AIR.default_state(),
        properties: Default::default(),
        nbt: None,
    };
    let mut updates = Vec::new();
    let mut count = 0;
    // BlockPos.betweenClosed advances X first, then Y, then Z.
    for z in min.z..=max.z {
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let pos = BlockPos::new(x, y, z);
                if keep && !access.block_state(pos)?.is_air() {
                    continue;
                }
                if let Some(filter) = filter {
                    let state = access.block_state(pos)?;
                    if !filter.matches_state(state) {
                        continue;
                    }
                    let nbt = if filter.requires_nbt() {
                        access.block_entity_data(pos)?
                    } else {
                        None
                    };
                    if !filter.matches(state, nbt.as_ref()) {
                        continue;
                    }
                }
                let old = access.block_state(pos)?;
                let affected = destroy && access.destroy_block(pos, true)?;
                let inside = x != min.x
                    && x != max.x
                    && y != min.y
                    && y != max.y
                    && z != min.z
                    && z != max.z;
                if outline && inside {
                    count += i32::from(affected);
                    continue;
                }
                let target = if hollow && inside { &air } else { block };
                if target.place(access, pos, if strict { 818 } else { 258 })? {
                    if !strict {
                        updates.push((pos, old));
                    }
                    count += 1;
                } else {
                    count += i32::from(affected);
                }
            }
        }
    }
    for (pos, old) in updates {
        access.update_neighbors_on_block_set(pos, old)?;
    }
    if count == 0 {
        return Err(error("commands.fill.failed", &[]).into());
    }
    Ok(CommandResult::success(
        count,
        Component::translatable("commands.fill.success", vec![builtins::integer(count)]),
        true,
    ))
}
