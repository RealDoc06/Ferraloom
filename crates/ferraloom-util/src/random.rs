//! Ports of RandomSource and the `world.level.levelgen` random family.
//!
//! Exclusive Rust borrows replace LegacyRandomSource's concurrent-use detector;
//! the 48-bit state transition and every consumed bit remain identical.
use crate::{java, mth};
use md5::{Digest, Md5};

pub const GOLDEN_RATIO_64: i64 = -7046029254386353131;
pub const SILVER_RATIO_64: i64 = 7640891576956012809;

/// Port of RandomSupport.Seed128bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seed128bit {
    pub seed_lo: i64,
    pub seed_hi: i64,
}
impl Seed128bit {
    #[inline]
    pub fn xor(self, other: Self) -> Self {
        Self {
            seed_lo: self.seed_lo ^ other.seed_lo,
            seed_hi: self.seed_hi ^ other.seed_hi,
        }
    }
    #[inline]
    pub fn mixed(self) -> Self {
        Self {
            seed_lo: mix_stafford13(self.seed_lo),
            seed_hi: mix_stafford13(self.seed_hi),
        }
    }
}
/// Port of RandomSupport#mixStafford13.
#[inline]
pub fn mix_stafford13(z: i64) -> i64 {
    let mut z = z as u64;
    z = (z ^ (z >> 30)).wrapping_mul(-4658895280553007687_i64 as u64);
    z = (z ^ (z >> 27)).wrapping_mul(-7723592293110705685_i64 as u64);
    (z ^ (z >> 31)) as i64
}
#[inline]
pub fn upgrade_seed_to_128bit_unmixed(seed: i64) -> Seed128bit {
    let seed_lo = seed ^ SILVER_RATIO_64;
    Seed128bit {
        seed_lo,
        seed_hi: seed_lo.wrapping_add(GOLDEN_RATIO_64),
    }
}
#[inline]
pub fn upgrade_seed_to_128bit(seed: i64) -> Seed128bit {
    upgrade_seed_to_128bit_unmixed(seed).mixed()
}
/// Port of RandomSupport#seedFromHashOf. MD5 bytes are read big-endian.
pub fn seed_from_hash_of(name: &str) -> Seed128bit {
    let digest = Md5::digest(name.as_bytes());
    Seed128bit {
        seed_lo: i64::from_be_bytes(digest[..8].try_into().unwrap()),
        seed_hi: i64::from_be_bytes(digest[8..].try_into().unwrap()),
    }
}

/// Port of RandomSource, with BitRandomSource's defaults. Xoroshiro overrides
/// the defaults that consume its 64-bit stream differently.
pub trait RandomSource {
    fn set_seed(&mut self, seed: i64);
    fn next_bits(&mut self, bits: u32) -> i32;
    fn next_gaussian(&mut self) -> f64;
    fn fork(&mut self) -> Random;
    fn fork_positional(&mut self) -> PositionalRandomFactory;
    /// WorldgenRandom tests `instanceof LegacyRandomSource`, not BitRandomSource.
    fn legacy_next_bits(&mut self, _bits: u32) -> Option<i32> {
        None
    }
    #[inline]
    fn next_int(&mut self) -> i32 {
        self.next_bits(32)
    }
    #[inline]
    fn next_int_bound(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "bound must be positive");
        if (bound & bound.wrapping_sub(1)) == 0 {
            return ((i64::from(bound) * i64::from(self.next_bits(31))) >> 31) as i32;
        }
        loop {
            let sample = self.next_bits(31);
            let modulo = sample % bound;
            if sample.wrapping_sub(modulo).wrapping_add(bound - 1) >= 0 {
                return modulo;
            }
        }
    }
    #[inline]
    fn next_long(&mut self) -> i64 {
        let upper = self.next_bits(32);
        let lower = self.next_bits(32);
        (i64::from(upper) << 32).wrapping_add(i64::from(lower))
    }
    #[inline]
    fn next_boolean(&mut self) -> bool {
        self.next_bits(1) != 0
    }
    #[inline]
    fn next_float(&mut self) -> f32 {
        self.next_bits(24) as f32 * 5.9604645e-8_f32
    }
    #[inline]
    fn next_double(&mut self) -> f64 {
        let upper = self.next_bits(26);
        let lower = self.next_bits(27);
        ((i64::from(upper) << 27) + i64::from(lower)) as f64 * 1.1102230246251565e-16
    }
    #[inline]
    fn next_int_between_inclusive(&mut self, min: i32, max: i32) -> i32 {
        self.next_int_bound(max.wrapping_sub(min).wrapping_add(1))
            .wrapping_add(min)
    }
    #[inline]
    fn next_int_between(&mut self, origin: i32, bound: i32) -> i32 {
        assert!(origin < bound, "bound - origin is non positive");
        origin.wrapping_add(self.next_int_bound(bound.wrapping_sub(origin)))
    }
    #[inline]
    fn triangle(&mut self, mean: f64, spread: f64) -> f64 {
        mean + spread * (self.next_double() - self.next_double())
    }
    #[inline]
    fn triangle_f32(&mut self, mean: f32, spread: f32) -> f32 {
        mean + spread * (self.next_float() - self.next_float())
    }
    fn consume_count(&mut self, rounds: i32) {
        for _ in 0..rounds {
            self.next_int();
        }
    }
}

