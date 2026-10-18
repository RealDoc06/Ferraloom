//! Ports of NyliumBlock decay and NetherrackBlock bonemeal conversion.
use super::super::{BlockBehavior, LevelAccessRef, LevelReaderRef, UpdateFlags};
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockState, tags::BlockTags};
use ferraloom_util::{BlockPos, Direction, random::RandomSource};

pub struct Nylium;
pub static NYLIUM: Nylium = Nylium;

/// Port of NyliumBlock#canBeNylium. LightEngine's two cached occlusion shapes
/// and the upper state's dampening are state-only, independent of the clock.
pub(super) fn can_be_nylium(
    state: BlockState,
    pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    let above = level.get_block_state(pos.above(1))?;
    Ok(
        ferraloom_light::light_dampening_into(state, above, Direction::Up, above.data().opacity)
            < 15,
    )
}

impl BlockBehavior for Nylium {
    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if !can_be_nylium(state, pos, &LevelReaderRef::Server(level))? {
            LevelAccessRef::Server(level).set_block(
                pos,
                Block::NETHERRACK.default_state(),
                UpdateFlags::ALL,
            )?;
        }
        Ok(())
    }
}

pub(super) fn netherrack_target(
    pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    if !level
        .get_block_state(pos.above(1))?
        .data()
        .propagates_skylight
    {
        return Ok(false);
    }
    let tags = level.block_tags()?;
    for z in -1..=1 {
        for y in -1..=1 {
            for x in -1..=1 {
                if tags.contains_key(
                    BlockTags::NYLIUM,
                    level.get_block_state(pos.offset(x, y, z))?.block(),
                ) {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

pub(super) fn spread_to_netherrack(
    pos: BlockPos,
    level: &mut LevelAccessRef<'_, '_>,
) -> Result<(), LevelError> {
    let mut crimson = false;
    let mut warped = false;
    'scan: for z in -1..=1 {
        for y in -1..=1 {
            for x in -1..=1 {
                let block = level.get_block_state(pos.offset(x, y, z))?.block();
                if level
                    .as_reader()
                    .block_tags()?
                    .contains_key(BlockTags::NYLIUM, block)
                {
                    crimson |= block == Block::CRIMSON_NYLIUM;
                    warped |= block == Block::WARPED_NYLIUM;
                    if crimson && warped {
                        break 'scan;
                    }
                }
            }
        }
    }
    let block = match (crimson, warped) {
        (true, true) => {
            if level.random()?.next_boolean() {
                Block::WARPED_NYLIUM
            } else {
                Block::CRIMSON_NYLIUM
            }
        }
        (true, false) => Block::CRIMSON_NYLIUM,
        (false, true) => Block::WARPED_NYLIUM,
        (false, false) => return Ok(()),
    };
    level.set_block(pos, block.default_state(), UpdateFlags::ALL)?;
    Ok(())
}
