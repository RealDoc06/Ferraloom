//! Ports of `net.minecraft.world.phys.Vec3` and `AABB`.
use crate::{
    core::{Axis, BlockPos, Direction, Vec3i},
    java, mth,
    random::RandomSource,
};
use std::ops::{Add, Mul, Neg, Sub};

/// Port of BlockHitResult. Geometry retains double precision independently of
/// the packet codec's relative float offsets; misses are explicit results.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockHitResult {
    pub location: Vec3,
    pub direction: Direction,
    pub block_pos: BlockPos,
    pub miss: bool,
    pub inside: bool,
    pub world_border_hit: bool,
}
impl BlockHitResult {
    pub const fn new(
        location: Vec3,
        direction: Direction,
        block_pos: BlockPos,
        inside: bool,
    ) -> Self {
        Self {
            location,
            direction,
            block_pos,
            miss: false,
            inside,
            world_border_hit: false,
        }
    }
    /// Port of BlockHitResult#miss.
    pub const fn miss(location: Vec3, direction: Direction, block_pos: BlockPos) -> Self {
        Self {
            miss: true,
            ..Self::new(location, direction, block_pos, false)
        }
    }
    /// Port of BlockHitResult#withPosition. The hit location stays unchanged.
    pub const fn with_position(self, block_pos: BlockPos) -> Self {
        Self { block_pos, ..self }
    }
    pub const fn with_direction(self, direction: Direction) -> Self {
        Self { direction, ..self }
    }
    pub const fn hit_border(self) -> Self {
        Self {
            world_border_hit: true,
            ..self
        }
    }
}
impl From<crate::shapes::ShapeHit> for BlockHitResult {
    fn from(hit: crate::shapes::ShapeHit) -> Self {
        Self::new(hit.location, hit.direction, hit.block_pos, hit.inside)
    }
}

