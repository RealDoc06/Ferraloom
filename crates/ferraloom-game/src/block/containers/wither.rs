//! WitherSkullBlock's server placement query. A matched pattern reaches the
//! explicit unported WITHER factory before any blocks or entities are changed.
use crate::{
    block::entity::BlockEntityState,
    level::{Level, LevelError},
};
use ferraloom_data::{Block, BlockState, EntityType, tags::BlockTags};
use ferraloom_protocol::play::Difficulty;
use ferraloom_util::{BlockPos, Direction};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct WitherPatternMatch {
    pub origin: BlockPos,
    pub forwards: Direction,
    pub up: Direction,
}
/// Port of BlockPattern#translateAndRotate for the one-deep wither pattern.
fn translate(origin: BlockPos, forwards: Direction, up: Direction, x: i32, y: i32) -> BlockPos {
    let up = up.step();
    let right = forwards.step().cross(up);
    origin.offset(
        -up.x * y + right.x * x,
        -up.y * y + right.y * x,
        -up.z * y + right.z * x,
    )
}
/// Port of WitherSkullBlock#getOrCreateWitherFull and BlockPattern#find/matches.
/// The local state cache is vanilla BlockInWorld's deliberate first-read cache,
/// shared across candidate origins/orientations; it is not a world writeback.
pub(super) fn find_pattern(
    level: &Level,
    origin: BlockPos,
) -> Result<Option<WitherPatternMatch>, LevelError> {
    let mut cache = BTreeMap::<BlockPos, BlockState>::new();
    for candidate in BlockPos::between_closed(origin, origin.offset(2, 2, 2)) {
        for forwards in Direction::VALUES {
            for up in Direction::VALUES {
                if up == forwards || up == forwards.opposite() {
                    continue;
                }
                let mut matches = true;
                'cells: for x in 0..3 {
                    for y in 0..3 {
                        let pos = translate(candidate, forwards, up, x, y);
                        let state = if let Some(state) = cache.get(&pos) {
                            Some(*state)
                        } else if level.has_chunk_at(pos) {
                            let state = level.get_block_state(pos)?;
                            cache.insert(pos, state);
                            Some(state)
                        } else {
                            None
                        };
                        let accepted = if y == 0 {
                            state.is_some_and(|state| {
                                matches!(
                                    state.block(),
                                    Block::WITHER_SKELETON_SKULL
                                        | Block::WITHER_SKELETON_WALL_SKULL
                                )
                            })
                        } else {
                            // Native '#'/'~' dereference getState; an unavailable cell
                            // is a required failure, not an invented negative match.
                            let state = state.ok_or(LevelError::MissingChunk(
                                ferraloom_util::ChunkPos::containing(pos),
                            ))?;
                            if y == 1 || x == 1 {
                                level.block_tags()?.contains_key(
                                    BlockTags::WITHER_SUMMON_BASE_BLOCKS,
                                    state.block(),
                                )
                            } else {
                                state.is_air()
                            }
                        };
                        if !accepted {
                            matches = false;
                            break 'cells;
                        }
                    }
                }
                if matches {
                    return Ok(Some(WitherPatternMatch {
                        origin: candidate,
                        forwards,
                        up,
                    }));
                }
            }
        }
    }
    Ok(None)
}
/// Port of both WitherSkullBlock#checkSpawn overloads. No-match, wrong receiver,
/// below-minimum and Peaceful are actual native outcomes; a match needs a real
/// WitherBoss instance before native clearPatternBlocks/criteria/addFreshEntity.
pub(super) fn check_spawn(pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
    let Some(handle) = level.block_entity_ref(pos)? else {
        return Ok(());
    };
    if !matches!(
        level.block_entity_state(handle)?,
        BlockEntityState::Skull(_)
    ) {
        return Ok(());
    }
    level.with_block_entity_callback(handle, |level| {
        let state = level.block_entity_state(handle)?.data().block_state;
        if !matches!(
            state.block(),
            Block::WITHER_SKELETON_SKULL | Block::WITHER_SKELETON_WALL_SKULL
        ) || pos.y < level.min_build_height()
            || level.difficulty() == Difficulty::Peaceful
        {
            return Ok(());
        }
        if find_pattern(level, pos)?.is_some() {
            return Err(LevelError::UnsupportedEntity(EntityType::WITHER));
        }
        Ok(())
    })
}