/// Port of MarsagliaPolarGaussian. The cache is owned separately to avoid the
/// self-referential RandomSource field in Java.
#[derive(Clone, Debug, Default)]
pub struct MarsagliaPolarGaussian {
    next: Option<f64>,
}
impl MarsagliaPolarGaussian {
    pub fn reset(&mut self) {
        self.next = None;
    }
    pub fn next_gaussian(&mut self, random: &mut impl RandomSource) -> f64 {
        if let Some(value) = self.next.take() {
            return value;
        }
        loop {
            let x = 2.0 * random.next_double() - 1.0;
            let y = 2.0 * random.next_double() - 1.0;
            let r = x * x + y * y;
            if r >= 1.0 || r == 0.0 {
                continue;
            }
            let multiplier = (-2.0 * r.ln() / r).sqrt();
            self.next = Some(y * multiplier);
            return x * multiplier;
        }
    }
}

/// Port of LegacyRandomSource.
#[derive(Clone, Debug)]
pub struct LegacyRandomSource {
    seed: u64,
    gaussian: MarsagliaPolarGaussian,
}
impl LegacyRandomSource {
    pub fn new(seed: i64) -> Self {
        let mut r = Self {
            seed: 0,
            gaussian: MarsagliaPolarGaussian::default(),
        };
        r.set_seed(seed);
        r
    }
    #[inline]
    pub fn next(&mut self, bits: u32) -> i32 {
        self.next_bits(bits)
    }
}
impl RandomSource for LegacyRandomSource {
    fn set_seed(&mut self, seed: i64) {
        self.seed = (seed as u64 ^ 25214903917) & ((1 << 48) - 1);
        self.gaussian.reset();
    }
    #[inline]
    fn next_bits(&mut self, bits: u32) -> i32 {
        self.seed = self.seed.wrapping_mul(25214903917).wrapping_add(11) & ((1 << 48) - 1);
        (self.seed as i64).wrapping_shr(48_u32.wrapping_sub(bits)) as i32
    }
    fn legacy_next_bits(&mut self, bits: u32) -> Option<i32> {
        Some(self.next_bits(bits))
    }
    fn next_gaussian(&mut self) -> f64 {
        let mut gaussian = std::mem::take(&mut self.gaussian);
        let result = gaussian.next_gaussian(self);
        self.gaussian = gaussian;
        result
    }
    fn fork(&mut self) -> Random {
        Random::Legacy(Self::new(self.next_long()))
    }
    fn fork_positional(&mut self) -> PositionalRandomFactory {
        PositionalRandomFactory::Legacy(LegacyPositionalRandomFactory {
            seed: self.next_long(),
        })
    }
}

/// Port of SingleThreadedRandomSource; its stream equals LegacyRandomSource,
/// but WorldgenRandom intentionally does not treat it as LegacyRandomSource.
#[derive(Clone, Debug)]
pub struct SingleThreadedRandomSource(LegacyRandomSource);
impl SingleThreadedRandomSource {
    pub fn new(seed: i64) -> Self {
        Self(LegacyRandomSource::new(seed))
    }
}
impl RandomSource for SingleThreadedRandomSource {
    fn set_seed(&mut self, seed: i64) {
        self.0.set_seed(seed);
    }
    #[inline]
    fn next_bits(&mut self, bits: u32) -> i32 {
        self.0.next_bits(bits)
    }
    fn next_gaussian(&mut self) -> f64 {
        self.0.next_gaussian()
    }
    fn fork(&mut self) -> Random {
        Random::SingleThreaded(Self::new(self.next_long()))
    }
    fn fork_positional(&mut self) -> PositionalRandomFactory {
        self.0.fork_positional()
    }
}

