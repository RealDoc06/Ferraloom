//! Port of CloneCommands. Coordinates and dimensions are resolved at execution;
//! the host owns canonical level access throughout the synchronous operation.
use super::{
    builtins::{self, CommandResult},
    exceptions::error,
};
use crate::ticks::TickArea;
use ferraloom_brigadier::CommandSyntaxException;
use ferraloom_data::{Block, BlockState};
use ferraloom_nbt::CompoundTag;
use ferraloom_protocol::{Identifier, chat::Component, component::DataComponentMap};
use ferraloom_util::BlockPos;
use std::{collections::VecDeque, fmt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Force,
    Move,
}

/// A loaded dimension/position pair, captured in native begin/end/destination order.
#[derive(Clone, Debug)]
pub struct Position {
    pub dimension: Identifier,
    pub position: BlockPos,
}

#[derive(Clone, Debug)]
pub struct BlockEntityData {
    pub custom: CompoundTag,
    pub components: DataComponentMap,
}

/// Port of CloneCommands' source block-entity capture. Components remain
/// separate from saveCustomOnly's payload throughout the copy.
pub fn capture_block_entity(
    level: &mut crate::level::Level,
    pos: BlockPos,
) -> Result<Option<BlockEntityData>, super::placement::PlacementError> {
    level
        .get_or_promote_block_entity(pos)?
        .map(|handle| {
            let receiver = level.block_entity_state(handle)?;
            Ok(BlockEntityData {
                custom: receiver.save_custom_only()?,
                components: receiver.data().components.clone(),
            })
        })
        .transpose()
}

/// Port of CloneCommands' loadCustomOnly/setComponents/setChanged sequence.
/// Class callbacks may replace the positional receiver, so retain the original
/// generation through all three calls and release it on every exit path.
pub fn restore_block_entity(
    level: &mut crate::level::Level,
    pos: BlockPos,
    data: &BlockEntityData,
) -> Result<(), crate::level::LevelError> {
    let Some(handle) = level.get_or_promote_block_entity(pos)? else {
        return Ok(());
    };
    level.retain_block_entity(handle)?;
    let result = (|| {
        handle.load_custom_only(&data.custom, level)?;
        level.block_entity_state_mut(handle)?.data_mut().components = data.components.clone();
        handle.set_changed(level)
    })();
    level.release_block_entity(handle);
    result
}

pub trait CloneAccess {
    type Error: std::error::Error + 'static;
    type Filter;
    fn max_block_modifications(&self) -> i32;
    fn has_chunks_at(
        &mut self,
        dimension: &Identifier,
        begin: BlockPos,
        end: BlockPos,
    ) -> Result<bool, Self::Error>;
    fn is_debug(&mut self, dimension: &Identifier) -> Result<bool, Self::Error>;
    fn block_state(
        &mut self,
        dimension: &Identifier,
        pos: BlockPos,
    ) -> Result<BlockState, Self::Error>;
    /// Evaluate the captured BlockPredicate against BlockInWorld at its native call site.
    fn matches(
        &mut self,
        filter: &Self::Filter,
        dimension: &Identifier,
        pos: BlockPos,
        state: BlockState,
    ) -> Result<bool, Self::Error>;
    /// saveCustomOnly and components(), with no metadata or component flattening.
    fn block_entity(
        &mut self,
        dimension: &Identifier,
        pos: BlockPos,
    ) -> Result<Option<BlockEntityData>, Self::Error>;
    fn is_solid_render(
        &mut self,
        dimension: &Identifier,
        pos: BlockPos,
        state: BlockState,
    ) -> Result<bool, Self::Error>;
    fn is_collision_shape_full_block(
        &mut self,
        dimension: &Identifier,
        pos: BlockPos,
        state: BlockState,
    ) -> Result<bool, Self::Error>;
    fn set_block(
        &mut self,
        dimension: &Identifier,
        pos: BlockPos,
        state: BlockState,
        flags: u32,
    ) -> Result<bool, Self::Error>;
    /// If the destination has a receiver, loadCustomOnly, setComponents, setChanged.
    fn restore_block_entity(
        &mut self,
        dimension: &Identifier,
        pos: BlockPos,
        data: &BlockEntityData,
    ) -> Result<(), Self::Error>;
    fn update_neighbors_on_block_set(
        &mut self,
        dimension: &Identifier,
        pos: BlockPos,
        previous: BlockState,
    ) -> Result<(), Self::Error>;
    /// LevelTicks#copyAreaFrom; same-dimension copying must snapshot before scheduling.
    /// Vanilla copies block ticks here, not fluid ticks.
    fn copy_block_ticks(
        &mut self,
        source: &Identifier,
        target: &Identifier,
        area: TickArea,
        offset: BlockPos,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug)]
pub enum CloneError<E> {
    Syntax(CommandSyntaxException),
    World(E),
}
impl<E: fmt::Display> fmt::Display for CloneError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(error) => error.fmt(f),
            Self::World(error) => error.fmt(f),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for CloneError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Syntax(error) => error,
            Self::World(error) => error,
        })
    }
}
impl<E> From<CommandSyntaxException> for CloneError<E> {
    fn from(error: CommandSyntaxException) -> Self {
        Self::Syntax(error)
    }
}
struct BlockInfo {
    pos: BlockPos,
    state: BlockState,
    entity: Option<BlockEntityData>,
    previous: BlockState,
}

