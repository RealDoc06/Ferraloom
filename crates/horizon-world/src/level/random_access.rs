//! Short access to the level's canonical RNG, including its Gaussian cache.
use super::LevelAccess;
use horizon_core::{
    position::BlockPos,
    random::{InvalidBound, LegacyPositionalRandom, LegacyRandom, PositionalRandom, RandomSource},
};
use std::marker::PhantomData;

/// A draw borrows canonical random state only for that primitive operation.
/// The handle may live across callbacks; a reference into the RNG may not.
pub struct LevelRandom<'a, L: LevelAccess + ?Sized>(Source<'a, L>);
enum Source<'a, L: LevelAccess + ?Sized> {
    Live(&'a mut L),
    Forked(LegacyRandom),
}
impl<'a, L: LevelAccess + ?Sized> LevelRandom<'a, L> {
    #[inline]
    pub(super) fn new(level: &'a mut L) -> Self {
        Self(Source::Live(level))
    }
    #[inline]
    fn draw<R>(&mut self, draw: impl FnOnce(&mut LegacyRandom) -> R) -> R {
        match &mut self.0 {
            Source::Live(level) => level.with_random(draw),
            Source::Forked(random) => draw(random),
        }
    }
}

pub struct LevelPositional<'a, L: LevelAccess + ?Sized> {
    random: LegacyPositionalRandom,
    level_type: PhantomData<fn() -> LevelRandom<'a, L>>,
}
impl<'a, L: LevelAccess + ?Sized> RandomSource for LevelRandom<'a, L> {
    type Positional = LevelPositional<'a, L>;

    #[inline]
    fn set_seed(&mut self, seed: i64) {
        self.draw(|random| random.set_seed(seed));
    }
    #[inline]
    fn next_int(&mut self) -> i32 {
        self.draw(LegacyRandom::next_int)
    }
    #[inline]
    fn next_int_bound(&mut self, bound: i32) -> Result<i32, InvalidBound> {
        self.draw(|random| random.next_int_bound(bound))
    }
    #[inline]
    fn next_long(&mut self) -> i64 {
        self.draw(LegacyRandom::next_long)
    }
    #[inline]
    fn next_boolean(&mut self) -> bool {
        self.draw(LegacyRandom::next_boolean)
    }
    #[inline]
    fn next_float(&mut self) -> f32 {
        self.draw(LegacyRandom::next_float)
    }
    #[inline]
    fn next_double(&mut self) -> f64 {
        self.draw(LegacyRandom::next_double)
    }
    #[inline]
    fn next_gaussian(&mut self) -> f64 {
        self.draw(LegacyRandom::next_gaussian)
    }
    #[inline]
    fn fork_positional(&mut self) -> Self::Positional {
        LevelPositional {
            random: self.draw(LegacyRandom::fork_positional),
            level_type: PhantomData,
        }
    }
    #[inline]
    fn fork(&mut self) -> Self {
        Self(Source::Forked(self.draw(LegacyRandom::fork)))
    }
}
impl<'a, L: LevelAccess + ?Sized> PositionalRandom for LevelPositional<'a, L> {
    type Source = LevelRandom<'a, L>;

    fn at(&self, position: BlockPos) -> Self::Source {
        LevelRandom(Source::Forked(self.random.at(position)))
    }
    fn from_seed(&self, seed: i64) -> Self::Source {
        LevelRandom(Source::Forked(self.random.from_seed(seed)))
    }
    fn from_hash_of(&self, name: &str) -> Self::Source {
        LevelRandom(Source::Forked(self.random.from_hash_of(name)))
    }
    fn from_hash_of_utf16(&self, name: &[u16]) -> Self::Source {
        LevelRandom(Source::Forked(self.random.from_hash_of_utf16(name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{level::LevelEvent, registry::BlockStateId};
    use std::cell::RefCell;

    struct Level {
        owner: RefCell<Option<LegacyRandom>>,
    }
    impl LevelAccess for Level {
        fn block_state(&mut self, _: BlockPos) -> BlockStateId {
            BlockStateId(0)
        }
        fn set_raw(&mut self, _: BlockPos, _: BlockStateId) -> Option<BlockStateId> {
            unreachable!()
        }
        fn is_loaded(&mut self, _: BlockPos) -> bool {
            true
        }
        fn with_random<R>(&mut self, draw: impl FnOnce(&mut LegacyRandom) -> R) -> R {
            draw(self.owner.borrow_mut().as_mut().expect("live owner"))
        }
        fn game_time(&self) -> i64 {
            0
        }
        fn emit(&mut self, _: LevelEvent) {}
    }

    #[test]
    fn owner_handoff_keeps_the_canonical_gaussian_cache_and_draw_sequence() {
        for seed in [0, 42, -1, i64::MAX] {
            let mut level = Level {
                owner: RefCell::new(Some(LegacyRandom::new(seed))),
            };
            let mut expected = LegacyRandom::new(seed);
            assert_eq!(level.random().next_int(), expected.next_int());
            assert_eq!(
                level.random().next_int_bound(17),
                expected.next_int_bound(17)
            );
            assert_eq!(level.random().next_float(), expected.next_float());
            assert_eq!(level.random().next_double(), expected.next_double());
            assert_eq!(level.random().next_gaussian(), expected.next_gaussian());

            // A synchronous callback can receive this same owner after the
            // primitive draw returns. Its draw cannot discard the cached pair.
            let mut returned = level.owner.borrow_mut().take().unwrap();
            assert_eq!(returned.next_int_bound(97), expected.next_int_bound(97));
            *level.owner.borrow_mut() = Some(returned);

            assert_eq!(level.random().next_gaussian(), expected.next_gaussian());
            assert_eq!(level.random().next_long(), expected.next_long());
            assert_eq!(level.random().next_boolean(), expected.next_boolean());
            assert_eq!(level.random().next_int_bound(0), expected.next_int_bound(0));
            let mut fork = level.random().fork();
            let mut expected_fork = expected.fork();
            assert_eq!(fork.next_long(), expected_fork.next_long());
            let positional = level.random().fork_positional();
            let expected_positional = expected.fork_positional();
            assert_eq!(
                positional.from_hash_of("region").next_long(),
                expected_positional.from_hash_of("region").next_long(),
            );
            assert_eq!(level.with_random(|random| random.state()), expected.state());
        }
    }
}
