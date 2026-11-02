//! The live feature RNG is the level's canonical stream, including callback draws.
use horizon_core::{
    position::BlockPos,
    random::{InvalidBound, LegacyPositionalRandom, LegacyRandom, PositionalRandom, RandomSource},
};
use std::{cell::RefCell, marker::PhantomData};

pub(super) trait FeatureRandomAccess {
    fn with_feature_random<R>(&mut self, draw: impl FnOnce(&mut LegacyRandom) -> R) -> R;
}
impl<C: super::tick_access::TickContext> FeatureRandomAccess for super::Sim<'_, C> {
    fn with_feature_random<R>(&mut self, draw: impl FnOnce(&mut LegacyRandom) -> R) -> R {
        super::LevelAccess::with_random(&mut *self.level, draw)
    }
}
impl<T: FeatureRandomAccess + ?Sized> FeatureRandomAccess for &mut T {
    fn with_feature_random<R>(&mut self, draw: impl FnOnce(&mut LegacyRandom) -> R) -> R {
        (**self).with_feature_random(draw)
    }
}

/// `TreeGrower#growTree` passes the same `RandomSource` into feature placement.
/// Only the context handle is shared with the feature-world facade. Each draw
/// ends its RNG borrow before a world callback can run; no stream is copied out
/// and restored. A vanilla fork owns its newly seeded independent stream.
pub(super) enum FeatureRandomSelector<'a, C: FeatureRandomAccess + 'a> {
    Live(&'a RefCell<C>),
    Forked(LegacyRandom),
}
impl<C: FeatureRandomAccess> FeatureRandomSelector<'_, C> {
    fn with_random<R>(&mut self, draw: impl FnOnce(&mut LegacyRandom) -> R) -> R {
        match self {
            Self::Live(context) => context.borrow_mut().with_feature_random(draw),
            Self::Forked(random) => draw(random),
        }
    }
}
pub(super) struct FeaturePositional<'a, C: FeatureRandomAccess + 'a> {
    source: LegacyPositionalRandom,
    context_type: PhantomData<fn() -> FeatureRandomSelector<'a, C>>,
}
impl<'a, C: FeatureRandomAccess + 'a> RandomSource for FeatureRandomSelector<'a, C> {
    type Positional = FeaturePositional<'a, C>;

    fn set_seed(&mut self, seed: i64) {
        self.with_random(|random| random.set_seed(seed));
    }
    fn next_int(&mut self) -> i32 {
        self.with_random(LegacyRandom::next_int)
    }
    fn next_int_bound(&mut self, bound: i32) -> Result<i32, InvalidBound> {
        self.with_random(|random| random.next_int_bound(bound))
    }
    fn next_long(&mut self) -> i64 {
        self.with_random(LegacyRandom::next_long)
    }
    fn next_boolean(&mut self) -> bool {
        self.with_random(LegacyRandom::next_boolean)
    }
    fn next_float(&mut self) -> f32 {
        self.with_random(LegacyRandom::next_float)
    }
    fn next_double(&mut self) -> f64 {
        self.with_random(LegacyRandom::next_double)
    }
    fn next_gaussian(&mut self) -> f64 {
        self.with_random(LegacyRandom::next_gaussian)
    }
    fn fork_positional(&mut self) -> Self::Positional {
        FeaturePositional {
            source: self.with_random(LegacyRandom::fork_positional),
            context_type: PhantomData,
        }
    }
    fn fork(&mut self) -> Self {
        Self::Forked(self.with_random(LegacyRandom::fork))
    }
}
impl<'a, C: FeatureRandomAccess + 'a> PositionalRandom for FeaturePositional<'a, C> {
    type Source = FeatureRandomSelector<'a, C>;

    fn at(&self, position: BlockPos) -> Self::Source {
        FeatureRandomSelector::Forked(self.source.at(position))
    }
    fn from_seed(&self, seed: i64) -> Self::Source {
        FeatureRandomSelector::Forked(self.source.from_seed(seed))
    }
    fn from_hash_of(&self, name: &str) -> Self::Source {
        FeatureRandomSelector::Forked(self.source.from_hash_of(name))
    }
    fn from_hash_of_utf16(&self, name: &[u16]) -> Self::Source {
        FeatureRandomSelector::Forked(self.source.from_hash_of_utf16(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Level(LegacyRandom);
    impl FeatureRandomAccess for Level {
        fn with_feature_random<R>(&mut self, draw: impl FnOnce(&mut LegacyRandom) -> R) -> R {
            draw(&mut self.0)
        }
    }
    impl Level {
        fn placement_callback(context: &RefCell<Self>) -> i32 {
            // A callback reenters feature RNG access while the outer feature's
            // handle remains alive, without retaining a canonical-state borrow.
            FeatureRandomSelector::Live(context)
                .next_int_bound(97)
                .unwrap()
        }
    }

    #[test]
    fn reentrant_feature_callback_keeps_the_same_stream_and_gaussian_cache() {
        for seed in [0, 42, -1, i64::MAX] {
            let context = RefCell::new(Level(LegacyRandom::new(seed)));
            let mut feature = FeatureRandomSelector::Live(&context);
            let mut vanilla = LegacyRandom::new(seed);
            assert_eq!(feature.next_int_bound(17), vanilla.next_int_bound(17));
            assert_eq!(
                Level::placement_callback(&context),
                vanilla.next_int_bound(97).unwrap()
            );
            assert_eq!(feature.next_gaussian(), vanilla.next_gaussian());
            assert_eq!(
                Level::placement_callback(&context),
                vanilla.next_int_bound(97).unwrap()
            );
            // Keep the cached second Gaussian in that same canonical object.
            assert_eq!(feature.next_gaussian(), vanilla.next_gaussian());
            assert_eq!(feature.next_long(), vanilla.next_long());
            assert_eq!(context.borrow().0.state(), vanilla.state());
        }
    }
}
