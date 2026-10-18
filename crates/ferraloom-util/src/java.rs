//! Java primitive semantics used by the ports. Rust casts already implement
//! Java's saturating floating-point to integer conversion (including NaN -> 0).

use crate::collections::JavaHash;
use std::{
    cmp::Ordering,
    hash::{Hash, Hasher},
};

/// Port of `Float.floatToIntBits`, which canonicalizes all NaNs.
#[inline]
pub fn float_to_int_bits(value: f32) -> u32 {
    if value.is_nan() {
        0x7fc0_0000
    } else {
        value.to_bits()
    }
}

macro_rules! java_float {
    ($name:ident, $primitive:ty, $bits:ty, $canonical:ident) => {
        /// Port of Float/Double value equality and ordering. Storage preserves raw bits;
        /// unwrap `.0` for primitive arithmetic and comparisons rather than boxed ordering.
        #[derive(Clone, Copy, Debug, Default)]
        #[repr(transparent)]
        pub struct $name(pub $primitive);

        impl $name {
            pub const fn from_bits(bits: $bits) -> Self {
                Self(<$primitive>::from_bits(bits))
            }
            pub const fn to_bits(self) -> $bits {
                self.0.to_bits()
            }
        }
        impl From<$primitive> for $name {
            fn from(value: $primitive) -> Self {
                Self(value)
            }
        }
        impl From<$name> for $primitive {
            fn from(value: $name) -> Self {
                value.0
            }
        }
        impl PartialEq for $name {
            fn eq(&self, other: &Self) -> bool {
                $canonical(self.0) == $canonical(other.0)
            }
        }
        impl Eq for $name {}
        impl PartialOrd for $name {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for $name {
            fn cmp(&self, other: &Self) -> Ordering {
                <$primitive>::from_bits($canonical(self.0))
                    .total_cmp(&<$primitive>::from_bits($canonical(other.0)))
            }
        }
        impl Hash for $name {
            fn hash<H: Hasher>(&self, state: &mut H) {
                $canonical(self.0).hash(state);
            }
        }
    };
}
java_float!(JavaF32, f32, u32, float_to_int_bits);
java_float!(JavaF64, f64, u64, double_to_long_bits);

impl JavaHash for JavaF32 {
    fn java_hash_code(&self) -> i32 {
        float_to_int_bits(self.0) as i32
    }
    fn java_compare(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl JavaHash for JavaF64 {
    fn java_hash_code(&self) -> i32 {
        let bits = double_to_long_bits(self.0);
        (bits ^ (bits >> 32)) as i32
    }
    fn java_compare(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Port of `Math.floorDiv(int, int)`, including MIN_VALUE / -1 overflow.
#[inline]
pub fn floor_div(a: i32, b: i32) -> i32 {
    let q = a.wrapping_div(b);
    if (a ^ b) < 0 && q.wrapping_mul(b) != a {
        q.wrapping_sub(1)
    } else {
        q
    }
}

/// Port of `Math.floorMod(int, int)`; the result has the divisor's sign.
#[inline]
pub fn floor_mod(a: i32, b: i32) -> i32 {
    a.wrapping_sub(floor_div(a, b).wrapping_mul(b))
}

/// Port of `Math.floorDiv(long, long)`.
#[inline]
pub fn floor_div_long(a: i64, b: i64) -> i64 {
    let q = a.wrapping_div(b);
    if (a ^ b) < 0 && q.wrapping_mul(b) != a {
        q.wrapping_sub(1)
    } else {
        q
    }
}

/// Port of `Math.floorMod(long, long)`.
#[inline]
pub fn floor_mod_long(a: i64, b: i64) -> i64 {
    a.wrapping_sub(floor_div_long(a, b).wrapping_mul(b))
}

/// Port of `String#hashCode`, which hashes UTF-16 code units, not Unicode scalars.
#[inline]
pub fn string_hash_code(value: &str) -> i32 {
    string_hash_code_utf16(value.encode_utf16())
}

/// Also accepts unpaired surrogates from Java strings.
pub fn string_hash_code_utf16(value: impl IntoIterator<Item = u16>) -> i32 {
    value.into_iter().fold(0_i32, |hash, unit| {
        hash.wrapping_mul(31).wrapping_add(i32::from(unit))
    })
}

/// Java `>>`, `>>>` and `<<` mask their shift distance, even when negative.
#[inline]
pub fn shr(value: i32, distance: i32) -> i32 {
    value.wrapping_shr(distance as u32)
}
#[inline]
pub fn ushr(value: i32, distance: i32) -> i32 {
    (value as u32).wrapping_shr(distance as u32) as i32
}
#[inline]
pub fn shl(value: i32, distance: i32) -> i32 {
    value.wrapping_shl(distance as u32)
}
#[inline]
pub fn shr_long(value: i64, distance: i32) -> i64 {
    value.wrapping_shr(distance as u32)
}
#[inline]
pub fn ushr_long(value: i64, distance: i32) -> i64 {
    (value as u64).wrapping_shr(distance as u32) as i64
}
#[inline]
pub fn shl_long(value: i64, distance: i32) -> i64 {
    value.wrapping_shl(distance as u32)
}

// Rust's min/max ignore a NaN operand; Java propagates it and orders signed zero.
#[inline]
pub fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        a
    } else if b.is_nan() {
        b
    } else if a == 0.0 && b == 0.0 {
        f64::from_bits(a.to_bits() | b.to_bits())
    } else if a <= b {
        a
    } else {
        b
    }
}
#[inline]
pub fn max(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        a
    } else if b.is_nan() {
        b
    } else if a == 0.0 && b == 0.0 {
        f64::from_bits(a.to_bits() & b.to_bits())
    } else if a >= b {
        a
    } else {
        b
    }
}
#[inline]
pub fn min_f32(a: f32, b: f32) -> f32 {
    min(f64::from(a), f64::from(b)) as f32
}
#[inline]
pub fn max_f32(a: f32, b: f32) -> f32 {
    max(f64::from(a), f64::from(b)) as f32
}

/// Java `Double.doubleToLongBits` canonicalizes NaNs.
#[inline]
pub fn double_to_long_bits(value: f64) -> u64 {
    if value.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        value.to_bits()
    }
}
#[inline]
pub fn double_equals(a: f64, b: f64) -> bool {
    double_to_long_bits(a) == double_to_long_bits(b)
}

#[cfg(test)]
mod float_tests {
    use super::*;
    use std::{collections::hash_map::DefaultHasher, fmt::Debug};

    include!("java_float_vectors.rs");

    fn compare<T: Eq + Ord + Hash + JavaHash + Debug>(a: T, b: T, equal: bool, order: i8) {
        assert_eq!(a == b, equal, "{a:?}, {b:?}");
        let expected = order.cmp(&0);
        assert_eq!(a.cmp(&b), expected, "{a:?}, {b:?}");
        assert_eq!(a.partial_cmp(&b), Some(expected));
        assert_eq!(a.java_compare(&b), Some(expected));
        if equal {
            let mut left = DefaultHasher::new();
            let mut right = DefaultHasher::new();
            a.hash(&mut left);
            b.hash(&mut right);
            assert_eq!(left.finish(), right.finish());
        }
    }

    #[test]
    fn native_float_value_semantics() {
        for &(bits, hash) in FLOAT_HASHES {
            let value = JavaF32::from_bits(bits);
            assert_eq!(value.java_hash_code(), hash);
            assert_eq!(value.to_bits(), bits);
            assert_eq!(f32::from(value).to_bits(), bits);
        }
        for &(a, b, equal, order) in FLOAT_COMPARISONS {
            compare(JavaF32::from_bits(a), JavaF32::from_bits(b), equal, order);
        }
    }

    #[test]
    fn native_double_value_semantics() {
        for &(bits, hash) in DOUBLE_HASHES {
            let value = JavaF64::from_bits(bits);
            assert_eq!(value.java_hash_code(), hash);
            assert_eq!(value.to_bits(), bits);
            assert_eq!(f64::from(value).to_bits(), bits);
        }
        for &(a, b, equal, order) in DOUBLE_COMPARISONS {
            compare(JavaF64::from_bits(a), JavaF64::from_bits(b), equal, order);
        }
    }
}
