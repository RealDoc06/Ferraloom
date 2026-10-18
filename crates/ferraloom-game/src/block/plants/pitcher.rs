//! Port of PitcherCropBlock growth and lower-half bonemeal lookup.
use super::{
    super::{BlockBehavior, LevelAccessRef, LevelReaderRef, UpdateFlags},
    bonemeal::inside_height,
    crops,
};
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockState, properties as p};
use ferraloom_util::{BlockPos, random::RandomSource};

pub struct Pitcher;
pub static PITCHER: Pitcher = Pitcher;

pub(super) fn lower_half(
    state: BlockState,
    pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
) -> Result<Option<(BlockPos, BlockState)>, LevelError> {
    if state.block() == Block::PITCHER_CROP && state.get(p::HALF) == Some(p::Half::Lower) {
        return Ok(Some((pos, state)));
    }
    let pos = pos.below(1);
    let below = level.get_block_state(pos)?;
    Ok(
        (below.block() == Block::PITCHER_CROP && below.get(p::HALF) == Some(p::Half::Lower))
            .then_some((pos, below)),
    )
}

pub(super) fn can_grow(
    state: BlockState,
    pos: BlockPos,
    new_age: u8,
    level: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    Ok(state.get(p::AGE).unwrap() < 4
        && level.get_raw_brightness(pos, 0)? >= 8
        && inside_height(level, pos.above(1))
        && (new_age < 3 || {
            let above = level.get_block_state(pos.above(1))?;
            above.is_air() || above.block() == Block::PITCHER_CROP
        }))
}

pub(super) fn grow(
    state: BlockState,
    pos: BlockPos,
    level: &mut LevelAccessRef<'_, '_>,
) -> Result<(), LevelError> {
    let age = (state.get(p::AGE).unwrap() + 1).min(4);
    if can_grow(state, pos, age, &level.as_reader())? {
        let lower = state.with(p::AGE, age).unwrap();
        level.set_block(pos, lower, UpdateFlags::CLIENTS)?;
        if age >= 3 {
            level.set_block(
                pos.above(1),
                lower.with(p::HALF, p::Half::Upper).unwrap(),
                UpdateFlags::ALL,
            )?;
        }
    }
    Ok(())
}

impl BlockBehavior for Pitcher {
    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let speed = crops::growth_speed(state.block(), pos, &LevelReaderRef::Server(level))?;
        if level.random().next_int_bound((25.0_f32 / speed) as i32 + 1) == 0 {
            grow(state, pos, &mut LevelAccessRef::Server(level))?;
        }
        Ok(())
    }
}
