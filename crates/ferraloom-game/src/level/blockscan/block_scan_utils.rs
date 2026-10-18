//! Port of BlockScanUtils. Box scans are chunk X/Z, section Y, then cell Y/Z/X;
//! the out-of-height AIR tail instead uses BlockPos#betweenClosed order.
use super::{BlockStateConsumer, Continuation};
use crate::level::{Level, LevelError};
use ferraloom_chunk::LevelChunkSection;
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{BlockPos, ChunkPos};

pub(super) fn find_blocks(
    level: &Level,
    from: BlockPos,
    to: BlockPos,
    predicate: &impl Fn(BlockState) -> bool,
    consumer: &mut impl BlockStateConsumer,
) -> Result<bool, LevelError> {
    let min = from.min(to);
    let max = from.max(to);
    let min_section_y = (level.min_build_height() >> 4).max(min.y >> 4);
    let max_section_y = (level.max_build_height() >> 4).min(max.y >> 4);
    for x in (min.x >> 4)..=(max.x >> 4) {
        for z in (min.z >> 4)..=(max.z >> 4) {
            // Native requires this read even when no section Y intersects.
            let chunk = level.get_full_chunk(ChunkPos::new(x, z))?;
            for y in min_section_y..=max_section_y {
                let section = chunk
                    .section(y)
                    .expect("scan section within dimension bounds");
                if section.states().maybe_has(predicate) {
                    let origin = BlockPos::new(x << 4, y << 4, z << 4);
                    let from = origin.max(min);
                    let to = origin.offset(15, 15, 15).min(max);
                    if find_blocks_in_section(section, from, to, predicate, consumer)? {
                        return Ok(true);
                    }
                }
            }
        }
    }
    let below = min.y < level.min_build_height();
    let above = max.y > level.max_build_height();
    let air = Block::AIR.default_state();
    if (below || above) && predicate(air) {
        if below {
            let end = BlockPos::new(max.x, level.min_build_height().wrapping_sub(1), max.z);
            if visit_air(BlockPos::between_closed(min, end), air, consumer)? {
                return Ok(true);
            }
        }
        if above {
            let start = BlockPos::new(min.x, level.max_build_height().wrapping_add(1), min.z);
            if visit_air(BlockPos::between_closed(start, max), air, consumer)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Port of BlockScanUtils#findBlocksInSection; bounds are absolute, inclusive.
fn find_blocks_in_section(
    section: &LevelChunkSection,
    from: BlockPos,
    to: BlockPos,
    predicate: &impl Fn(BlockState) -> bool,
    consumer: &mut impl BlockStateConsumer,
) -> Result<bool, LevelError> {
    for y in from.y..=to.y {
        for z in from.z..=to.z {
            for x in from.x..=to.x {
                let state = section.get_block_state((x & 15) as u8, (y & 15) as u8, (z & 15) as u8);
                if predicate(state)
                    && consumer.apply(BlockPos::new(x, y, z), state)? == Continuation::Abort
                {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}
fn visit_air(
    positions: impl Iterator<Item = BlockPos>,
    air: BlockState,
    consumer: &mut impl BlockStateConsumer,
) -> Result<bool, LevelError> {
    for pos in positions {
        if consumer.apply(pos, air)? == Continuation::Abort {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Port of BlockScanUtils#findBlocksWithCache, including duplicate positions.
pub(super) fn find_blocks_with_cache(
    level: &Level,
    positions: impl Iterator<Item = BlockPos>,
    predicate: &impl Fn(BlockState) -> bool,
    consumer: &mut impl BlockStateConsumer,
) -> Result<bool, LevelError> {
    let mut cache = super::FilteredSectionCache::new(level, predicate);
    for pos in positions {
        if let Some(state) = cache.get_block_state(pos)?
            && consumer.apply(pos, state)? == Continuation::Abort
        {
            return Ok(true);
        }
    }
    Ok(false)
}
