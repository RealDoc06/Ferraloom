//! SetBlockCommand, FillCommand and BlockInput#place; the host owns canonical level access.
use super::{
    PreparedCommand,
    arguments::{ArgumentValue, blocks::BlockInput},
    builtins::{self, CommandResult},
    exceptions::error,
};
use crate::level::{Level, LevelError};
use ferraloom_brigadier::CommandSyntaxException;
use ferraloom_data::{Block, BlockState};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::chat::Component;
use ferraloom_util::BlockPos;
use std::fmt;

#[derive(Debug)]
pub enum PlacementError {
    Syntax(CommandSyntaxException),
    Level(LevelError),
    Serialization(ferraloom_protocol::Error),
}
impl fmt::Display for PlacementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(error) => error.fmt(f),
            Self::Level(error) => error.fmt(f),
            Self::Serialization(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for PlacementError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Syntax(error) => error,
            Self::Level(error) => error,
            Self::Serialization(error) => error,
        })
    }
}
impl From<ferraloom_protocol::Error> for PlacementError {
    fn from(error: ferraloom_protocol::Error) -> Self {
        Self::Serialization(error)
    }
}
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
    fn block_entity_data(&mut self, pos: BlockPos) -> Result<Option<Tag>, PlacementError>;
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
    fn load_block_entity_data(&mut self, pos: BlockPos, tag: &Tag) -> Result<bool, PlacementError>;
}
/// Port of BlockInWorld#getEntity and BlockInput#test's full-metadata save.
pub fn block_entity_data(level: &mut Level, pos: BlockPos) -> Result<Option<Tag>, PlacementError> {
    level
        .get_or_promote_block_entity(pos)?
        .map(|handle| {
            Ok(Tag::Compound(
                level
                    .block_entity_state(handle)?
                    .save_with_full_metadata()?,
            ))
        })
        .transpose()
}

/// Port of BlockInput#place's NBT branch. The supplied tag is loaded as-is;
/// metadata does not mutate the receiver's identity or position. Retain the
/// original generation across class callbacks before comparing or notifying.
pub fn load_block_entity_data(
    level: &mut Level,
    pos: BlockPos,
    tag: &CompoundTag,
) -> Result<bool, PlacementError> {
    let Some(handle) = level.get_or_promote_block_entity(pos)? else {
        return Ok(false);
    };
    level.retain_block_entity(handle)?;
    let result = (|| {
        let before = level.block_entity_state(handle)?.save_without_metadata()?;
        handle.load_with_components(tag, level)?;
        let after = level.block_entity_state(handle)?.save_without_metadata()?;
        let changed = after != before;
        if changed {
            handle.set_changed(level)?;
            level.block_changed(pos);
        }
        Ok(changed)
    })();
    level.release_block_entity(handle);
    result
}

