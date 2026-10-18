//! Port of MultifaceSpreader and DefaultSpreaderConfig. Both level arms share
//! placement, direction order, shuffle consumption and post-processing marks.
use super::{
    super::{LevelAccessRef, LevelReaderRef, UpdateFlags},
    multiface,
};
use crate::level::LevelError;
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{BlockPos, Direction, random::RandomSource};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpreadPos {
    pub pos: BlockPos,
    pub face: Direction,
}

/// Port of DefaultSpreaderConfig#canSpreadInto and the ordered spread types.
pub fn get_spread_from_face_toward_direction(
    level: &LevelReaderRef<'_, '_>,
    state: BlockState,
    pos: BlockPos,
    from: Direction,
    toward: Direction,
) -> Result<Option<SpreadPos>, LevelError> {
    if toward.axis() == from.axis()
        || !multiface::has_face(state, from)
        || multiface::has_face(state, toward)
    {
        return Ok(None);
    }
    for candidate in [
        SpreadPos { pos, face: toward },
        SpreadPos {
            pos: pos.relative(toward, 1),
            face: from,
        },
        SpreadPos {
            pos: pos.relative(toward, 1).relative(from, 1),
            face: toward.opposite(),
        },
    ] {
        let old = level.get_block_state(candidate.pos)?;
        if (old.is_air()
            || old.block() == state.block()
            || (old.block() == Block::WATER && old.data().fluid.source))
            && multiface::state_for_placement(
                level,
                state.block(),
                old,
                candidate.pos,
                candidate.face,
            )?
            .is_some()
        {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}
/// Port of MultifaceSpreader#spreadToFace. Marking precedes setBlock.
pub fn spread_to_face(
    level: &mut LevelAccessRef<'_, '_>,
    block: Block,
    spread: SpreadPos,
    post_process: bool,
) -> Result<bool, LevelError> {
    let old = level.get_block_state(spread.pos)?;
    let Some(next) =
        multiface::state_for_placement(&level.as_reader(), block, old, spread.pos, spread.face)?
    else {
        return Ok(false);
    };
    if post_process {
        match level {
            LevelAccessRef::Server(level) => level.mark_pos_for_post_processing(spread.pos)?,
            LevelAccessRef::Worldgen(region) => region
                .mark_post_processing([spread.pos.x, spread.pos.y, spread.pos.z])
                .map_err(|_| LevelError::WorldgenMutationFailed(spread.pos))?,
        }
    }
    level.set_block(spread.pos, next, UpdateFlags::CLIENTS)
}
pub fn spread_from_face_toward_direction(
    level: &mut LevelAccessRef<'_, '_>,
    state: BlockState,
    pos: BlockPos,
    from: Direction,
    toward: Direction,
    post_process: bool,
) -> Result<Option<SpreadPos>, LevelError> {
    let Some(candidate) =
        get_spread_from_face_toward_direction(&level.as_reader(), state, pos, from, toward)?
    else {
        return Ok(None);
    };
    Ok(spread_to_face(level, state.block(), candidate, post_process)?.then_some(candidate))
}
fn shuffled(random: &mut impl RandomSource) -> [Direction; 6] {
    let mut faces = Direction::VALUES;
    for i in (2..=faces.len()).rev() {
        faces.swap(i - 1, random.next_int_bound(i as i32) as usize);
    }
    faces
}
/// Port of MultifaceSpreader#spreadFromFaceTowardRandomDirection. Java shuffles
/// the complete direction list before considering the first candidate.
pub fn spread_from_face_toward_random_direction(
    level: &mut LevelAccessRef<'_, '_>,
    state: BlockState,
    pos: BlockPos,
    from: Direction,
    random: &mut impl RandomSource,
    post_process: bool,
) -> Result<Option<SpreadPos>, LevelError> {
    for toward in shuffled(random) {
        if let Some(spread) =
            spread_from_face_toward_direction(level, state, pos, from, toward, post_process)?
        {
            return Ok(Some(spread));
        }
    }
    Ok(None)
}
pub fn spread_from_random_face_toward_random_direction(
    level: &mut LevelAccessRef<'_, '_>,
    state: BlockState,
    pos: BlockPos,
    random: &mut impl RandomSource,
) -> Result<Option<SpreadPos>, LevelError> {
    for from in shuffled(random) {
        if multiface::has_face(state, from) {
            if let Some(spread) =
                spread_from_face_toward_random_direction(level, state, pos, from, random, false)?
            {
                return Ok(Some(spread));
            }
        }
    }
    Ok(None)
}
pub fn spread_all(
    level: &mut LevelAccessRef<'_, '_>,
    state: BlockState,
    pos: BlockPos,
    post_process: bool,
) -> Result<usize, LevelError> {
    let mut count = 0;
    for from in Direction::VALUES {
        if multiface::has_face(state, from) {
            for toward in Direction::VALUES {
                count += usize::from(
                    spread_from_face_toward_direction(
                        level,
                        state,
                        pos,
                        from,
                        toward,
                        post_process,
                    )?
                    .is_some(),
                );
            }
        }
    }
    Ok(count)
}

/// The gameplay overload draws from the same level stream that neighbor
/// callbacks can consume, with no RNG clone across a reentrant setBlock.
pub fn spread_using_level_random(
    level: &mut LevelAccessRef<'_, '_>,
    state: BlockState,
    pos: BlockPos,
) -> Result<Option<SpreadPos>, LevelError> {
    for from in shuffled(level.random()?) {
        if multiface::has_face(state, from) {
            for toward in shuffled(level.random()?) {
                if let Some(spread) =
                    spread_from_face_toward_direction(level, state, pos, from, toward, false)?
                {
                    return Ok(Some(spread));
                }
            }
        }
    }
    Ok(None)
}
