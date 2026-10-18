//! The generation RNG and the live caller's RNG are borrowed without reseeding.
use crate::Error;
use ferraloom_util::random::{PositionalRandomFactory, Random, RandomSource, WorldgenRandom};

/// Region placement retains WorldgenRandom; live placement consumes the caller's
/// actual RandomSource, whose float/double methods may use different bits.
pub enum FeatureRandom<'a> {
    Region(&'a mut WorldgenRandom),
    Live(&'a mut dyn RandomSource),
}
impl FeatureRandom<'_> {
    pub(crate) fn region_mut(&mut self) -> Result<&mut WorldgenRandom, Error> {
        match self {
            Self::Region(random) => Ok(random),
            Self::Live(_) => Err(Error(
                "live feature random used with a generation region".into(),
            )),
        }
    }
}

/// Allows existing structure/template callers to retain their concrete RNG.
pub trait FeatureRandomSource: RandomSource {
    fn feature_random(&mut self) -> FeatureRandom<'_>;
}
impl FeatureRandomSource for WorldgenRandom {
    fn feature_random(&mut self) -> FeatureRandom<'_> {
        FeatureRandom::Region(self)
    }
}
impl FeatureRandomSource for FeatureRandom<'_> {
    fn feature_random(&mut self) -> FeatureRandom<'_> {
        match self {
            Self::Region(random) => FeatureRandom::Region(random),
            Self::Live(random) => FeatureRandom::Live(&mut **random),
        }
    }
}
impl RandomSource for FeatureRandom<'_> {
    #[inline]
    fn set_seed(&mut self, seed: i64) {
        match self {
            Self::Region(random) => random.set_seed(seed),
            Self::Live(random) => random.set_seed(seed),
        }
    }
    #[inline]
    fn next_bits(&mut self, bits: u32) -> i32 {
        match self {
            Self::Region(random) => random.next_bits(bits),
            Self::Live(random) => random.next_bits(bits),
        }
    }
    #[inline]
    fn next_int(&mut self) -> i32 {
        match self {
            Self::Region(random) => random.next_int(),
            Self::Live(random) => random.next_int(),
        }
    }
    #[inline]
    fn next_int_bound(&mut self, bound: i32) -> i32 {
        match self {
            Self::Region(random) => random.next_int_bound(bound),
            Self::Live(random) => random.next_int_bound(bound),
        }
    }
    #[inline]
    fn next_long(&mut self) -> i64 {
        match self {
            Self::Region(random) => random.next_long(),
            Self::Live(random) => random.next_long(),
        }
    }
    #[inline]
    fn next_boolean(&mut self) -> bool {
        match self {
            Self::Region(random) => random.next_boolean(),
            Self::Live(random) => random.next_boolean(),
        }
    }
    #[inline]
    fn next_float(&mut self) -> f32 {
        match self {
            Self::Region(random) => random.next_float(),
            Self::Live(random) => random.next_float(),
        }
    }
    #[inline]
    fn next_double(&mut self) -> f64 {
        match self {
            Self::Region(random) => random.next_double(),
            Self::Live(random) => random.next_double(),
        }
    }
    #[inline]
    fn next_gaussian(&mut self) -> f64 {
        match self {
            Self::Region(random) => random.next_gaussian(),
            Self::Live(random) => random.next_gaussian(),
        }
    }
    #[inline]
    fn fork(&mut self) -> Random {
        match self {
            Self::Region(random) => random.fork(),
            Self::Live(random) => random.fork(),
        }
    }
    #[inline]
    fn fork_positional(&mut self) -> PositionalRandomFactory {
        match self {
            Self::Region(random) => random.fork_positional(),
            Self::Live(random) => random.fork_positional(),
        }
    }
    #[inline]
    fn legacy_next_bits(&mut self, bits: u32) -> Option<i32> {
        match self {
            Self::Region(random) => random.legacy_next_bits(bits),
            Self::Live(random) => random.legacy_next_bits(bits),
        }
    }
    #[inline]
    fn next_int_between_inclusive(&mut self, min: i32, max: i32) -> i32 {
        match self {
            Self::Region(random) => random.next_int_between_inclusive(min, max),
            Self::Live(random) => random.next_int_between_inclusive(min, max),
        }
    }
    #[inline]
    fn next_int_between(&mut self, origin: i32, bound: i32) -> i32 {
        match self {
            Self::Region(random) => random.next_int_between(origin, bound),
            Self::Live(random) => random.next_int_between(origin, bound),
        }
    }
    #[inline]
    fn triangle(&mut self, mean: f64, spread: f64) -> f64 {
        match self {
            Self::Region(random) => random.triangle(mean, spread),
            Self::Live(random) => random.triangle(mean, spread),
        }
    }
    #[inline]
    fn triangle_f32(&mut self, mean: f32, spread: f32) -> f32 {
        match self {
            Self::Region(random) => random.triangle_f32(mean, spread),
            Self::Live(random) => random.triangle_f32(mean, spread),
        }
    }
    #[inline]
    fn consume_count(&mut self, rounds: i32) {
        match self {
            Self::Region(random) => random.consume_count(rounds),
            Self::Live(random) => random.consume_count(rounds),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_util::random::Algorithm;

    #[test]
    fn feature_random_preserves_live_and_region_streams() {
        fn compare(actual: &mut impl RandomSource, expected: &mut impl RandomSource) {
            for bound in [1, 2, 3, 17, 1234567, i32::MAX] {
                assert_eq!(actual.next_int_bound(bound), expected.next_int_bound(bound));
                assert_eq!(
                    actual.next_float().to_bits(),
                    expected.next_float().to_bits()
                );
                assert_eq!(
                    actual.next_double().to_bits(),
                    expected.next_double().to_bits()
                );
                assert_eq!(
                    actual.next_gaussian().to_bits(),
                    expected.next_gaussian().to_bits()
                );
                assert_eq!(actual.next_boolean(), expected.next_boolean());
                assert_eq!(actual.next_long(), expected.next_long());
            }
        }
        for algorithm in [Algorithm::Legacy, Algorithm::Xoroshiro] {
            for seed in [0, 1, -1, 8675309, i64::MIN, i64::MAX] {
                let mut actual = algorithm.new_instance(seed);
                let mut expected = actual.clone();
                compare(&mut FeatureRandom::Live(&mut actual), &mut expected);
                assert_eq!(actual.next_long(), expected.next_long());
                let mut actual = WorldgenRandom::new(algorithm.new_instance(seed));
                let mut expected = actual.clone();
                compare(&mut FeatureRandom::Region(&mut actual), &mut expected);
                assert_eq!(actual.count(), expected.count());
                assert_eq!(actual.next_long(), expected.next_long());
            }
        }
    }
}
