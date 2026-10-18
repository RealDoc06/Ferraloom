//! Native block matchers keep section scan order distinct from supplied-position
//! order. Required chunk acquisition errors propagate under both policies.
use super::{Level, LevelError};
use ferraloom_data::BlockState;
use ferraloom_util::{Aabb, BlockPos};

mod block_scan_utils;
mod filtered_section_cache;
mod positions;
pub use filtered_section_cache::FilteredSectionCache;
pub use positions::ManhattanPositions;

/// Port of Continuation, returned by BlockStateConsumer#apply.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Continuation {
    Continue,
    Abort,
}

/// Port of BlockStateConsumer. Fallible callbacks retain required-query errors.
pub trait BlockStateConsumer {
    fn apply(&mut self, pos: BlockPos, state: BlockState) -> Result<Continuation, LevelError>;
}
impl<F> BlockStateConsumer for F
where
    F: FnMut(BlockPos, BlockState) -> Result<Continuation, LevelError>,
{
    fn apply(&mut self, pos: BlockPos, state: BlockState) -> Result<Continuation, LevelError> {
        self(pos, state)
    }
}

/// Port of BlockMatcher. Filters compose with short-circuiting AND; allMatched
/// searches for the negation of the entire composed state predicate.
pub trait BlockMatcher {
    #[doc(hidden)]
    fn scan(&self, invert: bool, consumer: impl BlockStateConsumer) -> Result<bool, LevelError>;

    fn at_least_matched(&self, n: i32) -> Result<bool, LevelError> {
        let mut count = 0_i32;
        self.scan(false, |_, _| {
            count = count.wrapping_add(1);
            Ok(if count >= n {
                Continuation::Abort
            } else {
                Continuation::Continue
            })
        })
    }
    fn at_most_matched(&self, n: i32) -> Result<bool, LevelError> {
        self.at_least_matched(n.wrapping_add(1)).map(|found| !found)
    }
    fn any_matched(&self) -> Result<bool, LevelError> {
        self.scan(false, |_, _| Ok(Continuation::Abort))
    }
    fn none_matched(&self) -> Result<bool, LevelError> {
        self.any_matched().map(|found| !found)
    }
    fn all_matched(&self) -> Result<bool, LevelError> {
        self.scan(true, |_, _| Ok(Continuation::Abort))
            .map(|found| !found)
    }
    fn for_each(
        &self,
        mut consumer: impl FnMut(BlockPos, BlockState) -> Result<(), LevelError>,
    ) -> Result<(), LevelError> {
        self.for_each_until(|pos, state| {
            consumer(pos, state)?;
            Ok(Continuation::Continue)
        })
        .map(|_| ())
    }
    fn for_each_until(&self, consumer: impl BlockStateConsumer) -> Result<bool, LevelError> {
        self.scan(false, consumer)
    }
}

fn all_states(_: BlockState) -> bool {
    true
}

/// Port of BoxBlockMatcher. Coordinates are inclusive and normalized by the
/// scanner, rather than by a block stream or collision-box iterator.
pub struct BoxBlockMatcher<'a, P = fn(BlockState) -> bool> {
    level: &'a Level,
    from: BlockPos,
    to: BlockPos,
    predicate: P,
}
impl<'a> BoxBlockMatcher<'a> {
    pub fn new(level: &'a Level, from: BlockPos, to: BlockPos) -> Self {
        Self {
            level,
            from,
            to,
            predicate: all_states,
        }
    }
}
impl<'a, P: Fn(BlockState) -> bool> BoxBlockMatcher<'a, P> {
    pub fn filter_state<Q: Fn(BlockState) -> bool>(
        self,
        predicate: Q,
    ) -> BoxBlockMatcher<'a, impl Fn(BlockState) -> bool> {
        BoxBlockMatcher {
            level: self.level,
            from: self.from,
            to: self.to,
            predicate: move |state| (self.predicate)(state) && predicate(state),
        }
    }
}
impl<P: Fn(BlockState) -> bool> BlockMatcher for BoxBlockMatcher<'_, P> {
    fn scan(
        &self,
        invert: bool,
        mut consumer: impl BlockStateConsumer,
    ) -> Result<bool, LevelError> {
        block_scan_utils::find_blocks(
            self.level,
            self.from,
            self.to,
            &|state| (self.predicate)(state) != invert,
            &mut consumer,
        )
    }
}

