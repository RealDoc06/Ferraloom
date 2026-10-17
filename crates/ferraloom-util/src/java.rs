//! Java primitive semantics used by the ports. Rust casts already implement
//! Java's saturating floating-point to integer conversion (including NaN -> 0).

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