/// Port of Xoroshiro128PlusPlus (128-bit state, 64-bit output).
#[derive(Clone, Debug)]
pub struct Xoroshiro128PlusPlus {
    pub seed_lo: u64,
    pub seed_hi: u64,
}
impl Xoroshiro128PlusPlus {
    pub fn new(seed_lo: i64, seed_hi: i64) -> Self {
        if seed_lo | seed_hi == 0 {
            Self {
                seed_lo: GOLDEN_RATIO_64 as u64,
                seed_hi: SILVER_RATIO_64 as u64,
            }
        } else {
            Self {
                seed_lo: seed_lo as u64,
                seed_hi: seed_hi as u64,
            }
        }
    }
    #[inline]
    pub fn next_long(&mut self) -> i64 {
        let s0 = self.seed_lo;
        let mut s1 = self.seed_hi;
        let result = s0.wrapping_add(s1).rotate_left(17).wrapping_add(s0);
        s1 ^= s0;
        self.seed_lo = s0.rotate_left(49) ^ s1 ^ (s1 << 21);
        self.seed_hi = s1.rotate_left(28);
        result as i64
    }
}

/// Port of XoroshiroRandomSource.
#[derive(Clone, Debug)]
pub struct XoroshiroRandomSource {
    generator: Xoroshiro128PlusPlus,
    gaussian: MarsagliaPolarGaussian,
}
impl XoroshiroRandomSource {
    /// Port of XoroshiroRandomSource.CODEC: persist the two raw generator words.
    /// Gaussian cache is intentionally omitted, as in vanilla saved random sequences.
    pub fn seed_state(&self) -> Seed128bit {
        Seed128bit {
            seed_lo: self.generator.seed_lo as i64,
            seed_hi: self.generator.seed_hi as i64,
        }
    }
    pub fn new(seed: i64) -> Self {
        Self::from_seed(upgrade_seed_to_128bit(seed))
    }
    pub fn from_seed(seed: Seed128bit) -> Self {
        Self::from_words(seed.seed_lo, seed.seed_hi)
    }
    pub fn from_words(lo: i64, hi: i64) -> Self {
        Self {
            generator: Xoroshiro128PlusPlus::new(lo, hi),
            gaussian: MarsagliaPolarGaussian::default(),
        }
    }
}
impl RandomSource for XoroshiroRandomSource {
    fn set_seed(&mut self, seed: i64) {
        *self = Self::new(seed);
    }
    #[inline]
    fn next_bits(&mut self, bits: u32) -> i32 {
        (self.generator.next_long() as u64).wrapping_shr(64_u32.wrapping_sub(bits)) as i32
    }
    #[inline]
    fn next_int(&mut self) -> i32 {
        self.generator.next_long() as i32
    }
    #[inline]
    fn next_int_bound(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "bound must be positive");
        let mut product = u64::from(self.next_int() as u32) * bound as u64;
        if (product as u32) < bound as u32 {
            let threshold = (bound as u32).wrapping_neg() % bound as u32;
            while (product as u32) < threshold {
                product = u64::from(self.next_int() as u32) * bound as u64;
            }
        }
        (product >> 32) as i32
    }
    #[inline]
    fn next_long(&mut self) -> i64 {
        self.generator.next_long()
    }
    #[inline]
    fn next_boolean(&mut self) -> bool {
        self.generator.next_long() & 1 != 0
    }
    #[inline]
    fn next_float(&mut self) -> f32 {
        ((self.generator.next_long() as u64) >> 40) as f32 * 5.9604645e-8_f32
    }
    #[inline]
    fn next_double(&mut self) -> f64 {
        ((self.generator.next_long() as u64) >> 11) as f64 * 1.1102230246251565e-16
    }
    fn next_gaussian(&mut self) -> f64 {
        let mut gaussian = std::mem::take(&mut self.gaussian);
        let result = gaussian.next_gaussian(self);
        self.gaussian = gaussian;
        result
    }
    fn fork(&mut self) -> Random {
        Random::Xoroshiro(Self::from_words(
            self.generator.next_long(),
            self.generator.next_long(),
        ))
    }
    fn fork_positional(&mut self) -> PositionalRandomFactory {
        PositionalRandomFactory::Xoroshiro(XoroshiroPositionalRandomFactory {
            seed_lo: self.generator.next_long(),
            seed_hi: self.generator.next_long(),
        })
    }
}