/// Port of OrderedBlockMatcher. Each query restarts the cloneable iterator;
/// positions remain lazy, in caller order, with duplicate positions preserved.
pub struct OrderedBlockMatcher<'a, I, P = fn(BlockState) -> bool> {
    level: &'a Level,
    positions: I,
    predicate: P,
}
impl<'a, I> OrderedBlockMatcher<'a, I> {
    pub fn new(level: &'a Level, positions: I) -> Self {
        Self {
            level,
            positions,
            predicate: all_states,
        }
    }
}
impl<'a, I: Iterator<Item = BlockPos> + Clone, P: Fn(BlockState) -> bool>
    OrderedBlockMatcher<'a, I, P>
{
    pub fn filter_pos<Q: Fn(&BlockPos) -> bool + Clone>(
        self,
        predicate: Q,
    ) -> OrderedBlockMatcher<'a, std::iter::Filter<I, Q>, P> {
        OrderedBlockMatcher {
            level: self.level,
            positions: self.positions.filter(predicate),
            predicate: self.predicate,
        }
    }
    pub fn filter_state<Q: Fn(BlockState) -> bool>(
        self,
        predicate: Q,
    ) -> OrderedBlockMatcher<'a, I, impl Fn(BlockState) -> bool> {
        OrderedBlockMatcher {
            level: self.level,
            positions: self.positions,
            predicate: move |state| (self.predicate)(state) && predicate(state),
        }
    }
    pub fn find_first(&self) -> Result<Option<BlockPos>, LevelError> {
        self.find_first_where(|_, _| true)
    }
    /// Port of OrderedBlockMatcher#findFirst(BiPredicate).
    pub fn find_first_where(
        &self,
        mut predicate: impl FnMut(BlockPos, BlockState) -> bool,
    ) -> Result<Option<BlockPos>, LevelError> {
        let mut result = None;
        self.for_each_until(|pos, state| {
            Ok(if predicate(pos, state) {
                result = Some(pos);
                Continuation::Abort
            } else {
                Continuation::Continue
            })
        })?;
        Ok(result)
    }
}
impl<I: Iterator<Item = BlockPos> + Clone, P: Fn(BlockState) -> bool> BlockMatcher
    for OrderedBlockMatcher<'_, I, P>
{
    fn scan(
        &self,
        invert: bool,
        mut consumer: impl BlockStateConsumer,
    ) -> Result<bool, LevelError> {
        block_scan_utils::find_blocks_with_cache(
            self.level,
            self.positions.clone(),
            &|state| (self.predicate)(state) != invert,
            &mut consumer,
        )
    }
}

impl Level {
    /// Port of LevelReader#findBlocksIn(AABB), flooring both inclusive corners.
    pub fn find_blocks_in(&self, box_: Aabb) -> BoxBlockMatcher<'_> {
        self.find_blocks_between(
            BlockPos::containing(box_.min_x, box_.min_y, box_.min_z),
            BlockPos::containing(box_.max_x, box_.max_y, box_.max_z),
        )
    }
    /// Port of LevelReader#findBlocksIn(BlockPos, BlockPos).
    pub fn find_blocks_between(&self, from: BlockPos, to: BlockPos) -> BoxBlockMatcher<'_> {
        BoxBlockMatcher::new(self, from, to)
    }
    /// Port of LevelReader#findBlocksIn(Iterable).
    pub fn find_blocks_at<I>(&self, positions: I) -> OrderedBlockMatcher<'_, I::IntoIter>
    where
        I: IntoIterator<Item = BlockPos>,
        I::IntoIter: Clone,
    {
        OrderedBlockMatcher::new(self, positions.into_iter())
    }
    /// Port of LevelReader#findBlocksInBoxByManhattanDistance(origin, rangeXZ, rangeY).
    pub fn find_blocks_in_box_by_manhattan_distance(
        &self,
        origin: BlockPos,
        range_xz: i32,
        range_y: i32,
    ) -> OrderedBlockMatcher<'_, ManhattanPositions> {
        self.find_blocks_at(ManhattanPositions::new(
            origin,
            range_xz,
            range_y,
            range_xz.wrapping_mul(2).wrapping_add(range_y),
        ))
    }
    /// Port of LevelReader#findBlocksInManhattan.
    pub fn find_blocks_in_manhattan(
        &self,
        origin: BlockPos,
        range: i32,
    ) -> OrderedBlockMatcher<'_, ManhattanPositions> {
        self.find_blocks_at(ManhattanPositions::new(origin, range, range, range))
    }
    /// Port of LevelReader#containsAnyLiquid. Waterlogged and flowing states use
    /// the same canonical state fluid accessor as liquid blocks.
    pub fn contains_any_liquid(&self, box_: Aabb) -> Result<bool, LevelError> {
        self.find_blocks_in(box_)
            .filter_state(|state| !state.data().fluid.empty)
            .any_matched()
    }
    /// Port of LevelReader#getBlockStatesIfLoaded. The presence check never
    /// acquires chunks; a successful result retains BlockGetter's lazy X/Y/Z
    /// block stream, rather than the section order used by BoxBlockMatcher.
    pub fn get_block_states_if_loaded(
        &self,
        box_: Aabb,
    ) -> impl Iterator<Item = Result<BlockState, LevelError>> + '_ {
        let from = BlockPos::containing(box_.min_x, box_.min_y, box_.min_z);
        let to = BlockPos::containing(box_.max_x, box_.max_y, box_.max_z);
        let mut loaded = to.y >= self.min_build_height() && from.y <= self.max_build_height();
        if loaded {
            'chunks: for x in (from.x >> 4)..=(to.x >> 4) {
                for z in (from.z >> 4)..=(to.z >> 4) {
                    if !self.has_chunk(ferraloom_util::ChunkPos::new(x, z)) {
                        loaded = false;
                        break 'chunks;
                    }
                }
            }
        }
        loaded
            .then(|| BlockPos::between_closed(from, to))
            .into_iter()
            .flatten()
            .map(|pos| self.get_block_state(pos))
    }
}

#[cfg(test)]
mod tests;