/// Port of CloneCommands#clone, after getLoadedDimensionAndPosition has resolved
/// all three arguments. The end argument is resolved even though its dimension
/// is necessarily the same as the begin argument's dimension.
pub fn execute<A: CloneAccess>(
    access: &mut A,
    begin: Position,
    end: Position,
    destination: Position,
    filter: &A::Filter,
    mode: Mode,
    strict: bool,
) -> Result<CommandResult, CloneError<A::Error>> {
    let min = BlockPos::new(
        begin.position.x.min(end.position.x),
        begin.position.y.min(end.position.y),
        begin.position.z.min(end.position.z),
    );
    let max = BlockPos::new(
        begin.position.x.max(end.position.x),
        begin.position.y.max(end.position.y),
        begin.position.z.max(end.position.z),
    );
    let dest = destination.position;
    let dest_end = BlockPos::new(
        dest.x.wrapping_add(max.x.wrapping_sub(min.x)),
        dest.y.wrapping_add(max.y.wrapping_sub(min.y)),
        dest.z.wrapping_add(max.z.wrapping_sub(min.z)),
    );
    if mode == Mode::Normal
        && begin.dimension == destination.dimension
        && dest.x <= max.x
        && dest_end.x >= min.x
        && dest.y <= max.y
        && dest_end.y >= min.y
        && dest.z <= max.z
        && dest_end.z >= min.z
    {
        return Err(error("commands.clone.overlap", &[]).into());
    }
    let area = (i64::from(max.x) - i64::from(min.x) + 1)
        .wrapping_mul(i64::from(max.y) - i64::from(min.y) + 1)
        .wrapping_mul(i64::from(max.z) - i64::from(min.z) + 1);
    let limit = access.max_block_modifications();
    if area > i64::from(limit) {
        return Err(error(
            "commands.clone.toobig",
            &[limit.to_string(), area.to_string()],
        )
        .into());
    }
    if !access
        .has_chunks_at(&begin.dimension, begin.position, end.position)
        .map_err(CloneError::World)?
        || !access
            .has_chunks_at(&destination.dimension, dest, dest_end)
            .map_err(CloneError::World)?
    {
        return Err(error("argument.pos.unloaded", &[]).into());
    }
    if access
        .is_debug(&destination.dimension)
        .map_err(CloneError::World)?
    {
        return Err(error("commands.clone.failed", &[]).into());
    }
    let offset = BlockPos::new(
        dest.x.wrapping_sub(min.x),
        dest.y.wrapping_sub(min.y),
        dest.z.wrapping_sub(min.z),
    );
    let mut solid = Vec::new();
    let mut entities = Vec::new();
    let mut other = Vec::new();
    let mut clear = VecDeque::new();
    for z in min.z..=max.z {
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let pos = BlockPos::new(x, y, z);
                let target = BlockPos::new(
                    x.wrapping_add(offset.x),
                    y.wrapping_add(offset.y),
                    z.wrapping_add(offset.z),
                );
                let state = access
                    .block_state(&begin.dimension, pos)
                    .map_err(CloneError::World)?;
                if !access
                    .matches(filter, &begin.dimension, pos, state)
                    .map_err(CloneError::World)?
                {
                    continue;
                }
                let entity = access
                    .block_entity(&begin.dimension, pos)
                    .map_err(CloneError::World)?;
                let category = if entity.is_some() {
                    1
                } else if !access
                    .is_solid_render(&begin.dimension, pos, state)
                    .map_err(CloneError::World)?
                    && !access
                        .is_collision_shape_full_block(&begin.dimension, pos, state)
                        .map_err(CloneError::World)?
                {
                    2
                } else {
                    0
                };
                let previous = access
                    .block_state(&destination.dimension, target)
                    .map_err(CloneError::World)?;
                let info = BlockInfo {
                    pos: target,
                    state,
                    entity,
                    previous,
                };
                match category {
                    0 => solid.push(info),
                    1 => entities.push(info),
                    _ => other.push(info),
                }
                if category == 2 {
                    clear.push_front(pos);
                } else {
                    clear.push_back(pos);
                }
            }
        }
    }
    let flags = if strict { 818 } else { 2 };
    if mode == Mode::Move {
        for &pos in &clear {
            access
                .set_block(&begin.dimension, pos, Block::BARRIER.default_state(), 818)
                .map_err(CloneError::World)?;
        }
        for &pos in &clear {
            access
                .set_block(
                    &begin.dimension,
                    pos,
                    Block::AIR.default_state(),
                    if strict { flags } else { 3 },
                )
                .map_err(CloneError::World)?;
        }
    }
    let blocks: Vec<_> = solid.into_iter().chain(entities).chain(other).collect();
    for info in blocks.iter().rev() {
        access
            .set_block(
                &destination.dimension,
                info.pos,
                Block::BARRIER.default_state(),
                818,
            )
            .map_err(CloneError::World)?;
    }
    let mut count = 0;
    for info in &blocks {
        if access
            .set_block(&destination.dimension, info.pos, info.state, flags)
            .map_err(CloneError::World)?
        {
            count += 1;
        }
    }
    for info in &blocks {
        if let Some(entity) = &info.entity {
            access
                .restore_block_entity(&destination.dimension, info.pos, entity)
                .map_err(CloneError::World)?;
            access
                .set_block(&destination.dimension, info.pos, info.state, flags)
                .map_err(CloneError::World)?;
        }
    }
    if !strict {
        for info in blocks.iter().rev() {
            access
                .update_neighbors_on_block_set(&destination.dimension, info.pos, info.previous)
                .map_err(CloneError::World)?;
        }
    }
    access
        .copy_block_ticks(
            &begin.dimension,
            &destination.dimension,
            TickArea { min, max },
            offset,
        )
        .map_err(CloneError::World)?;
    if count == 0 {
        return Err(error("commands.clone.failed", &[]).into());
    }
    Ok(CommandResult::success(
        count,
        Component::translatable("commands.clone.success", vec![builtins::integer(count)]),
        true,
    ))
}

#[cfg(test)]
mod tests;