/// Port of LegacyRandomSource.LegacyPositionalRandomFactory.
#[derive(Clone, Copy, Debug)]
pub struct LegacyPositionalRandomFactory {
    pub seed: i64,
}
impl LegacyPositionalRandomFactory {
    pub fn at(self, x: i32, y: i32, z: i32) -> LegacyRandomSource {
        LegacyRandomSource::new(mth::get_seed(x, y, z) ^ self.seed)
    }
    pub fn from_hash_of(self, name: &str) -> LegacyRandomSource {
        LegacyRandomSource::new(i64::from(java::string_hash_code(name)) ^ self.seed)
    }
    pub fn from_seed(self, seed: i64) -> LegacyRandomSource {
        LegacyRandomSource::new(seed)
    }
}
/// Port of XoroshiroRandomSource.XoroshiroPositionalRandomFactory.
#[derive(Clone, Copy, Debug)]
pub struct XoroshiroPositionalRandomFactory {
    pub seed_lo: i64,
    pub seed_hi: i64,
}
impl XoroshiroPositionalRandomFactory {
    pub fn at(self, x: i32, y: i32, z: i32) -> XoroshiroRandomSource {
        XoroshiroRandomSource::from_words(mth::get_seed(x, y, z) ^ self.seed_lo, self.seed_hi)
    }
    pub fn from_hash_of(self, name: &str) -> XoroshiroRandomSource {
        XoroshiroRandomSource::from_seed(seed_from_hash_of(name).xor(Seed128bit {
            seed_lo: self.seed_lo,
            seed_hi: self.seed_hi,
        }))
    }
    pub fn from_seed(self, seed: i64) -> XoroshiroRandomSource {
        XoroshiroRandomSource::from_words(seed ^ self.seed_lo, seed ^ self.seed_hi)
    }
}
#[derive(Clone, Copy, Debug)]
pub enum PositionalRandomFactory {
    Legacy(LegacyPositionalRandomFactory),
    Xoroshiro(XoroshiroPositionalRandomFactory),
}
impl PositionalRandomFactory {
    pub fn at(self, x: i32, y: i32, z: i32) -> Random {
        match self {
            Self::Legacy(f) => Random::Legacy(f.at(x, y, z)),
            Self::Xoroshiro(f) => Random::Xoroshiro(f.at(x, y, z)),
        }
    }
    pub fn from_hash_of(self, name: &str) -> Random {
        match self {
            Self::Legacy(f) => Random::Legacy(f.from_hash_of(name)),
            Self::Xoroshiro(f) => Random::Xoroshiro(f.from_hash_of(name)),
        }
    }
    pub fn from_seed(self, seed: i64) -> Random {
        match self {
            Self::Legacy(f) => Random::Legacy(f.from_seed(seed)),
            Self::Xoroshiro(f) => Random::Xoroshiro(f.from_seed(seed)),
        }
    }
}

/// Closed set used where vanilla returns a polymorphic RandomSource.
#[derive(Clone, Debug)]
pub enum Random {
    Legacy(LegacyRandomSource),
    SingleThreaded(SingleThreadedRandomSource),
    Xoroshiro(XoroshiroRandomSource),
}
macro_rules! dispatch { ($self:ident, $method:ident $(, $arg:ident)*) => { match $self { Self::Legacy(r) => r.$method($($arg),*), Self::SingleThreaded(r) => r.$method($($arg),*), Self::Xoroshiro(r) => r.$method($($arg),*) } }; }
impl RandomSource for Random {
    fn set_seed(&mut self, seed: i64) {
        dispatch!(self, set_seed, seed);
    }
    #[inline]
    fn next_bits(&mut self, bits: u32) -> i32 {
        dispatch!(self, next_bits, bits)
    }
    #[inline]
    fn next_int(&mut self) -> i32 {
        dispatch!(self, next_int)
    }
    #[inline]
    fn next_int_bound(&mut self, bound: i32) -> i32 {
        dispatch!(self, next_int_bound, bound)
    }
    #[inline]
    fn next_long(&mut self) -> i64 {
        dispatch!(self, next_long)
    }
    #[inline]
    fn next_boolean(&mut self) -> bool {
        dispatch!(self, next_boolean)
    }
    #[inline]
    fn next_float(&mut self) -> f32 {
        dispatch!(self, next_float)
    }
    #[inline]
    fn next_double(&mut self) -> f64 {
        dispatch!(self, next_double)
    }
    fn next_gaussian(&mut self) -> f64 {
        dispatch!(self, next_gaussian)
    }
    fn fork(&mut self) -> Random {
        dispatch!(self, fork)
    }
    fn fork_positional(&mut self) -> PositionalRandomFactory {
        dispatch!(self, fork_positional)
    }
    fn legacy_next_bits(&mut self, bits: u32) -> Option<i32> {
        dispatch!(self, legacy_next_bits, bits)
    }
}