/// Port of Vec3. Equality follows Double.compare, including NaN and signed zero.
#[derive(Clone, Copy, Debug, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const X_AXIS: Self = Self::new(1.0, 0.0, 0.0);
    pub const Y_AXIS: Self = Self::new(0.0, 1.0, 0.0);
    pub const Z_AXIS: Self = Self::new(0.0, 0.0, 1.0);
    #[inline]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
    #[inline]
    pub fn at_lower_corner_of(pos: Vec3i) -> Self {
        Self::new(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z))
    }
    #[inline]
    pub fn at_lower_corner_with_offset(pos: Vec3i, x: f64, y: f64, z: f64) -> Self {
        Self::at_lower_corner_of(pos).add_xyz(x, y, z)
    }
    #[inline]
    pub fn at_center_of(pos: Vec3i) -> Self {
        Self::at_lower_corner_with_offset(pos, 0.5, 0.5, 0.5)
    }
    #[inline]
    pub fn at_bottom_center_of(pos: Vec3i) -> Self {
        Self::at_lower_corner_with_offset(pos, 0.5, 0.0, 0.5)
    }
    #[inline]
    pub fn up_from_bottom_center_of(pos: Vec3i, offset: f64) -> Self {
        Self::at_lower_corner_with_offset(pos, 0.5, offset, 0.5)
    }
    #[inline]
    pub fn vector_to(self, other: Self) -> Self {
        other - self
    }
    #[inline]
    pub fn add_xyz(self, x: f64, y: f64, z: f64) -> Self {
        Self::new(self.x + x, self.y + y, self.z + z)
    }
    #[inline]
    pub fn subtract_xyz(self, x: f64, y: f64, z: f64) -> Self {
        self.add_xyz(-x, -y, -z)
    }
    #[inline]
    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }
    #[inline]
    pub fn cross(self, other: Self) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }
    #[inline]
    pub fn length_sqr(self) -> f64 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }
    #[inline]
    pub fn length(self) -> f64 {
        self.length_sqr().sqrt()
    }
    #[inline]
    pub fn normalize(self) -> Self {
        let length = self.length();
        if length < f64::from(mth::EPSILON) {
            Self::ZERO
        } else {
            Self::new(self.x / length, self.y / length, self.z / length)
        }
    }
    #[inline]
    pub fn distance_to_sqr(self, other: Self) -> f64 {
        (other - self).length_sqr()
    }
    #[inline]
    pub fn distance_to(self, other: Self) -> f64 {
        self.distance_to_sqr(other).sqrt()
    }
    #[inline]
    pub fn closer_than(self, other: Self, distance: f64) -> bool {
        self.distance_to_sqr(other) < distance * distance
    }
    #[inline]
    pub fn closer_than_xz_y(self, other: Self, xz: f64, y: f64) -> bool {
        let d = other - self;
        d.horizontal_distance_sqr() < xz * xz && d.y.abs() < y
    }
    #[inline]
    pub fn scale(self, scale: f64) -> Self {
        self.multiply(scale, scale, scale)
    }
    #[inline]
    pub fn reverse(self) -> Self {
        self.scale(-1.0)
    }
    #[inline]
    pub fn multiply(self, x: f64, y: f64, z: f64) -> Self {
        Self::new(self.x * x, self.y * y, self.z * z)
    }
    #[inline]
    pub fn horizontal(self) -> Self {
        Self::new(self.x, 0.0, self.z)
    }
    #[inline]
    pub fn horizontal_distance_sqr(self) -> f64 {
        self.x * self.x + self.z * self.z
    }
    #[inline]
    pub fn horizontal_distance(self) -> f64 {
        self.horizontal_distance_sqr().sqrt()
    }
    #[inline]
    pub fn lerp(self, other: Self, t: f64) -> Self {
        Self::new(
            mth::lerp(t, self.x, other.x),
            mth::lerp(t, self.y, other.y),
            mth::lerp(t, self.z, other.z),
        )
    }
    #[inline]
    pub fn x_rot(self, radians: f32) -> Self {
        let cos = f64::from(mth::cos(f64::from(radians)));
        let sin = f64::from(mth::sin(f64::from(radians)));
        Self::new(
            self.x,
            self.y * cos + self.z * sin,
            self.z * cos - self.y * sin,
        )
    }
    #[inline]
    pub fn y_rot(self, radians: f32) -> Self {
        let cos = f64::from(mth::cos(f64::from(radians)));
        let sin = f64::from(mth::sin(f64::from(radians)));
        Self::new(
            self.x * cos + self.z * sin,
            self.y,
            self.z * cos - self.x * sin,
        )
    }
    #[inline]
    pub fn z_rot(self, radians: f32) -> Self {
        let cos = f64::from(mth::cos(f64::from(radians)));
        let sin = f64::from(mth::sin(f64::from(radians)));
        Self::new(
            self.x * cos + self.y * sin,
            self.y * cos - self.x * sin,
            self.z,
        )
    }
    #[inline]
    pub fn rotate_clockwise_90(self) -> Self {
        Self::new(-self.z, self.y, self.x)
    }
    pub fn direction_from_rotation(pitch: f32, yaw: f32) -> Self {
        let yc = mth::cos(f64::from(-yaw * mth::DEG_TO_RAD - mth::PI));
        let ys = mth::sin(f64::from(-yaw * mth::DEG_TO_RAD - mth::PI));
        let xc = -mth::cos(f64::from(-pitch * mth::DEG_TO_RAD));
        let xs = mth::sin(f64::from(-pitch * mth::DEG_TO_RAD));
        Self::new(f64::from(ys * xc), f64::from(xs), f64::from(yc * xc))
    }
    pub fn rotation(self) -> (f32, f32) {
        (
            (-self.y / self.length()).asin() as f32 * mth::RAD_TO_DEG,
            (-self.x).atan2(self.z) as f32 * mth::RAD_TO_DEG,
        )
    }
    #[inline]
    pub fn get(self, axis: Axis) -> f64 {
        axis.choose(self.x, self.y, self.z)
    }
    #[inline]
    pub fn with(self, axis: Axis, value: f64) -> Self {
        Self::new(
            if axis == Axis::X { value } else { self.x },
            if axis == Axis::Y { value } else { self.y },
            if axis == Axis::Z { value } else { self.z },
        )
    }
    pub fn align(self, axes: &[Axis]) -> Self {
        Self::new(
            if axes.contains(&Axis::X) {
                f64::from(mth::floor(self.x))
            } else {
                self.x
            },
            if axes.contains(&Axis::Y) {
                f64::from(mth::floor(self.y))
            } else {
                self.y
            },
            if axes.contains(&Axis::Z) {
                f64::from(mth::floor(self.z))
            } else {
                self.z
            },
        )
    }
    pub fn offset_random(self, random: &mut impl RandomSource, offset: f32) -> Self {
        self.add_xyz(
            f64::from((random.next_float() - 0.5) * offset),
            f64::from((random.next_float() - 0.5) * offset),
            f64::from((random.next_float() - 0.5) * offset),
        )
    }
    pub fn offset_random_xz(self, random: &mut impl RandomSource, offset: f32) -> Self {
        self.add_xyz(
            f64::from((random.next_float() - 0.5) * offset),
            0.0,
            f64::from((random.next_float() - 0.5) * offset),
        )
    }
    pub fn relative(self, direction: Direction, distance: f64) -> Self {
        let n = direction.step();
        self.add_xyz(
            distance * f64::from(n.x),
            distance * f64::from(n.y),
            distance * f64::from(n.z),
        )
    }
    pub fn projected_on(self, onto: Self) -> Self {
        if onto.length_sqr() == 0.0 {
            onto
        } else {
            onto.scale(self.dot(onto)).scale(1.0 / onto.length_sqr())
        }
    }
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
    pub fn java_hash_code(self) -> i32 {
        [self.x, self.y, self.z]
            .into_iter()
            .map(|v| {
                let n = java::double_to_long_bits(v);
                (n ^ (n >> 32)) as i32
            })
            .reduce(|a, b| a.wrapping_mul(31).wrapping_add(b))
            .unwrap()
    }
}
impl PartialEq for Vec3 {
    fn eq(&self, other: &Self) -> bool {
        java::double_equals(self.x, other.x)
            && java::double_equals(self.y, other.y)
            && java::double_equals(self.z, other.z)
    }
}
impl Eq for Vec3 {}
impl Add for Vec3 {
    type Output = Self;
    #[inline]
    fn add(self, other: Self) -> Self {
        self.add_xyz(other.x, other.y, other.z)
    }
}
impl Sub for Vec3 {
    type Output = Self;
    #[inline]
    fn sub(self, other: Self) -> Self {
        self.subtract_xyz(other.x, other.y, other.z)
    }
}
impl Mul<f64> for Vec3 {
    type Output = Self;
    #[inline]
    fn mul(self, scale: f64) -> Self {
        self.scale(scale)
    }
}
impl Neg for Vec3 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        self.reverse()
    }
}

