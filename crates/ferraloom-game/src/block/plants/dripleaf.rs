//! Ports of dripleaf placement and bonemeal growth shared with generation.
use super::{
    super::{LevelAccessRef, LevelReaderRef, UpdateFlags, fluid},
    bonemeal::inside_height,
};
use crate::level::LevelError;
use ferraloom_data::{Block, BlockState, properties as p};
use ferraloom_util::{BlockPos, random::RandomSource};

pub(super) fn can_grow_into(
    level: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
) -> Result<bool, LevelError> {
    let state = level.get_block_state(pos)?;
    Ok(inside_height(level, pos)
        && (state.is_air() || matches!(state.block(), Block::WATER | Block::SMALL_DRIPLEAF)))
}

fn place(
    block: Block,
    facing: p::Facing,
    pos: BlockPos,
    level: &mut LevelAccessRef<'_, '_>,
) -> Result<(), LevelError> {
    let waterlogged = super::source_water(level.as_reader().get_fluid_state(pos)?);
    let state = block
        .default_state()
        .with(p::WATERLOGGED, waterlogged)
        .unwrap()
        .with(p::FACING, facing)
        .unwrap();
    level.set_block(pos, state, UpdateFlags::ALL)?;
    Ok(())
}

/// Port of BigDripleafBlock#placeWithRandomHeight. Even zero available height
/// places the leaf at the original position, as vanilla does.
pub(super) fn place_with_random_height(
    pos: BlockPos,
    facing: p::Facing,
    level: &mut LevelAccessRef<'_, '_>,
) -> Result<(), LevelError> {
    let desired = level.random()?.next_int_bound(4) + 2;
    let mut height = 0;
    while height < desired && can_grow_into(&level.as_reader(), pos.above(height))? {
        height += 1;
    }
    let mut y = 0;
    while y < height - 1 {
        place(Block::BIG_DRIPLEAF_STEM, facing, pos.above(y), level)?;
        y += 1;
    }
    place(Block::BIG_DRIPLEAF, facing, pos.above(y), level)
}

pub(super) fn head_pos(
    state: BlockState,
    mut pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
) -> Result<Option<BlockPos>, LevelError> {
    if state.block() == Block::BIG_DRIPLEAF {
        return Ok(Some(pos));
    }
    loop {
        pos = pos.above(1);
        let forward = level.get_block_state(pos)?;
        if forward.block() != state.block() {
            return Ok((forward.block() == Block::BIG_DRIPLEAF).then_some(pos));
        }
    }
}

pub(super) fn bonemeal(
    mut state: BlockState,
    mut pos: BlockPos,
    level: &mut LevelAccessRef<'_, '_>,
) -> Result<(), LevelError> {
    if state.block() == Block::SMALL_DRIPLEAF {
        while state.get(p::HALF) == Some(p::Half::Upper) {
            pos = pos.below(1);
            state = level.get_block_state(pos)?;
        }
        let above = pos.above(1);
        let replaced = fluid::legacy_block(level.as_reader().get_fluid_state(above)?);
        level.set_block(above, replaced, UpdateFlags(18))?;
        return place_with_random_height(pos, state.get(p::FACING).unwrap(), level);
    }
    if let Some(head) = head_pos(state, pos, &level.as_reader())? {
        if state.block() == Block::BIG_DRIPLEAF_STEM
            || can_grow_into(&level.as_reader(), head.above(1))?
        {
            let facing = state.get(p::FACING).unwrap();
            place(Block::BIG_DRIPLEAF_STEM, facing, head, level)?;
            place(Block::BIG_DRIPLEAF, facing, head.above(1), level)?;
        }
    }
    Ok(())
}
