//! Java value hashes. Identity-hashed enums/objects deliberately have no blanket
//! implementation: use the reference collections, whose order is not a parity property.
use std::cmp::Ordering;

/// Port of Object#hashCode and the optional Comparable ordering used by HashMap tree bins.
/// Implementations must agree with Java equals, and remain stable while stored.
/// `java_compare` is only for classes implementing Comparable of their own class
/// (HashMap#comparableClassFor), not inherited Comparable of a superclass.
pub trait JavaHash {
    fn java_hash_code(&self) -> i32;
    fn java_is_null(&self) -> bool {
        false
    }
    fn java_compare(&self, _other: &Self) -> Option<Ordering> {
        None
    }
}
impl<T: JavaHash + ?Sized> JavaHash for &T {
    fn java_hash_code(&self) -> i32 {
        T::java_hash_code(self)
    }
    fn java_is_null(&self) -> bool {
        T::java_is_null(self)
    }
    fn java_compare(&self, other: &Self) -> Option<Ordering> {
        T::java_compare(self, other)
    }
}
macro_rules! integer_hash {
    ($($ty:ty),*) => { $(impl JavaHash for $ty {
        fn java_hash_code(&self) -> i32 { *self as i32 }
        fn java_compare(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
    })* };
}
integer_hash!(i8, i16, u16, i32);
impl JavaHash for i64 {
    fn java_hash_code(&self) -> i32 {
        (*self as u64 ^ (*self as u64 >> 32)) as i32
    }
    fn java_compare(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl JavaHash for bool {
    fn java_hash_code(&self) -> i32 {
        if *self { 1231 } else { 1237 }
    }
    fn java_compare(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl JavaHash for str {
    fn java_hash_code(&self) -> i32 {
        crate::java::string_hash_code(self)
    }
    fn java_compare(&self, other: &Self) -> Option<Ordering> {
        Some(self.encode_utf16().cmp(other.encode_utf16()))
    }
}
impl JavaHash for String {
    fn java_hash_code(&self) -> i32 {
        self.as_str().java_hash_code()
    }
    fn java_compare(&self, other: &Self) -> Option<Ordering> {
        self.as_str().java_compare(other)
    }
}
/// `None` represents a Java null key, including fastutil's dedicated null slot.
impl<T: JavaHash> JavaHash for Option<T> {
    fn java_is_null(&self) -> bool {
        self.is_none()
    }
    fn java_hash_code(&self) -> i32 {
        self.as_ref().map_or(0, JavaHash::java_hash_code)
    }
    fn java_compare(&self, other: &Self) -> Option<Ordering> {
        self.as_ref()?.java_compare(other.as_ref()?)
    }
}
macro_rules! position_hash {
    ($($ty:ty),*) => { $(impl JavaHash for $ty {
        fn java_hash_code(&self) -> i32 { <$ty>::java_hash_code(*self) }
    })* };
}
position_hash!(crate::BlockPos, crate::SectionPos, crate::ChunkPos);
impl JavaHash for crate::Vec3i {
    fn java_hash_code(&self) -> i32 {
        Self::java_hash_code(*self)
    }
    fn java_compare(&self, other: &Self) -> Option<Ordering> {
        Some(self.compare_to(*other).cmp(&0))
    }
}
impl JavaHash for crate::Uuid {
    fn java_hash_code(&self) -> i32 {
        (self.most ^ self.least).java_hash_code()
    }
    fn java_compare(&self, other: &Self) -> Option<Ordering> {
        Some(
            self.most
                .cmp(&other.most)
                .then(self.least.cmp(&other.least)),
        )
    }
}
/// Port of java.lang.runtime.ObjectMethods record hash generation. Field hashes
/// fold from zero, unlike Objects.hash (which starts at one).
pub fn record_hash_code(fields: impl IntoIterator<Item = i32>) -> i32 {
    fields
        .into_iter()
        .fold(0_i32, |h, f| h.wrapping_mul(31).wrapping_add(f))
}
/// Port of Identifier#hashCode. ResourceKey itself inherits Object identity hashing.
pub fn identifier_hash_code(namespace: &str, path: &str) -> i32 {
    namespace
        .java_hash_code()
        .wrapping_mul(31)
        .wrapping_add(path.java_hash_code())
}

/// Port of fastutil HashCommon#mix(int).
#[inline]
pub fn mix_int(value: i32) -> u32 {
    let h = (value as u32).wrapping_mul(0x9e37_79b9);
    h ^ (h >> 16)
}
/// Port of fastutil HashCommon#mix(long).
#[inline]
pub fn mix_long(value: i64) -> u64 {
    let h = (value as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let h = h ^ (h >> 32);
    h ^ (h >> 16)
}
pub const DEFAULT_LOAD_FACTOR: f32 = 0.75;
pub const DEFAULT_INITIAL_SIZE: usize = 16;
/// Port of HashCommon#arraySize. The f64 division is intentional.
pub fn array_size(expected: usize, load_factor: f32) -> usize {
    assert!(
        load_factor > 0.0 && load_factor <= 1.0,
        "invalid fastutil load factor"
    );
    let size = ((expected as f64 / f64::from(load_factor)).ceil() as usize)
        .checked_next_power_of_two()
        .expect("table too large")
        .max(2);
    assert!(size <= 1 << 30, "table too large");
    size
}
/// Port of HashCommon#maxFill, preserving an empty probe slot even at f=1.
pub fn max_fill(capacity: usize, load_factor: f32) -> usize {
    ((capacity as f64 * f64::from(load_factor)).ceil() as usize).min(capacity - 1)
}

/// Key layout for the shared fastutil open-addressing implementation.
pub trait OpenHashKey<K> {
    fn hash(key: &K) -> usize;
    fn is_null(_key: &K) -> bool {
        false
    }
}
#[derive(Clone, Copy, Debug)]
pub struct LongHash;
impl OpenHashKey<i64> for LongHash {
    fn hash(key: &i64) -> usize {
        mix_long(*key) as usize
    }
    fn is_null(key: &i64) -> bool {
        *key == 0
    }
}
#[derive(Clone, Copy, Debug)]
pub struct IntHash;
impl OpenHashKey<i32> for IntHash {
    fn hash(key: &i32) -> usize {
        mix_int(*key) as usize
    }
    fn is_null(key: &i32) -> bool {
        *key == 0
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ShortHash;
impl OpenHashKey<i16> for ShortHash {
    fn hash(key: &i16) -> usize {
        mix_int(i32::from(*key)) as usize
    }
    fn is_null(key: &i16) -> bool {
        *key == 0
    }
}
/// JavaHash value keys. Use NullableObjectHash for Option<T> with Java null.
#[derive(Clone, Copy, Debug)]
pub struct ObjectHash;
impl<K: JavaHash> OpenHashKey<K> for ObjectHash {
    fn is_null(key: &K) -> bool {
        key.java_is_null()
    }
    fn hash(key: &K) -> usize {
        mix_int(key.java_hash_code()) as usize
    }
}
#[derive(Clone, Copy, Debug)]
pub struct NullableObjectHash;
impl<K: JavaHash> OpenHashKey<Option<K>> for NullableObjectHash {
    fn hash(key: &Option<K>) -> usize {
        mix_int(key.java_hash_code()) as usize
    }
    fn is_null(key: &Option<K>) -> bool {
        key.is_none()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CharHash;
impl OpenHashKey<u16> for CharHash {
    fn hash(key: &u16) -> usize {
        mix_int(i32::from(*key)) as usize
    }
    fn is_null(key: &u16) -> bool {
        *key == 0
    }
}

impl JavaHash for crate::MutableBlockPos {
    fn java_hash_code(&self) -> i32 {
        self.immutable().java_hash_code()
    }
}
/// Port of java.lang.Double#hashCode, including canonical NaN bits.
pub fn double_hash_code(value: f64) -> i32 {
    (crate::java::double_to_long_bits(value) as i64).java_hash_code()
}
impl JavaHash for crate::Vec3 {
    fn java_hash_code(&self) -> i32 {
        record_hash_code([self.x, self.y, self.z].map(double_hash_code))
    }
}
impl JavaHash for crate::Aabb {
    fn java_hash_code(&self) -> i32 {
        record_hash_code(
            [
                self.min_x, self.min_y, self.min_z, self.max_x, self.max_y, self.max_z,
            ]
            .map(double_hash_code),
        )
    }
}
