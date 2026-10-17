//! Port of `net.minecraft.util.Mth`. Unsuffixed floating-point helpers use
//! doubles; `_f32` helpers preserve the float overload's intermediate rounding.
#![allow(clippy::too_many_arguments)]
use crate::{java, random::RandomSource};
use std::sync::LazyLock;

pub const PI: f32 = std::f32::consts::PI;
pub const HALF_PI: f32 = std::f32::consts::FRAC_PI_2;
pub const TWO_PI: f32 = std::f32::consts::TAU;
pub const DEG_TO_RAD: f32 = 0.017453292;
pub const RAD_TO_DEG: f32 = 57.295776;
pub const EPSILON: f32 = 1.0e-5;
const SIN_SCALE: f64 = 10430.378350470453;
static SIN: LazyLock<Box<[f32; 65536]>> = LazyLock::new(|| {
    let mut table = Box::new([0.0; 65536]);
    for (i, v) in table.iter_mut().enumerate() {
        *v = (i as f64 / SIN_SCALE).sin() as f32;
    }
    table
});
static ATAN: LazyLock<[(f64, f64); 257]> = LazyLock::new(|| {
    std::array::from_fn(|i| {
        let asin = (i as f64 / 256.0).asin();
        (asin, asin.cos())
    })
});

#[inline]
pub fn sin(value: f64) -> f32 {
    SIN[((value * SIN_SCALE) as i64 & 65535) as usize]
}
#[inline]
pub fn cos(value: f64) -> f32 {
    SIN[((value * SIN_SCALE + 16384.0) as i64 & 65535) as usize]
}
#[inline]
pub fn sqrt(value: f32) -> f32 {
    f64::from(value).sqrt() as f32
}
#[inline]
pub fn floor(value: f64) -> i32 {
    value.floor() as i32
}
#[inline]
pub fn lfloor(value: f64) -> i64 {
    value.floor() as i64
}
#[inline]
pub fn ceil(value: f64) -> i32 {
    value.ceil() as i32
}
#[inline]
pub fn ceil_long(value: f64) -> i64 {
    value.ceil() as i64
}
#[inline]
pub fn clamp_int(value: i32, min: i32, max: i32) -> i32 {
    value.max(min).min(max)
}
#[inline]
pub fn clamp_long(value: i64, min: i64, max: i64) -> i64 {
    value.max(min).min(max)
}

macro_rules! float_math {
    ($ty:ty, $clamp:ident, $lerp:ident, $lerp2:ident, $lerp3:ident, $clamped:ident, $inverse:ident, $map:ident, $cmap:ident, $smooth:ident, $deriv:ident, $modulo:ident, $wrap:ident, $rot:ident, $frac:ident, $min:path, $floor:path) => {
        #[inline]
        pub fn $clamp(value: $ty, min: $ty, max: $ty) -> $ty {
            if value < min { min } else { $min(value, max) }
        }
        #[inline]
        pub fn $lerp(t: $ty, a: $ty, b: $ty) -> $ty {
            a + t * (b - a)
        }
        #[inline]
        pub fn $lerp2(tx: $ty, ty: $ty, a: $ty, b: $ty, c: $ty, d: $ty) -> $ty {
            $lerp(ty, $lerp(tx, a, b), $lerp(tx, c, d))
        }
        #[inline]
        pub fn $lerp3(
            tx: $ty,
            ty: $ty,
            tz: $ty,
            a: $ty,
            b: $ty,
            c: $ty,
            d: $ty,
            e: $ty,
            f: $ty,
            g: $ty,
            h: $ty,
        ) -> $ty {
            $lerp(tz, $lerp2(tx, ty, a, b, c, d), $lerp2(tx, ty, e, f, g, h))
        }
        #[inline]
        pub fn $clamped(t: $ty, a: $ty, b: $ty) -> $ty {
            if t < 0.0 {
                a
            } else if t > 1.0 {
                b
            } else {
                $lerp(t, a, b)
            }
        }
        #[inline]
        pub fn $inverse(v: $ty, a: $ty, b: $ty) -> $ty {
            (v - a) / (b - a)
        }
        #[inline]
        pub fn $map(v: $ty, a: $ty, b: $ty, c: $ty, d: $ty) -> $ty {
            $lerp($inverse(v, a, b), c, d)
        }
        #[inline]
        pub fn $cmap(v: $ty, a: $ty, b: $ty, c: $ty, d: $ty) -> $ty {
            $clamped($inverse(v, a, b), c, d)
        }
        #[inline]
        pub fn $smooth(x: $ty) -> $ty {
            x * x * x * (x * (x * 6.0 - 15.0) + 10.0)
        }
        #[inline]
        pub fn $deriv(x: $ty) -> $ty {
            30.0 * x * x * (x - 1.0) * (x - 1.0)
        }
        #[inline]
        pub fn $modulo(v: $ty, m: $ty) -> $ty {
            (v % m + m) % m
        }
        #[inline]
        pub fn $wrap(v: $ty) -> $ty {
            let mut a = v % 360.0;
            if a >= 180.0 {
                a -= 360.0;
            }
            if a < -180.0 {
                a += 360.0;
            }
            a
        }
        #[inline]
        pub fn $rot(t: $ty, from: $ty, to: $ty) -> $ty {
            from + t * $wrap(to - from)
        }
        #[inline]
        pub fn $frac(v: $ty) -> $ty {
            v - $floor(v as f64) as $ty
        }
    };
}
float_math!(
    f64,
    clamp,
    lerp,
    lerp2,
    lerp3,
    clamped_lerp,
    inverse_lerp,
    map,
    clamped_map,
    smoothstep,
    smoothstep_derivative,
    positive_modulo,
    wrap_degrees,
    rot_lerp,
    frac,
    java::min,
    lfloor
);
float_math!(
    f32,
    clamp_f32,
    lerp_f32,
    lerp2_f32,
    lerp3_f32,
    clamped_lerp_f32,
    inverse_lerp_f32,
    map_f32,
    clamped_map_f32,
    smoothstep_f32,
    smoothstep_derivative_f32,
    positive_modulo_f32,
    wrap_degrees_f32,
    rot_lerp_f32,
    frac_f32,
    java::min_f32,
    floor
);

