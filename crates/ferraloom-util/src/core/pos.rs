use super::{Axis, Direction, Rotation};
use crate::{mth, phys::Vec3};

macro_rules! integer_position {
    ($name:ident, $source:literal) => {
        #[doc=$source]
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $name {
            pub x: i32,
            pub y: i32,
            pub z: i32,
        }
        impl $name {
            pub const ZERO: Self = Self::new(0, 0, 0);
            #[inline]
            pub const fn new(x: i32, y: i32, z: i32) -> Self {
                Self { x, y, z }
            }
            #[inline]
            pub fn as_vec3i(self) -> Vec3i {
                Vec3i::new(self.x, self.y, self.z)
            }
            #[inline]
            pub fn offset(self, x: i32, y: i32, z: i32) -> Self {
                Self::new(
                    self.x.wrapping_add(x),
                    self.y.wrapping_add(y),
                    self.z.wrapping_add(z),
                )
            }
            #[inline]
            pub fn offset_vec(self, pos: Vec3i) -> Self {
                self.offset(pos.x, pos.y, pos.z)
            }
            #[inline]
            pub fn subtract(self, pos: Vec3i) -> Self {
                self.offset(
                    pos.x.wrapping_neg(),
                    pos.y.wrapping_neg(),
                    pos.z.wrapping_neg(),
                )
            }
            #[inline]
            pub fn multiply(self, scale: i32) -> Self {
                Self::new(
                    self.x.wrapping_mul(scale),
                    self.y.wrapping_mul(scale),
                    self.z.wrapping_mul(scale),
                )
            }
            #[inline]
            pub fn relative(self, direction: Direction, steps: i32) -> Self {
                self.offset_vec(direction.step().multiply(steps))
            }
            #[inline]
            pub fn relative_axis(self, axis: Axis, steps: i32) -> Self {
                self.offset(
                    axis.choose(steps, 0, 0),
                    axis.choose(0, steps, 0),
                    axis.choose(0, 0, steps),
                )
            }
            #[inline]
            pub fn above(self, steps: i32) -> Self {
                self.relative(Direction::Up, steps)
            }
            #[inline]
            pub fn below(self, steps: i32) -> Self {
                self.relative(Direction::Down, steps)
            }
            #[inline]
            pub fn north(self, steps: i32) -> Self {
                self.relative(Direction::North, steps)
            }
            #[inline]
            pub fn south(self, steps: i32) -> Self {
                self.relative(Direction::South, steps)
            }
            #[inline]
            pub fn west(self, steps: i32) -> Self {
                self.relative(Direction::West, steps)
            }
            #[inline]
            pub fn east(self, steps: i32) -> Self {
                self.relative(Direction::East, steps)
            }
            #[inline]
            pub fn get(self, axis: Axis) -> i32 {
                axis.choose(self.x, self.y, self.z)
            }
            #[inline]
            pub fn cross(self, v: Vec3i) -> Self {
                Self::new(
                    self.y
                        .wrapping_mul(v.z)
                        .wrapping_sub(self.z.wrapping_mul(v.y)),
                    self.z
                        .wrapping_mul(v.x)
                        .wrapping_sub(self.x.wrapping_mul(v.z)),
                    self.x
                        .wrapping_mul(v.y)
                        .wrapping_sub(self.y.wrapping_mul(v.x)),
                )
            }
            #[inline]
            pub fn java_hash_code(self) -> i32 {
                self.y
                    .wrapping_add(self.z.wrapping_mul(31))
                    .wrapping_mul(31)
                    .wrapping_add(self.x)
            }
            /// Java's subtraction comparator can overflow, so this is not Rust Ord.
            #[inline]
            pub fn compare_to(self, pos: Self) -> i32 {
                if self.y == pos.y {
                    if self.z == pos.z {
                        self.x.wrapping_sub(pos.x)
                    } else {
                        self.z.wrapping_sub(pos.z)
                    }
                } else {
                    self.y.wrapping_sub(pos.y)
                }
            }
            #[inline]
            pub fn dist_to_low_corner_sqr(self, x: f64, y: f64, z: f64) -> f64 {
                let dx = f64::from(self.x) - x;
                let dy = f64::from(self.y) - y;
                let dz = f64::from(self.z) - z;
                dx * dx + dy * dy + dz * dz
            }
            #[inline]
            pub fn dist_sqr(self, pos: Vec3i) -> f64 {
                self.dist_to_low_corner_sqr(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z))
            }
            #[inline]
            pub fn dist_to_center_sqr(self, pos: Vec3) -> f64 {
                let dx = f64::from(self.x) + 0.5 - pos.x;
                let dy = f64::from(self.y) + 0.5 - pos.y;
                let dz = f64::from(self.z) + 0.5 - pos.z;
                dx * dx + dy * dy + dz * dz
            }
            #[inline]
            pub fn closer_than(self, pos: Vec3i, distance: f64) -> bool {
                self.dist_sqr(pos) < distance * distance
            }
            #[inline]
            pub fn dist_manhattan(self, pos: Vec3i) -> i32 {
                (pos.x.wrapping_sub(self.x).wrapping_abs() as f32
                    + pos.y.wrapping_sub(self.y).wrapping_abs() as f32
                    + pos.z.wrapping_sub(self.z).wrapping_abs() as f32) as i32
            }
            #[inline]
            pub fn dist_chessboard(self, pos: Vec3i) -> i32 {
                self.x
                    .wrapping_sub(pos.x)
                    .wrapping_abs()
                    .max(self.y.wrapping_sub(pos.y).wrapping_abs())
                    .max(self.z.wrapping_sub(pos.z).wrapping_abs())
            }
        }
    };
}
integer_position!(Vec3i, "Port of net.minecraft.core.Vec3i.");
integer_position!(
    BlockPos,
    "Port of net.minecraft.core.BlockPos. Packing is X:26, Z:26, Y:12."
);
integer_position!(
    SectionPos,
    "Port of net.minecraft.core.SectionPos. Packing is X:22, Z:22, Y:20."
);
impl Vec3i {
    pub fn multiply_axes(self, x: i32, y: i32, z: i32) -> Self {
        Self::new(
            self.x.wrapping_mul(x),
            self.y.wrapping_mul(y),
            self.z.wrapping_mul(z),
        )
    }
    pub fn differs_horizontally(self, other: Self) -> bool {
        self.x != other.x || self.z != other.z
    }
}
impl From<BlockPos> for Vec3i {
    fn from(pos: BlockPos) -> Self {
        pos.as_vec3i()
    }
}
impl From<Vec3i> for BlockPos {
    fn from(pos: Vec3i) -> Self {
        Self::new(pos.x, pos.y, pos.z)
    }
}
impl BlockPos {
    pub const PACKED_HORIZONTAL_LENGTH: u32 = 26;
    pub const PACKED_Y_LENGTH: u32 = 12;
    pub const MAX_HORIZONTAL_COORDINATE: i32 = (1 << 25) - 1;
    #[inline]
    pub fn containing(x: f64, y: f64, z: f64) -> Self {
        Self::new(mth::floor(x), mth::floor(y), mth::floor(z))
    }
    #[inline]
    pub fn of(node: i64) -> Self {
        Self::new(
            (node >> 38) as i32,
            ((node << 52) >> 52) as i32,
            ((node << 26) >> 38) as i32,
        )
    }
    #[inline]
    pub fn as_long(self) -> i64 {
        ((i64::from(self.x) & 0x3ffffff) << 38)
            | ((i64::from(self.z) & 0x3ffffff) << 12)
            | (i64::from(self.y) & 0xfff)
    }
    #[inline]
    pub fn offset_packed(node: i64, x: i32, y: i32, z: i32) -> i64 {
        Self::of(node).offset(x, y, z).as_long()
    }
    #[inline]
    pub fn flat_index(node: i64) -> i64 {
        node & -16
    }
    #[inline]
    pub fn min(self, other: Self) -> Self {
        Self::new(
            self.x.min(other.x),
            self.y.min(other.y),
            self.z.min(other.z),
        )
    }
    #[inline]
    pub fn max(self, other: Self) -> Self {
        Self::new(
            self.x.max(other.x),
            self.y.max(other.y),
            self.z.max(other.z),
        )
    }
    #[inline]
    pub fn at_y(self, y: i32) -> Self {
        Self::new(self.x, y, self.z)
    }
    #[inline]
    pub fn rotate(self, rotation: Rotation) -> Self {
        match rotation {
            Rotation::None => self,
            Rotation::Clockwise90 => Self::new(self.z.wrapping_neg(), self.y, self.x),
            Rotation::Clockwise180 => {
                Self::new(self.x.wrapping_neg(), self.y, self.z.wrapping_neg())
            }
            Rotation::CounterClockwise90 => Self::new(self.z, self.y, self.x.wrapping_neg()),
        }
    }
    pub fn mutable(self) -> MutableBlockPos {
        MutableBlockPos { position: self }
    }
    pub fn clamp_location_within(self, v: Vec3) -> Vec3 {
        Vec3::new(
            mth::clamp(
                v.x,
                f64::from(self.x as f32 + mth::EPSILON),
                f64::from(self.x) + 1.0 - f64::from(mth::EPSILON),
            ),
            mth::clamp(
                v.y,
                f64::from(self.y as f32 + mth::EPSILON),
                f64::from(self.y) + 1.0 - f64::from(mth::EPSILON),
            ),
            mth::clamp(
                v.z,
                f64::from(self.z as f32 + mth::EPSILON),
                f64::from(self.z) + 1.0 - f64::from(mth::EPSILON),
            ),
        )
    }
    /// Port of BlockPos#betweenClosed(BlockPos,BlockPos). X advances fastest,
    /// followed by Y, then Z. Each Rust item is an owned snapshot.
    pub fn between_closed(a: Self, b: Self) -> BetweenClosed {
        BetweenClosed::new(a.min(b), a.max(b))
    }
}
/// Port of BlockPos.MutableBlockPos. Reads use BlockPos methods; mutation is explicit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MutableBlockPos {
    position: BlockPos,
}
impl std::ops::Deref for MutableBlockPos {
    type Target = BlockPos;
    fn deref(&self) -> &BlockPos {
        &self.position
    }
}
impl MutableBlockPos {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self {
            position: BlockPos::new(x, y, z),
        }
    }
    pub fn immutable(&self) -> BlockPos {
        self.position
    }
    pub fn set(&mut self, x: i32, y: i32, z: i32) -> &mut Self {
        self.position = BlockPos::new(x, y, z);
        self
    }
    pub fn set_containing(&mut self, x: f64, y: f64, z: f64) -> &mut Self {
        self.position = BlockPos::containing(x, y, z);
        self
    }
    pub fn set_packed(&mut self, node: i64) -> &mut Self {
        self.position = BlockPos::of(node);
        self
    }
    pub fn set_with_offset(&mut self, pos: BlockPos, x: i32, y: i32, z: i32) -> &mut Self {
        self.position = pos.offset(x, y, z);
        self
    }
    pub fn set_x(&mut self, x: i32) -> &mut Self {
        self.position.x = x;
        self
    }
    pub fn set_y(&mut self, y: i32) -> &mut Self {
        self.position.y = y;
        self
    }
    pub fn set_z(&mut self, z: i32) -> &mut Self {
        self.position.z = z;
        self
    }
    pub fn move_by(&mut self, x: i32, y: i32, z: i32) -> &mut Self {
        self.position = self.position.offset(x, y, z);
        self
    }
    pub fn move_direction(&mut self, direction: Direction, steps: i32) -> &mut Self {
        self.position = self.position.relative(direction, steps);
        self
    }
    pub fn clamp(&mut self, axis: Axis, min: i32, max: i32) -> &mut Self {
        let value = mth::clamp_int(self.position.get(axis), min, max);
        match axis {
            Axis::X => self.set_x(value),
            Axis::Y => self.set_y(value),
            Axis::Z => self.set_z(value),
        }
    }
}
/// Port of BlockPos#betweenClosed(int,...). Java's int dimensions/count wrap.
#[derive(Clone, Debug)]
pub struct BetweenClosed {
    min: BlockPos,
    width: i32,
    height: i32,
    index: i32,
    limit: i32,
}
impl BetweenClosed {
    pub fn new(min: BlockPos, max: BlockPos) -> Self {
        let width = max.x.wrapping_sub(min.x).wrapping_add(1);
        let height = max.y.wrapping_sub(min.y).wrapping_add(1);
        let depth = max.z.wrapping_sub(min.z).wrapping_add(1);
        Self {
            min,
            width,
            height,
            index: 0,
            limit: width.wrapping_mul(height).wrapping_mul(depth),
        }
    }
}
impl Iterator for BetweenClosed {
    type Item = BlockPos;
    fn next(&mut self) -> Option<BlockPos> {
        if self.index == self.limit {
            return None;
        }
        let x = self.index.wrapping_rem(self.width);
        let remaining = self.index.wrapping_div(self.width);
        let y = remaining.wrapping_rem(self.height);
        let z = remaining.wrapping_div(self.height);
        self.index = self.index.wrapping_add(1);
        Some(self.min.offset(x, y, z))
    }
}
impl SectionPos {
    pub const SECTION_BITS: u32 = 4;
    pub const SECTION_SIZE: i32 = 16;
    pub const SECTION_BLOCK_COUNT: usize = 4096;
    #[inline]
    pub fn of(node: i64) -> Self {
        Self::new(
            (node >> 42) as i32,
            ((node << 44) >> 44) as i32,
            ((node << 22) >> 42) as i32,
        )
    }
    #[inline]
    pub fn as_long(self) -> i64 {
        ((i64::from(self.x) & 0x3fffff) << 42)
            | ((i64::from(self.z) & 0x3fffff) << 20)
            | (i64::from(self.y) & 0xfffff)
    }
    #[inline]
    pub fn from_block(pos: BlockPos) -> Self {
        Self::new(pos.x >> 4, pos.y >> 4, pos.z >> 4)
    }
    #[inline]
    pub fn from_position(pos: Vec3) -> Self {
        Self::from_block(BlockPos::containing(pos.x, pos.y, pos.z))
    }
    #[inline]
    pub fn block_to_section_coord(block: i32) -> i32 {
        block >> 4
    }
    #[inline]
    pub fn section_to_block_coord(section: i32) -> i32 {
        section.wrapping_shl(4)
    }
    #[inline]
    pub fn section_relative(block: i32) -> i32 {
        block & 15
    }
    #[inline]
    pub fn section_relative_pos(pos: BlockPos) -> i16 {
        (((pos.x & 15) << 8) | ((pos.z & 15) << 4) | (pos.y & 15)) as i16
    }
    #[inline]
    pub fn section_relative_x(relative: i16) -> i32 {
        (i32::from(relative) >> 8) & 15
    }
    #[inline]
    pub fn section_relative_y(relative: i16) -> i32 {
        i32::from(relative) & 15
    }
    #[inline]
    pub fn section_relative_z(relative: i16) -> i32 {
        (i32::from(relative) >> 4) & 15
    }
    pub fn relative_to_block_pos(self, relative: i16) -> BlockPos {
        self.origin().offset(
            Self::section_relative_x(relative),
            Self::section_relative_y(relative),
            Self::section_relative_z(relative),
        )
    }
    #[inline]
    pub fn origin(self) -> BlockPos {
        BlockPos::new(
            self.x.wrapping_shl(4),
            self.y.wrapping_shl(4),
            self.z.wrapping_shl(4),
        )
    }
    pub fn center(self) -> BlockPos {
        self.origin().offset(8, 8, 8)
    }
    pub fn chunk(self) -> ChunkPos {
        ChunkPos::new(self.x, self.z)
    }
    pub fn blocks_inside(self) -> BetweenClosed {
        let origin = self.origin();
        BetweenClosed::new(origin, origin.offset(15, 15, 15))
    }
    #[inline]
    pub fn zero_node(node: i64) -> i64 {
        node & -1048576
    }
    #[inline]
    pub fn offset_packed(node: i64, x: i32, y: i32, z: i32) -> i64 {
        Self::of(node).offset(x, y, z).as_long()
    }
    pub fn around_and_at_block_pos(pos: BlockPos, mut consume: impl FnMut(i64)) {
        let min = Self::from_block(pos.offset(-1, -1, -1));
        let max = Self::from_block(pos.offset(1, 1, 1));
        for x in min.x..=max.x {
            for y in min.y..=max.y {
                for z in min.z..=max.z {
                    consume(Self::new(x, y, z).as_long());
                }
            }
        }
    }
}
/// Port of `world.level.ChunkPos`. 26.3 calls toLong/of `pack`/`unpack`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}
impl ChunkPos {
    pub const ZERO: Self = Self::new(0, 0);
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }
    #[inline]
    pub fn containing(pos: BlockPos) -> Self {
        Self::new(pos.x >> 4, pos.z >> 4)
    }
    #[inline]
    pub fn pack(self) -> i64 {
        (i64::from(self.x as u32)) | (i64::from(self.z) << 32)
    }
    #[inline]
    pub fn unpack(node: i64) -> Self {
        Self::new(node as i32, (node >> 32) as i32)
    }
    #[inline]
    pub fn to_long(self) -> i64 {
        self.pack()
    }
    #[inline]
    pub fn min_block_x(self) -> i32 {
        self.x.wrapping_shl(4)
    }
    #[inline]
    pub fn min_block_z(self) -> i32 {
        self.z.wrapping_shl(4)
    }
    #[inline]
    pub fn block_at(self, x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(
            self.min_block_x().wrapping_add(x),
            y,
            self.min_block_z().wrapping_add(z),
        )
    }
    pub fn middle_block_position(self, y: i32) -> BlockPos {
        self.block_at(8, y, 8)
    }
    pub fn region_x(self) -> i32 {
        self.x >> 5
    }
    pub fn region_z(self) -> i32 {
        self.z >> 5
    }
    pub fn region_local_x(self) -> i32 {
        self.x & 31
    }
    pub fn region_local_z(self) -> i32 {
        self.z & 31
    }
    pub fn distance_squared(self, other: Self) -> i32 {
        let x = other.x.wrapping_sub(self.x);
        let z = other.z.wrapping_sub(self.z);
        x.wrapping_mul(x).wrapping_add(z.wrapping_mul(z))
    }
    pub fn chessboard_distance(self, other: Self) -> i32 {
        self.x
            .wrapping_sub(other.x)
            .wrapping_abs()
            .max(self.z.wrapping_sub(other.z).wrapping_abs())
    }
    pub fn java_hash_code(self) -> i32 {
        1664525_i32.wrapping_mul(self.x).wrapping_add(1013904223)
            ^ 1664525_i32
                .wrapping_mul(self.z ^ -559038737)
                .wrapping_add(1013904223)
    }
    pub fn range_closed(from: Self, to: Self) -> impl Iterator<Item = Self> {
        let mut next = Some(from);
        std::iter::from_fn(move || {
            let pos = next?;
            next = if pos.x == to.x {
                if pos.z == to.z {
                    None
                } else {
                    Some(Self::new(
                        from.x,
                        pos.z.wrapping_add(if from.z < to.z { 1 } else { -1 }),
                    ))
                }
            } else {
                Some(Self::new(
                    pos.x.wrapping_add(if from.x < to.x { 1 } else { -1 }),
                    pos.z,
                ))
            };
            Some(pos)
        })
    }
}
/// Port of net.minecraft.core.QuartPos.
pub struct QuartPos;
impl QuartPos {
    #[inline]
    pub fn from_block(block: i32) -> i32 {
        block >> 2
    }
    #[inline]
    pub fn quart_local(block: i32) -> i32 {
        block & 3
    }
    #[inline]
    pub fn to_block(quart: i32) -> i32 {
        quart.wrapping_shl(2)
    }
    #[inline]
    pub fn from_section(section: i32) -> i32 {
        section.wrapping_shl(2)
    }
    #[inline]
    pub fn to_section(quart: i32) -> i32 {
        quart >> 2
    }
}
