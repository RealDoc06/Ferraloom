//! Port of BambooStalkBlock/BambooSaplingBlock growth and their leaf crown.
use super::super::{BlockBehavior, LevelAccessRef, LevelReaderRef, UpdateFlags};
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockState, properties as p};
use ferraloom_util::{BlockPos, Direction, random::RandomSource};
pub struct Bamboo;
pub static BAMBOO: Bamboo = Bamboo;
pub fn height(
    level: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
    direction: Direction,
) -> Result<i32, LevelError> {
    let mut height = 0;
    while height < 16
        && level
            .get_block_state(pos.relative(direction, height + 1))?
            .block()
            == Block::BAMBOO
    {
        height += 1;
    }
    Ok(height)
}
/// Port of BambooStalkBlock#growBamboo. Crown rewrites precede the stage draw.
pub fn grow(
    state: BlockState,
    pos: BlockPos,
    height: i32,
    level: &mut LevelAccessRef<'_, '_>,
) -> Result<(), LevelError> {
    let below = level.get_block_state(pos.below(1))?;
    let two_below = level.get_block_state(pos.below(2))?;
    let mut leaves = p::Leaves::None;
    if height >= 1 {
        if below.block() != Block::BAMBOO || below.get(p::LEAVES) == Some(p::Leaves::None) {
            leaves = p::Leaves::Small;
        } else {
            leaves = p::Leaves::Large;
            if two_below.block() == Block::BAMBOO {
                level.set_block(
                    pos.below(1),
                    below.with(p::LEAVES, p::Leaves::Small).unwrap(),
                    UpdateFlags::ALL,
                )?;
                level.set_block(
                    pos.below(2),
                    two_below.with(p::LEAVES, p::Leaves::None).unwrap(),
                    UpdateFlags::ALL,
                )?;
            }
        }
    }
    let age = if state.get(p::AGE) != Some(1) && two_below.block() != Block::BAMBOO {
        0
    } else {
        1
    };
    let stage = if (height < 11 || level.random()?.next_float() >= 0.25) && height != 15 {
        0
    } else {
        1
    };
    level.set_block(
        pos.above(1),
        Block::BAMBOO
            .default_state()
            .with(p::AGE, age)
            .unwrap()
            .with(p::LEAVES, leaves)
            .unwrap()
            .with(p::STAGE, stage)
            .unwrap(),
        UpdateFlags::ALL,
    )?;
    Ok(())
}
pub fn grow_sapling(pos: BlockPos, level: &mut LevelAccessRef<'_, '_>) -> Result<(), LevelError> {
    level.set_block(
        pos.above(1),
        Block::BAMBOO
            .default_state()
            .with(p::LEAVES, p::Leaves::Small)
            .unwrap(),
        UpdateFlags::ALL,
    )?;
    Ok(())
}
impl BlockBehavior for Bamboo {
    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let sapling = state.block() == Block::BAMBOO_SAPLING;
        if !sapling && state.get(p::STAGE) != Some(0) {
            return Ok(());
        }
        if level.random().next_int_bound(3) == 0
            && level.get_block_state(pos.above(1))?.is_air()
            && LevelReaderRef::Server(level).get_raw_brightness(pos.above(1), 0)? >= 9
        {
            if sapling {
                grow_sapling(pos, &mut LevelAccessRef::Server(level))?;
            } else {
                let height = height(&LevelReaderRef::Server(level), pos, Direction::Down)? + 1;
                if height < 16 {
                    grow(state, pos, height, &mut LevelAccessRef::Server(level))?;
                }
            }
        }
        Ok(())
    }
}