/// Port of WorldgenRandom.Algorithm.
#[derive(Clone, Copy, Debug)]
pub enum Algorithm {
    Legacy,
    Xoroshiro,
}
impl Algorithm {
    pub fn new_instance(self, seed: i64) -> Random {
        match self {
            Self::Legacy => Random::Legacy(LegacyRandomSource::new(seed)),
            Self::Xoroshiro => Random::Xoroshiro(XoroshiroRandomSource::new(seed)),
        }
    }
}
/// Port of WorldgenRandom. Its inherited Gaussian cache survives setSeed.
#[derive(Clone, Debug)]
pub struct WorldgenRandom<R = Random> {
    source: R,
    count: i32,
    gaussian: MarsagliaPolarGaussian,
}
impl<R: RandomSource> WorldgenRandom<R> {
    pub fn new(source: R) -> Self {
        Self {
            source,
            count: 0,
            gaussian: MarsagliaPolarGaussian::default(),
        }
    }
    pub fn count(&self) -> i32 {
        self.count
    }
    pub fn set_decoration_seed(&mut self, seed: i64, x: i32, z: i32) -> i64 {
        self.set_seed(seed);
        let xs = self.next_long() | 1;
        let zs = self.next_long() | 1;
        let result = i64::from(x)
            .wrapping_mul(xs)
            .wrapping_add(i64::from(z).wrapping_mul(zs))
            ^ seed;
        self.set_seed(result);
        result
    }
    pub fn set_feature_seed(&mut self, seed: i64, index: i32, step: i32) {
        self.set_seed(
            seed.wrapping_add(i64::from(index))
                .wrapping_add(i64::from(10000_i32.wrapping_mul(step))),
        );
    }
    pub fn set_large_feature_seed(&mut self, seed: i64, x: i32, z: i32) {
        self.set_seed(seed);
        let xs = self.next_long();
        let zs = self.next_long();
        self.set_seed(i64::from(x).wrapping_mul(xs) ^ i64::from(z).wrapping_mul(zs) ^ seed);
    }
    pub fn set_large_feature_with_salt(&mut self, seed: i64, x: i32, z: i32, salt: i32) {
        self.set_seed(
            i64::from(x)
                .wrapping_mul(341873128712)
                .wrapping_add(i64::from(z).wrapping_mul(132897987541))
                .wrapping_add(seed)
                .wrapping_add(i64::from(salt)),
        );
    }
}
impl<R: RandomSource> RandomSource for WorldgenRandom<R> {
    fn set_seed(&mut self, seed: i64) {
        self.source.set_seed(seed);
    }
    #[inline]
    fn next_bits(&mut self, bits: u32) -> i32 {
        self.count = self.count.wrapping_add(1);
        self.source.legacy_next_bits(bits).unwrap_or_else(|| {
            (self.source.next_long() as u64).wrapping_shr(64_u32.wrapping_sub(bits)) as i32
        })
    }
    fn legacy_next_bits(&mut self, bits: u32) -> Option<i32> {
        Some(self.next_bits(bits))
    }
    fn next_gaussian(&mut self) -> f64 {
        let mut gaussian = std::mem::take(&mut self.gaussian);
        let result = gaussian.next_gaussian(self);
        self.gaussian = gaussian;
        result
    }
    fn fork(&mut self) -> Random {
        self.source.fork()
    }
    fn fork_positional(&mut self) -> PositionalRandomFactory {
        self.source.fork_positional()
    }
}
/// Port of WorldgenRandom#seedSlimeChunk, preserving int multiplication before
/// the expressions are widened to long.
pub fn seed_slime_chunk(x: i32, z: i32, seed: i64, salt: i64) -> SingleThreadedRandomSource {
    let seed = seed
        .wrapping_add(i64::from(x.wrapping_mul(x).wrapping_mul(4987142)))
        .wrapping_add(i64::from(x.wrapping_mul(5947611)))
        .wrapping_add(i64::from(z.wrapping_mul(z)).wrapping_mul(4392871))
        .wrapping_add(i64::from(z.wrapping_mul(389711)))
        ^ salt;
    SingleThreadedRandomSource::new(seed)
}

#[cfg(test)]
mod state_tests {
    use super::*;
    #[test]
    fn xoroshiro_saved_state_resumes_next_draw() {
        let mut source = XoroshiroRandomSource::new(-90210);
        source.next_int_bound(79);
        source.next_double();
        let mut restored = XoroshiroRandomSource::from_seed(source.seed_state());
        for _ in 0..32 {
            assert_eq!(source.next_long(), restored.next_long());
        }
    }
}
