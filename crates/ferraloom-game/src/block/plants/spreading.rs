//! Port of SpreadingSnowyBlock's light occlusion, decay and random propagation.
use super::super::{BlockBehavior, LevelAccessRef, LevelReaderRef, UpdateFlags};
use crate::level::{Level, LevelError};
use ferraloom_data::{
    Block, BlockState, properties as p,
    tags::{BlockTags, FluidTags},
};
use ferraloom_util::{BlockPos, Direction, random::RandomSource};

pub struct SpreadingSnowy;
pub static SPREADING_SNOWY: SpreadingSnowy = SpreadingSnowy;

/// Port of SpreadingSnowyBlock#canStayAlive. A single snow layer precedes the
/// full-fluid and occlusion checks, including waterlogged upper blocks.
pub(super) fn can_stay_alive(
    state: BlockState,
    pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    let above = level.get_block_state(pos.above(1))?;
    if above.block() == Block::SNOW && above.get(p::LAYERS) == Some(1) {
        return Ok(true);
    }
    if above.data().fluid.amount == 8 {
        return Ok(false);
    }
    Ok(
        ferraloom_light::light_dampening_into(state, above, Direction::Up, above.data().opacity)
            < 15,
    )
}
pub(super) fn can_propagate(
    state: BlockState,
    pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    Ok(can_stay_alive(state, pos, level)?
        && !level.fluid_tags()?.contains_key(
            FluidTags::WATER,
            level.get_fluid_state(pos.above(1))?.fluid_type,
        ))
}

impl BlockBehavior for SpreadingSnowy {
    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if !can_stay_alive(state, pos, &LevelReaderRef::Server(level))? {
            LevelAccessRef::Server(level).set_block(
                pos,
                Block::DIRT.default_state(),
                UpdateFlags::ALL,
            )?;
        } else if level.max_local_raw_brightness(pos.above(1))? >= 9 {
            let spreading = state.block().default_state();
            for _ in 0..4 {
                let dx = level.random().next_int_bound(3) - 1;
                let dy = level.random().next_int_bound(5) - 3;
                let dz = level.random().next_int_bound(3) - 1;
                let target = pos.offset(dx, dy, dz);
                if level.get_block_state(target)?.block() == Block::DIRT
                    && can_propagate(spreading, target, &LevelReaderRef::Server(level))?
                {
                    let snowy = level.block_tags()?.contains_key(
                        BlockTags::SNOW,
                        level.get_block_state(target.above(1))?.block(),
                    );
                    LevelAccessRef::Server(level).set_block(
                        target,
                        spreading.with(p::SNOWY, snowy).unwrap(),
                        UpdateFlags::ALL,
                    )?;
                }
            }
        }
        Ok(())
    }
}
