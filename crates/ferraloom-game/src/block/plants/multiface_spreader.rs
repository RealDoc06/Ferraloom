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

/// Concrete configurations from DefaultSpreaderConfig and SculkVeinSpreaderConfig.
/// Charge/cursor behavior is separate from these block placement rules.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpreaderConfig {
    Default(Block),
    SculkVein,
    SculkVeinSameSpace,
}
impl SpreaderConfig {
    pub fn for_block(block: Block) -> Self {
        if block == Block::SCULK_VEIN {
            Self::SculkVein
        } else {
            Self::Default(block)
        }
    }
    pub fn block(self) -> Block {
        match self {
            Self::Default(block) => block,
            _ => Block::SCULK_VEIN,
        }
    }
    /// Port of SculkVeinSpreaderConfig#isOtherBlockValidAsSource.
    pub fn is_other_block_valid_as_source(self, state: BlockState) -> bool {
        !matches!(self, Self::Default(_)) && state.block() != Block::SCULK_VEIN
    }
    pub fn can_spread_from(self, state: BlockState, face: Direction) -> bool {
        self.is_other_block_valid_as_source(state) || multiface::has_face(state, face)
    }
    /// Port of both configs' stateCanBeReplaced, preserving rejection order.
    pub fn state_can_be_replaced(
        self,
        level: &LevelReaderRef<'_, '_>,
        source: BlockPos,
        placement: SpreadPos,
        old: BlockState,
    ) -> Result<bool, LevelError> {
        let default = || {
            old.is_air()
                || old.block() == self.block()
                || old.block() == Block::WATER && old.data().fluid.source
        };
        if matches!(self, Self::Default(_)) {
            return Ok(default());
        }
        let against = level.get_block_state(placement.pos.relative(placement.face, 1))?;
        if matches!(
            against.block(),
            Block::SCULK | Block::SCULK_CATALYST | Block::MOVING_PISTON
        ) {
            return Ok(false);
        }
        if source.dist_manhattan(placement.pos.into()) == 2 {
            let neighbor = source.relative(placement.face.opposite(), 1);
            if level.is_face_sturdy(level.get_block_state(neighbor)?, neighbor, placement.face)? {
                return Ok(false);
            }
        }
        let fluid = old.data().fluid;
        if !fluid.empty && fluid.fluid_type != ferraloom_data::registries::fluid::Id::WATER {
            return Ok(false);
        }
        Ok(!level
            .block_tags()?
            .contains_key(ferraloom_data::tags::BlockTags::FIRE, old.block())
            && (old.data().replaceable || default()))
    }
    pub fn can_spread_into(
        self,
        level: &LevelReaderRef<'_, '_>,
        source: BlockPos,
        spread: SpreadPos,
    ) -> Result<bool, LevelError> {
        let old = level.get_block_state(spread.pos)?;
        Ok(self.state_can_be_replaced(level, source, spread, old)?
            && multiface::state_for_placement(level, self.block(), old, spread.pos, spread.face)?
                .is_some())
    }
    pub fn get_spread(
        self,
        level: &LevelReaderRef<'_, '_>,
        state: BlockState,
        pos: BlockPos,
        from: Direction,
        toward: Direction,
    ) -> Result<Option<SpreadPos>, LevelError> {
        if toward.axis() == from.axis()
            || !(self.is_other_block_valid_as_source(state)
                || multiface::has_face(state, from) && !multiface::has_face(state, toward))
        {
            return Ok(None);
        }
        let candidates = [
            SpreadPos { pos, face: toward },
            SpreadPos {
                pos: pos.relative(toward, 1),
                face: from,
            },
            SpreadPos {
                pos: pos.relative(toward, 1).relative(from, 1),
                face: toward.opposite(),
            },
        ];
        let count = if self == Self::SculkVeinSameSpace {
            1
        } else {
            candidates.len()
        };
        for candidate in &candidates[..count] {
            if self.can_spread_into(level, pos, *candidate)? {
                return Ok(Some(*candidate));
            }
        }
        Ok(None)
    }
    pub fn spread_from_face_toward_direction(
        self,
        level: &mut LevelAccessRef<'_, '_>,
        state: BlockState,
        pos: BlockPos,
        from: Direction,
        toward: Direction,
        post_process: bool,
    ) -> Result<Option<SpreadPos>, LevelError> {
        let Some(candidate) = self.get_spread(&level.as_reader(), state, pos, from, toward)? else {
            return Ok(None);
        };
        Ok(spread_to_face(level, self.block(), candidate, post_process)?.then_some(candidate))
    }
    pub fn spread_from_face_toward_random_direction(
        self,
        level: &mut LevelAccessRef<'_, '_>,
        state: BlockState,
        pos: BlockPos,
        from: Direction,
        random: &mut impl RandomSource,
        post_process: bool,
    ) -> Result<Option<SpreadPos>, LevelError> {
        for toward in shuffled(random) {
            if let Some(spread) = self.spread_from_face_toward_direction(
                level,
                state,
                pos,
                from,
                toward,
                post_process,
            )? {
                return Ok(Some(spread));
            }
        }
        Ok(None)
    }
    pub fn spread_from_random_face_toward_random_direction(
        self,
        level: &mut LevelAccessRef<'_, '_>,
        state: BlockState,
        pos: BlockPos,
        random: &mut impl RandomSource,
    ) -> Result<Option<SpreadPos>, LevelError> {
        for from in shuffled(random) {
            if self.can_spread_from(state, from)
                && let Some(spread) = self.spread_from_face_toward_random_direction(
                    level, state, pos, from, random, false,
                )?
            {
                return Ok(Some(spread));
            }
        }
        Ok(None)
    }
    pub fn spread_all(
        self,
        level: &mut LevelAccessRef<'_, '_>,
        state: BlockState,
        pos: BlockPos,
        post_process: bool,
    ) -> Result<usize, LevelError> {
        let mut count = 0;
        for from in Direction::VALUES {
            if self.can_spread_from(state, from) {
                for toward in Direction::VALUES {
                    count += usize::from(
                        self.spread_from_face_toward_direction(
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
}

/// Port of DefaultSpreaderConfig or the vein block's concrete configuration.
pub fn get_spread_from_face_toward_direction(
    level: &LevelReaderRef<'_, '_>,
    state: BlockState,
    pos: BlockPos,
    from: Direction,
    toward: Direction,
) -> Result<Option<SpreadPos>, LevelError> {
    SpreaderConfig::for_block(state.block()).get_spread(level, state, pos, from, toward)
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
            LevelAccessRef::Server(level) => {
                ferraloom_worldgen::feature::LiveWorldGenLevel::mark_post_processing(
                    *level,
                    [spread.pos.x, spread.pos.y, spread.pos.z],
                )
                .map_err(|source| LevelError::WorldgenMutationFailed {
                    pos: spread.pos,
                    source,
                })?
            }
            LevelAccessRef::Worldgen(region) => region
                .mark_post_processing([spread.pos.x, spread.pos.y, spread.pos.z])
                .map_err(|source| LevelError::WorldgenMutationFailed {
                    pos: spread.pos,
                    source,
                })?,
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
    SpreaderConfig::for_block(state.block()).spread_from_face_toward_direction(
        level,
        state,
        pos,
        from,
        toward,
        post_process,
    )
}

fn shuffled(random: &mut impl RandomSource) -> [Direction; 6] {
    let mut faces = Direction::VALUES;
    for i in (2..=faces.len()).rev() {
        faces.swap(i - 1, random.next_int_bound(i as i32) as usize);
    }
    faces
}
/// Port of MultifaceSpreader#spreadFromFaceTowardRandomDirection.
pub fn spread_from_face_toward_random_direction(
    level: &mut LevelAccessRef<'_, '_>,
    state: BlockState,
    pos: BlockPos,
    from: Direction,
    random: &mut impl RandomSource,
    post_process: bool,
) -> Result<Option<SpreadPos>, LevelError> {
    SpreaderConfig::for_block(state.block()).spread_from_face_toward_random_direction(
        level,
        state,
        pos,
        from,
        random,
        post_process,
    )
}
pub fn spread_from_random_face_toward_random_direction(
    level: &mut LevelAccessRef<'_, '_>,
    state: BlockState,
    pos: BlockPos,
    random: &mut impl RandomSource,
) -> Result<Option<SpreadPos>, LevelError> {
    SpreaderConfig::for_block(state.block())
        .spread_from_random_face_toward_random_direction(level, state, pos, random)
}
pub fn spread_all(
    level: &mut LevelAccessRef<'_, '_>,
    state: BlockState,
    pos: BlockPos,
    post_process: bool,
) -> Result<usize, LevelError> {
    SpreaderConfig::for_block(state.block()).spread_all(level, state, pos, post_process)
}

/// The gameplay overload draws from the same level stream that neighbor
/// callbacks can consume, with no RNG clone across a reentrant setBlock.
pub fn spread_using_level_random(
    level: &mut LevelAccessRef<'_, '_>,
    state: BlockState,
    pos: BlockPos,
) -> Result<Option<SpreadPos>, LevelError> {
    let config = SpreaderConfig::for_block(state.block());
    for from in shuffled(level.random()?) {
        if config.can_spread_from(state, from) {
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