#[inline]
pub fn positive_modulo_int(value: i32, modulo: i32) -> i32 {
    java::floor_mod(value, modulo)
}
#[inline]
pub fn wrap_degrees_int(value: i32) -> i32 {
    let mut a = value % 360;
    if a >= 180 {
        a -= 360;
    }
    if a < -180 {
        a += 360;
    }
    a
}
#[inline]
pub fn wrap_degrees_long(value: i64) -> f32 {
    wrap_degrees_int((value % 360) as i32) as f32
}
#[inline]
pub fn wrap_degrees_90(value: f32) -> f32 {
    let mut a = value % 90.0;
    if a >= 45.0 {
        a -= 90.0;
    }
    if a < -45.0 {
        a += 90.0;
    }
    a
}
#[inline]
pub fn degrees_difference(from: f32, to: f32) -> f32 {
    wrap_degrees_f32(to - from)
}
#[inline]
pub fn degrees_difference_abs(a: f32, b: f32) -> f32 {
    degrees_difference(a, b).abs()
}
#[inline]
pub fn rotate_if_necessary(base: f32, target: f32, max_diff: f32) -> f32 {
    target - clamp_f32(degrees_difference(base, target), -max_diff, max_diff)
}
#[inline]
pub fn approach(current: f32, target: f32, increment: f32) -> f32 {
    let increment = increment.abs();
    if current < target {
        clamp_f32(current + increment, current, target)
    } else {
        clamp_f32(current - increment, target, current)
    }
}
#[inline]
pub fn approach_degrees(current: f32, target: f32, increment: f32) -> f32 {
    approach(
        current,
        current + degrees_difference(current, target),
        increment,
    )
}
#[inline]
pub fn pack_degrees(angle: f32) -> i8 {
    floor(f64::from(angle * 256.0 / 360.0)) as i8
}
#[inline]
pub fn unpack_degrees(rotation: i8) -> f32 {
    i32::from(rotation).wrapping_mul(360) as f32 / 256.0
}
#[inline]
pub fn equal(a: f64, b: f64) -> bool {
    (b - a).abs() < f64::from(EPSILON)
}
#[inline]
pub fn equal_f32(a: f32, b: f32) -> bool {
    (b - a).abs() < EPSILON
}
#[inline]
pub fn get_seed(x: i32, y: i32, z: i32) -> i64 {
    let seed =
        i64::from(x.wrapping_mul(3129871)) ^ i64::from(z).wrapping_mul(116129781) ^ i64::from(y);
    seed.wrapping_mul(seed)
        .wrapping_mul(42317861)
        .wrapping_add(seed.wrapping_mul(11))
        >> 16
}
#[inline]
pub fn murmur_hash3_mixer(hash: i32) -> i32 {
    let mut h = hash as u32;
    h = (h ^ (h >> 16)).wrapping_mul(0x85ebca6b);
    h = (h ^ (h >> 13)).wrapping_mul(0xc2b2ae35);
    (h ^ (h >> 16)) as i32
}
#[inline]
pub fn fast_inv_sqrt(mut x: f64) -> f64 {
    let half = 0.5 * x;
    let bits = 6910469410427058090_i64.wrapping_sub((x.to_bits() as i64) >> 1);
    x = f64::from_bits(bits as u64);
    x * (1.5 - half * x * x)
}
#[inline]
pub fn inv_sqrt(x: f64) -> f64 {
    1.0 / x.sqrt()
}
#[inline]
pub fn inv_sqrt_f32(x: f32) -> f32 {
    1.0 / sqrt(x)
}
#[inline]
pub fn fast_inv_cube_root(x: f32) -> f32 {
    let bits = if x.is_nan() { 0x7fc00000 } else { x.to_bits() } as i32;
    let mut y = f32::from_bits(1419967116_i32.wrapping_sub(bits / 3) as u32);
    y = 0.6666667 * y + 1.0 / (3.0 * y * y * x);
    0.6666667 * y + 1.0 / (3.0 * y * y * x)
}
/// Port of Mth#atan2, including vanilla's lookup-table approximation.
pub fn atan2(mut y: f64, mut x: f64) -> f64 {
    let d2 = x * x + y * y;
    if d2.is_nan() {
        return f64::NAN;
    }
    let neg_y = y < 0.0;
    if neg_y {
        y = -y;
    }
    let neg_x = x < 0.0;
    if neg_x {
        x = -x;
    }
    let steep = y > x;
    if steep {
        std::mem::swap(&mut x, &mut y);
    }
    let inv = fast_inv_sqrt(d2);
    x *= inv;
    y *= inv;
    let bias = f64::from_bits(4805340802404319232);
    let yp = bias + y;
    let index = yp.to_bits() as u32 as usize;
    let (phi, cphi) = ATAN[index];
    let sd = y * cphi - x * (yp - bias);
    let mut theta = phi + (6.0 + sd * sd) * sd * (1.0 / 6.0);
    if steep {
        theta = std::f64::consts::FRAC_PI_2 - theta;
    }
    if neg_x {
        theta = std::f64::consts::PI - theta;
    }
    if neg_y {
        theta = -theta;
    }
    theta
}
#[inline]
pub fn smallest_encompassing_power_of_two(input: i32) -> i32 {
    let mut n = input.wrapping_sub(1);
    n |= n >> 1;
    n |= n >> 2;
    n |= n >> 4;
    n |= n >> 8;
    n |= n >> 16;
    n.wrapping_add(1)
}
#[inline]
pub fn is_power_of_two(n: i32) -> bool {
    n != 0 && (n & n.wrapping_sub(1)) == 0
}
#[inline]
pub fn ceil_log2(n: i32) -> i32 {
    const TABLE: [i32; 32] = [
        0, 1, 28, 2, 29, 14, 24, 3, 30, 22, 20, 15, 25, 17, 4, 8, 31, 27, 13, 23, 21, 19, 16, 7,
        26, 12, 18, 6, 11, 5, 10, 9,
    ];
    let n = if is_power_of_two(n) {
        n
    } else {
        smallest_encompassing_power_of_two(n)
    };
    TABLE[(((i64::from(n) * 125613361) >> 27) & 31) as usize]
}
#[inline]
pub fn log2(n: i32) -> i32 {
    ceil_log2(n) - i32::from(!is_power_of_two(n))
}
#[inline]
pub fn lerp_int(t: f32, a: i32, b: i32) -> i32 {
    a.wrapping_add(floor(f64::from(t * b.wrapping_sub(a) as f32)))
}
#[inline]
pub fn lerp_discrete(t: f32, a: i32, b: i32) -> i32 {
    a.wrapping_add(floor(f64::from(
        t * b.wrapping_sub(a).wrapping_sub(1) as f32,
    )))
    .wrapping_add(i32::from(t > 0.0))
}
#[inline]
pub fn catmullrom(t: f32, a: f32, b: f32, c: f32, d: f32) -> f32 {
    0.5 * (2.0 * b
        + (c - a) * t
        + (2.0 * a - 5.0 * b + 4.0 * c - d) * t * t
        + (3.0 * b - a - 3.0 * c + d) * t * t * t)
}
#[inline]
pub fn sign(n: f64) -> i32 {
    if n == 0.0 {
        0
    } else if n > 0.0 {
        1
    } else {
        -1
    }
}
#[inline]
pub fn triangle_wave(index: f32, period: f32) -> f32 {
    ((index % period - period * 0.5).abs() - period * 0.25) / (period * 0.25)
}
#[inline]
pub fn positive_ceil_div(n: i32, divisor: i32) -> i32 {
    java::floor_div(n.wrapping_neg(), divisor).wrapping_neg()
}
#[inline]
pub fn positive_ceil_div_long(n: i64, divisor: i64) -> i64 {
    java::floor_div_long(n.wrapping_neg(), divisor).wrapping_neg()
}
#[inline]
pub fn round_toward(n: i32, multiple: i32) -> i32 {
    positive_ceil_div(n, multiple).wrapping_mul(multiple)
}
#[inline]
pub fn quantize(value: f64, resolution: i32) -> i32 {
    floor(value / f64::from(resolution)).wrapping_mul(resolution)
}
#[inline]
pub fn wobble(coord: f64) -> f64 {
    let mut random =
        crate::random::SingleThreadedRandomSource::new(i64::from(floor(coord * 3000.0)));
    coord + (2.0 * random.next_double() - 1.0) * 1.0e-7 / 2.0
}
#[inline]
pub fn next_int(random: &mut impl RandomSource, min: i32, max: i32) -> i32 {
    if min >= max {
        min
    } else {
        random
            .next_int_bound(max.wrapping_sub(min).wrapping_add(1))
            .wrapping_add(min)
    }
}
#[inline]
pub fn next_float(random: &mut impl RandomSource, min: f32, max: f32) -> f32 {
    if min >= max {
        min
    } else {
        random.next_float() * (max - min) + min
    }
}
#[inline]
pub fn next_double(random: &mut impl RandomSource, min: f64, max: f64) -> f64 {
    if min >= max {
        min
    } else {
        random.next_double() * (max - min) + min
    }
}
#[inline]
pub fn normal(random: &mut impl RandomSource, mean: f32, deviation: f32) -> f32 {
    mean + random.next_gaussian() as f32 * deviation
}

