//! Port of SeaPickleBlock#performBonemeal, retaining every attempted-position draw.
use super::super::{LevelAccessRef, UpdateFlags};
use crate::level::LevelError;
use ferraloom_data::{Block, BlockState, properties as p, tags::BlockTags};
use ferraloom_util::{BlockPos, random::RandomSource};

pub(super) fn grow(
    state: BlockState,
    pos: BlockPos,
    level: &mut LevelAccessRef<'_, '_>,
) -> Result<(), LevelError> {
    let mut span = 1;
    let mut offset = 0;
    for x in 0..5 {
        for z in 0..span {
            for y in -1..=0 {
                let at = pos.offset(x - 2, y, z - offset);
                if at != pos
                    && level.random()?.next_int_bound(6) == 0
                    && level.get_block_state(at)?.block() == Block::WATER
                {
                    let below = level.get_block_state(at.below(1))?;
                    if level
                        .as_reader()
                        .block_tags()?
                        .contains_key(BlockTags::CORAL_BLOCKS, below.block())
                    {
                        let count = (level.random()?.next_int_bound(4) + 1) as u8;
                        level.set_block(
                            at,
                            Block::SEA_PICKLE
                                .default_state()
                                .with(p::PICKLES, count)
                                .unwrap(),
                            UpdateFlags::ALL,
                        )?;
                    }
                }
            }
        }
        if x < 2 {
            span += 2;
            offset += 1;
        } else {
            span -= 2;
            offset -= 1;
        }
    }
    level.set_block(
        pos,
        state.with(p::PICKLES, 4).unwrap(),
        UpdateFlags::CLIENTS,
    )?;
    Ok(())
}