impl BlockInput {
    /// Port of BlockInput#place, including defined-property restoration and NBT-only changes.
    pub fn place<A: PlacementAccess>(
        &self,
        access: &mut A,
        pos: BlockPos,
        flags: u32,
    ) -> Result<bool, PlacementError> {
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
        return Err(super::exceptions::error_with_arguments(
            "commands.fill.toobig",
            vec![builtins::integer(limit), builtins::long(area)],
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{CommandSourceStack, Commands};
    use ferraloom_util::Vec3;
    use std::collections::BTreeMap;
    #[derive(serde::Deserialize)]
    struct Vector {
        command: String,
        limit: i32,
        debug: bool,
        shape: i32,
        trace: serde_json::Value,
        blocks: serde_json::Value,
        feedback: serde_json::Value,
        value: Option<i32>,
        error: Option<String>,
        error_component: Option<serde_json::Value>,
    }
    #[derive(Default)]
    struct World {
        blocks: BTreeMap<(i32, i32, i32), BlockState>,
        trace: Vec<serde_json::Value>,
        limit: i32,
        debug: bool,
        shape: Option<BlockState>,
    }
    fn key(pos: BlockPos) -> (i32, i32, i32) {
        (pos.x, pos.y, pos.z)
    }
    impl World {
        fn record(
            &mut self,
            kind: &str,
            pos: BlockPos,
            state: Option<BlockState>,
            flags: Option<u32>,
        ) {
            let mut value = serde_json::json!({"call":kind,"pos":[pos.x,pos.y,pos.z]});
            if let Some(state) = state {
                value["state"] = serde_json::json!(state.id());
            }
            if let Some(flags) = flags {
                value["flags"] = serde_json::json!(flags);
            }
            self.trace.push(value);
        }
    }
    impl PlacementAccess for World {
        fn is_debug(&self) -> bool {
            self.debug
        }
        fn max_block_modifications(&self) -> i32 {
            self.limit
        }
        fn has_chunk_at(&self, _pos: BlockPos) -> bool {
            true
        }
        fn in_world_bounds(&self, _pos: BlockPos) -> bool {
            true
        }
        fn block_state(&mut self, pos: BlockPos) -> Result<BlockState, LevelError> {
            Ok(self
                .blocks
                .get(&key(pos))
                .copied()
                .unwrap_or(Block::AIR.default_state()))
        }
        fn block_entity_data(&mut self, pos: BlockPos) -> Result<Option<Tag>, PlacementError> {
            self.record("nbt", pos, None, None);
            Ok(None)
        }
        fn set_block(
            &mut self,
            pos: BlockPos,
            state: BlockState,
            flags: u32,
        ) -> Result<bool, LevelError> {
            self.record("set", pos, Some(state), Some(flags));
            let old = self.block_state(pos)?;
            if state.is_air() {
                self.blocks.remove(&key(pos));
            } else {
                self.blocks.insert(key(pos), state);
            }
            Ok(old != state)
        }
        fn destroy_block(&mut self, pos: BlockPos, drop: bool) -> Result<bool, LevelError> {
            assert!(drop);
            self.record("destroy", pos, None, None);
            Ok(self.blocks.remove(&key(pos)).is_some())
        }
        fn update_from_neighbor_shapes(
            &mut self,
            state: BlockState,
            pos: BlockPos,
        ) -> Result<BlockState, LevelError> {
            self.record("shapes", pos, Some(state), None);
            Ok(self.shape.unwrap_or(state))
        }
        fn update_neighbors_on_block_set(
            &mut self,
            pos: BlockPos,
            old: BlockState,
        ) -> Result<(), LevelError> {
            self.record("neighbors", pos, Some(old), None);
            Ok(())
        }
        fn load_block_entity_data(
            &mut self,
            pos: BlockPos,
            _tag: &Tag,
        ) -> Result<bool, PlacementError> {
            self.record("load_nbt", pos, None, None);
            Ok(false)
        }
    }
    #[test]
    fn placement_modes_call_order_flags_mutations_and_feedback_match_java() {
        let vectors: Vec<Vector> =
            serde_json::from_str(include_str!("vanilla/placement-vectors.json")).unwrap();
        let commands = Commands::default();
        let source = CommandSourceStack::server(4, Vec3::ZERO);
        let mut world = World::default();
        for vector in vectors {
            world.trace.clear();
            world.limit = vector.limit;
            world.debug = vector.debug;
            world.shape = u16::try_from(vector.shape)
                .ok()
                .and_then(BlockState::from_id);
            let command = commands.prepare(&vector.command, source.clone()).unwrap();
            match (run(&command, &mut world), vector.error) {
                (Err(PlacementError::Syntax(error)), Some(expected)) => {
                    assert_eq!(error.raw_message.text(), expected, "{}", vector.command);
                    assert_eq!(
                        serde_json::from_str::<serde_json::Value>(
                            &super::super::exceptions::error_component(&error)
                                .to_json()
                                .unwrap()
                        )
                        .unwrap(),
                        vector.error_component.unwrap(),
                        "{}",
                        vector.command
                    );
                }
                (Ok(result), None) => {
                    assert_eq!(Some(result.value), vector.value, "{}", vector.command);
                    let feedback=result.feedback.into_iter().map(|feedback|serde_json::json!({
                        "broadcast":feedback.broadcast,
                        "message":serde_json::from_str::<serde_json::Value>(&feedback.message.to_json().unwrap()).unwrap(),
                    })).collect::<Vec<_>>();
                    assert_eq!(
                        serde_json::json!(feedback),
                        vector.feedback,
                        "{}",
                        vector.command
                    );
                }
                (result, error) => panic!(
                    "{}: unexpected {result:?}, expected error {error:?}",
                    vector.command
                ),
            }
            assert_eq!(
                serde_json::json!(world.trace),
                vector.trace,
                "{}",
                vector.command
            );
            let blocks = world
                .blocks
                .iter()
                .map(|((x, y, z), state)| serde_json::json!({"pos":[x,y,z],"state":state.id()}))
                .collect::<Vec<_>>();
            assert_eq!(
                serde_json::json!(blocks),
                vector.blocks,
                "{}",
                vector.command
            );
        }
    }
}

#[cfg(test)]
#[path = "placement/block_entity_tests.rs"]
mod block_entity_tests;