/// Port of AABB. Construction normalizes each pair of bounds, even after
/// over-deflation or the intersection of disjoint boxes.
#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub min_x: f64,
    pub min_y: f64,
    pub min_z: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub max_z: f64,
}
impl Aabb {
    #[inline]
    pub fn new(min_x: f64, min_y: f64, min_z: f64, max_x: f64, max_y: f64, max_z: f64) -> Self {
        Self {
            min_x: java::min(min_x, max_x),
            min_y: java::min(min_y, max_y),
            min_z: java::min(min_z, max_z),
            max_x: java::max(min_x, max_x),
            max_y: java::max(min_y, max_y),
            max_z: java::max(min_z, max_z),
        }
    }
    pub fn from_corners(a: Vec3, b: Vec3) -> Self {
        Self::new(a.x, a.y, a.z, b.x, b.y, b.z)
    }
    pub fn from_block(pos: BlockPos) -> Self {
        Self::new(
            f64::from(pos.x),
            f64::from(pos.y),
            f64::from(pos.z),
            f64::from(pos.x.wrapping_add(1)),
            f64::from(pos.y.wrapping_add(1)),
            f64::from(pos.z.wrapping_add(1)),
        )
    }
    pub fn unit_cube_from_lower_corner(pos: Vec3) -> Self {
        Self::from_corners(pos, pos.add_xyz(1.0, 1.0, 1.0))
    }
    pub fn of_size(center: Vec3, x: f64, y: f64, z: f64) -> Self {
        Self::new(
            center.x - x / 2.0,
            center.y - y / 2.0,
            center.z - z / 2.0,
            center.x + x / 2.0,
            center.y + y / 2.0,
            center.z + z / 2.0,
        )
    }
    pub fn min(self, axis: Axis) -> f64 {
        axis.choose(self.min_x, self.min_y, self.min_z)
    }
    pub fn max(self, axis: Axis) -> f64 {
        axis.choose(self.max_x, self.max_y, self.max_z)
    }
    #[inline]
    pub fn inflate(self, x: f64, y: f64, z: f64) -> Self {
        Self::new(
            self.min_x - x,
            self.min_y - y,
            self.min_z - z,
            self.max_x + x,
            self.max_y + y,
            self.max_z + z,
        )
    }
    #[inline]
    pub fn inflate_uniform(self, amount: f64) -> Self {
        self.inflate(amount, amount, amount)
    }
    #[inline]
    pub fn deflate(self, x: f64, y: f64, z: f64) -> Self {
        self.inflate(-x, -y, -z)
    }
    #[inline]
    pub fn move_by(self, x: f64, y: f64, z: f64) -> Self {
        Self::new(
            self.min_x + x,
            self.min_y + y,
            self.min_z + z,
            self.max_x + x,
            self.max_y + y,
            self.max_z + z,
        )
    }
    pub fn move_vec(self, v: Vec3) -> Self {
        self.move_by(v.x, v.y, v.z)
    }
    pub fn expand_towards(self, x: f64, y: f64, z: f64) -> Self {
        Self::new(
            if x < 0.0 { self.min_x + x } else { self.min_x },
            if y < 0.0 { self.min_y + y } else { self.min_y },
            if z < 0.0 { self.min_z + z } else { self.min_z },
            if x > 0.0 { self.max_x + x } else { self.max_x },
            if y > 0.0 { self.max_y + y } else { self.max_y },
            if z > 0.0 { self.max_z + z } else { self.max_z },
        )
    }
    pub fn contract(self, x: f64, y: f64, z: f64) -> Self {
        Self::new(
            if x < 0.0 { self.min_x - x } else { self.min_x },
            if y < 0.0 { self.min_y - y } else { self.min_y },
            if z < 0.0 { self.min_z - z } else { self.min_z },
            if x > 0.0 { self.max_x - x } else { self.max_x },
            if y > 0.0 { self.max_y - y } else { self.max_y },
            if z > 0.0 { self.max_z - z } else { self.max_z },
        )
    }
    #[inline]
    pub fn intersect(self, other: Self) -> Self {
        Self::new(
            java::max(self.min_x, other.min_x),
            java::max(self.min_y, other.min_y),
            java::max(self.min_z, other.min_z),
            java::min(self.max_x, other.max_x),
            java::min(self.max_y, other.max_y),
            java::min(self.max_z, other.max_z),
        )
    }
    pub fn minmax(self, other: Self) -> Self {
        Self::new(
            java::min(self.min_x, other.min_x),
            java::min(self.min_y, other.min_y),
            java::min(self.min_z, other.min_z),
            java::max(self.max_x, other.max_x),
            java::max(self.max_y, other.max_y),
            java::max(self.max_z, other.max_z),
        )
    }
    #[inline]
    pub fn intersects(self, other: Self) -> bool {
        self.min_x < other.max_x
            && self.max_x > other.min_x
            && self.min_y < other.max_y
            && self.max_y > other.min_y
            && self.min_z < other.max_z
            && self.max_z > other.min_z
    }
    #[inline]
    pub fn contains(self, v: Vec3) -> bool {
        v.x >= self.min_x
            && v.x < self.max_x
            && v.y >= self.min_y
            && v.y < self.max_y
            && v.z >= self.min_z
            && v.z < self.max_z
    }
    pub fn x_size(self) -> f64 {
        self.max_x - self.min_x
    }
    pub fn y_size(self) -> f64 {
        self.max_y - self.min_y
    }
    pub fn z_size(self) -> f64 {
        self.max_z - self.min_z
    }
    pub fn size(self) -> f64 {
        (self.x_size() + self.y_size() + self.z_size()) / 3.0
    }
    pub fn min_position(self) -> Vec3 {
        Vec3::new(self.min_x, self.min_y, self.min_z)
    }
    pub fn max_position(self) -> Vec3 {
        Vec3::new(self.max_x, self.max_y, self.max_z)
    }
    pub fn center(self) -> Vec3 {
        Vec3::new(
            mth::lerp(0.5, self.min_x, self.max_x),
            mth::lerp(0.5, self.min_y, self.max_y),
            mth::lerp(0.5, self.min_z, self.max_z),
        )
    }
    pub fn bottom_center(self) -> Vec3 {
        self.center().with(Axis::Y, self.min_y)
    }
    pub fn has_nan(self) -> bool {
        [
            self.min_x, self.min_y, self.min_z, self.max_x, self.max_y, self.max_z,
        ]
        .into_iter()
        .any(f64::is_nan)
    }
    pub fn next_deflated(self) -> Self {
        Self::new(
            self.min_x.next_up(),
            self.min_y.next_up(),
            self.min_z.next_up(),
            self.max_x.next_down(),
            self.max_y.next_down(),
            self.max_z.next_down(),
        )
    }
    /// Port of AABB#clip. Only entering faces are considered. Start-inside rays,
    /// start-on-face rays and endpoint-only hits can therefore return None.
    pub fn clip(self, from: Vec3, to: Vec3) -> Option<Vec3> {
        self.clip_with_direction(from, to).map(|hit| hit.location)
    }
    pub fn clip_with_direction(self, from: Vec3, to: Vec3) -> Option<AabbHit> {
        let mut scale = 1.0;
        let delta = to - from;
        self.clip_direction(from, delta, &mut scale)
            .map(|direction| AabbHit {
                location: from.add_xyz(scale * delta.x, scale * delta.y, scale * delta.z),
                direction,
            })
    }
    fn clip_direction(self, from: Vec3, delta: Vec3, scale: &mut f64) -> Option<Direction> {
        let mut result = None;
        for (axis, b, c, negative, positive) in [
            (Axis::X, Axis::Y, Axis::Z, Direction::West, Direction::East),
            (Axis::Y, Axis::Z, Axis::X, Direction::Down, Direction::Up),
            (
                Axis::Z,
                Axis::X,
                Axis::Y,
                Direction::North,
                Direction::South,
            ),
        ] {
            let da = delta.get(axis);
            let (point, direction) = if da > 1.0e-7 {
                (self.min(axis), negative)
            } else if da < -1.0e-7 {
                (self.max(axis), positive)
            } else {
                continue;
            };
            let s = (point - from.get(axis)) / da;
            let pb = from.get(b) + s * delta.get(b);
            let pc = from.get(c) + s * delta.get(c);
            if 0.0 < s
                && s < *scale
                && self.min(b) - 1.0e-7 < pb
                && pb < self.max(b) + 1.0e-7
                && self.min(c) - 1.0e-7 < pc
                && pc < self.max(c) + 1.0e-7
            {
                *scale = s;
                result = Some(direction);
            }
        }
        result
    }
    /// Port of AABB#clip(Iterable,Vec3,Vec3,BlockPos), without BlockHitResult's
    /// game-layer payload. Equal-distance ties keep the first shape/face.
    pub fn clip_all(
        boxes: impl IntoIterator<Item = Self>,
        from: Vec3,
        to: Vec3,
        pos: BlockPos,
    ) -> Option<AabbHit> {
        let mut scale = 1.0;
        let delta = to - from;
        let mut direction = None;
        for aabb in boxes {
            if let Some(d) = aabb
                .move_by(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z))
                .clip_direction(from, delta, &mut scale)
            {
                direction = Some(d);
            }
        }
        direction.map(|direction| AabbHit {
            location: from.add_xyz(scale * delta.x, scale * delta.y, scale * delta.z),
            direction,
        })
    }
    pub fn collided_along_vector(self, vector: Vec3, boxes: &[Self]) -> bool {
        let from = self.center();
        let to = from + vector;
        boxes.iter().any(|part| {
            let inflated = part.inflate(
                self.x_size() * 0.5 - 1.0e-7,
                self.y_size() * 0.5 - 1.0e-7,
                self.z_size() * 0.5 - 1.0e-7,
            );
            inflated.contains(to) || inflated.contains(from) || inflated.clip(from, to).is_some()
        })
    }
    pub fn distance_to_sqr(self, point: Vec3) -> f64 {
        let x = java::max(java::max(self.min_x - point.x, point.x - self.max_x), 0.0);
        let y = java::max(java::max(self.min_y - point.y, point.y - self.max_y), 0.0);
        let z = java::max(java::max(self.min_z - point.z, point.z - self.max_z), 0.0);
        x * x + y * y + z * z
    }
    pub fn distance_to_box_sqr(self, other: Self) -> f64 {
        let x = java::max(
            java::max(self.min_x - other.max_x, other.min_x - self.max_x),
            0.0,
        );
        let y = java::max(
            java::max(self.min_y - other.max_y, other.min_y - self.max_y),
            0.0,
        );
        let z = java::max(
            java::max(self.min_z - other.max_z, other.min_z - self.max_z),
            0.0,
        );
        x * x + y * y + z * z
    }
}
impl PartialEq for Aabb {
    fn eq(&self, other: &Self) -> bool {
        self.min_position() == other.min_position() && self.max_position() == other.max_position()
    }
}
impl Eq for Aabb {}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AabbHit {
    pub location: Vec3,
    pub direction: Direction,
}