#[inline]
pub fn abs_max_int(a: i32, b: i32) -> i32 {
    a.wrapping_abs().max(b.wrapping_abs())
}
#[inline]
pub fn abs_max(a: f64, b: f64) -> f64 {
    java::max(a.abs(), b.abs())
}
#[inline]
pub fn abs_max_f32(a: f32, b: f32) -> f32 {
    java::max_f32(a.abs(), b.abs())
}
#[inline]
pub fn chessboard_distance(x0: i32, z0: i32, x1: i32, z1: i32) -> i32 {
    abs_max_int(x1.wrapping_sub(x0), z1.wrapping_sub(z0))
}
#[inline]
pub fn square(x: f64) -> f64 {
    x * x
}
#[inline]
pub fn square_f32(x: f32) -> f32 {
    x * x
}
#[inline]
pub fn square_int(x: i32) -> i32 {
    x.wrapping_mul(x)
}
#[inline]
pub fn square_long(x: i64) -> i64 {
    x.wrapping_mul(x)
}
#[inline]
pub fn cube(x: f32) -> f32 {
    x * x * x
}
#[inline]
pub fn length_squared_2d(x: f64, y: f64) -> f64 {
    x * x + y * y
}
#[inline]
pub fn length_2d(x: f64, y: f64) -> f64 {
    length_squared_2d(x, y).sqrt()
}
#[inline]
pub fn length_squared(x: f64, y: f64, z: f64) -> f64 {
    x * x + y * y + z * z
}
#[inline]
pub fn length(x: f64, y: f64, z: f64) -> f64 {
    length_squared(x, y, z).sqrt()
}
#[inline]
pub fn length_squared_2d_f32(x: f32, y: f32) -> f32 {
    x * x + y * y
}
#[inline]
pub fn length_2d_f32(x: f32, y: f32) -> f32 {
    sqrt(length_squared_2d_f32(x, y))
}
#[inline]
pub fn length_squared_f32(x: f32, y: f32, z: f32) -> f32 {
    x * x + y * y + z * z
}
#[inline]
pub fn length_f32(x: f32, y: f32, z: f32) -> f32 {
    sqrt(length_squared_f32(x, y, z))
}
#[inline]
pub fn smallest_square_side(item_count: i32) -> i32 {
    assert!(item_count >= 0, "item count must be nonnegative");
    ceil(f64::from(item_count).sqrt())
}
#[inline]
pub fn is_power_of_two_long(n: i64) -> bool {
    n != 0 && (n & n.wrapping_sub(1)) == 0
}
#[inline]
pub fn round_toward_long(n: i64, multiple: i64) -> i64 {
    positive_ceil_div_long(n, multiple).wrapping_mul(multiple)
}
#[inline]
pub fn is_multiple_of(dividend: i32, divisor: i32) -> bool {
    dividend.wrapping_rem(divisor) == 0
}
pub fn get_int(input: &str, default: i32) -> i32 {
    input.parse().unwrap_or(default)
}
/// Port of Mth#rotLerpRad, preserving the float loop's rounding.
pub fn rot_lerp_rad(t: f32, from: f32, to: f32) -> f32 {
    let mut diff = to - from;
    while diff < -PI {
        diff += TWO_PI;
    }
    while diff >= PI {
        diff -= TWO_PI;
    }
    from + t * diff
}
pub fn binary_search(mut from: i32, to: i32, condition: impl Fn(i32) -> bool) -> i32 {
    let mut len = to.wrapping_sub(from);
    while len > 0 {
        let half = len / 2;
        let middle = from.wrapping_add(half);
        if condition(middle) {
            len = half;
        } else {
            from = middle.wrapping_add(1);
            len = len.wrapping_sub(half.wrapping_add(1));
        }
    }
    from
}
/// Port of Mth#outFromOrigin. Alternates positive then negative distances,
/// continuing on the remaining side when a boundary is reached.
pub fn out_from_origin(
    origin: i32,
    lower: i32,
    upper: i32,
    step: i32,
) -> impl Iterator<Item = i32> {
    assert!(lower <= upper, "upper bound is below lower bound");
    assert!(step >= 1, "step must be positive");
    let origin = clamp_int(origin, lower, upper);
    let mut cursor = origin;
    let mut finished = false;
    std::iter::from_fn(move || {
        if finished {
            return None;
        }
        let distance = origin.wrapping_sub(cursor).wrapping_abs();
        if origin.wrapping_sub(distance) < lower && origin.wrapping_add(distance) > upper {
            finished = true;
            return None;
        }
        let result = cursor;
        let negative = cursor <= origin;
        let can_positive = origin.wrapping_add(distance).wrapping_add(step) <= upper;
        let attempted = origin
            .wrapping_sub(distance)
            .wrapping_sub(if negative { step } else { 0 });
        cursor = if (!negative || !can_positive) && attempted >= lower {
            attempted
        } else {
            origin.wrapping_add(distance).wrapping_add(step)
        };
        Some(result)
    })
}
pub fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> i32 {
    hsv_to_argb(hue, saturation, value, 0)
}
pub fn hsv_to_argb(hue: f32, saturation: f32, value: f32, alpha: i32) -> i32 {
    let h = (hue * 6.0) as i32 % 6;
    let f = hue * 6.0 - h as f32;
    let p = value * (1.0 - saturation);
    let q = value * (1.0 - f * saturation);
    let t = value * (1.0 - (1.0 - f) * saturation);
    let [r, g, b] = match h {
        0 => [value, t, p],
        1 => [q, value, p],
        2 => [p, value, t],
        3 => [p, q, value],
        4 => [t, p, value],
        5 => [value, p, q],
        _ => panic!("invalid HSV hue"),
    };
    (alpha << 24)
        | (clamp_int((r * 255.0) as i32, 0, 255) << 16)
        | (clamp_int((g * 255.0) as i32, 0, 255) << 8)
        | clamp_int((b * 255.0) as i32, 0, 255)
}
/// Port of Mth#rayIntersectsAABB (unbounded forward ray).
pub fn ray_intersects_aabb(start: crate::Vec3, direction: crate::Vec3, aabb: crate::Aabb) -> bool {
    let center = crate::Vec3::new(
        (aabb.min_x + aabb.max_x) * 0.5,
        (aabb.min_y + aabb.max_y) * 0.5,
        (aabb.min_z + aabb.max_z) * 0.5,
    );
    let extent = crate::Vec3::new(
        (aabb.max_x - aabb.min_x) * 0.5,
        (aabb.max_y - aabb.min_y) * 0.5,
        (aabb.max_z - aabb.min_z) * 0.5,
    );
    let diff = start - center;
    if diff.x.abs() > extent.x && diff.x * direction.x >= 0.0 {
        return false;
    }
    if diff.y.abs() > extent.y && diff.y * direction.y >= 0.0 {
        return false;
    }
    if diff.z.abs() > extent.z && diff.z * direction.z >= 0.0 {
        return false;
    }
    let dx = direction.x.abs();
    let dy = direction.y.abs();
    let dz = direction.z.abs();
    if (direction.y * diff.z - direction.z * diff.y).abs() > extent.y * dz + extent.z * dy {
        return false;
    }
    if (direction.z * diff.x - direction.x * diff.z).abs() > extent.x * dz + extent.z * dx {
        return false;
    }
    (direction.x * diff.y - direction.y * diff.x).abs() < extent.x * dy + extent.y * dx
}
