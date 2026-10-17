//! Vanilla 26.3 voxel geometry. Coordinates, discrete occupancy, traversal order
//! and epsilon comparisons follow `net.minecraft.world.phys.shapes`.

mod context;
mod discrete;
mod merger;
mod rotation;
mod voxel;

pub use context::{
    CollisionContext, CollisionShapeProvider, EmptyCollisionContext, EntityCollisionContext,
    EntityCollisionData, PositionCollisionContext,
};
pub use discrete::{BitSetDiscreteVoxelShape, DiscreteVoxelShape};
pub use merger::{IndexMerger, IndexMergerKind};
pub use rotation::OctahedralGroup;
pub use voxel::{ShapeHit, VoxelShape, VoxelShapeKind};

use crate::{Aabb, Axis, AxisDirection, Direction, Vec3};
use std::sync::{Arc, OnceLock};

pub const EPSILON: f64 = 1.0e-7;
pub const BIG_EPSILON: f64 = 1.0e-6;

/// Ports CubePointRange, DoubleArrayList and OffsetDoubleList. Nested offsets
/// are kept separate because regrouping additions changes double rounding.
#[derive(Clone, Debug)]
pub enum Coordinates {
    Cube(usize),
    Array(Arc<[f64]>),
    Offset { source: Arc<Self>, offset: f64 },
}

impl Coordinates {
    pub fn cube(intervals: usize) -> Self {
        Self::Cube(intervals)
    }
    pub fn array(values: impl Into<Arc<[f64]>>) -> Self {
        Self::Array(values.into())
    }
    pub fn offset(&self, offset: f64) -> Self {
        Self::Offset {
            source: Arc::new(self.clone()),
            offset,
        }
    }
    pub fn len(&self) -> usize {
        match self {
            Self::Cube(n) => n + 1,
            Self::Array(a) => a.len(),
            Self::Offset { source, .. } => source.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn is_cube(&self) -> bool {
        matches!(self, Self::Cube(_))
    }
    pub fn get(&self, index: usize) -> f64 {
        assert!(index < self.len());
        match self {
            Self::Cube(n) => index as f64 / *n as f64,
            Self::Array(a) => a[index],
            Self::Offset { source, offset } => source.get(index) + offset,
        }
    }
}

/// Port of BooleanOp's complete truth table. Operations that include empty
/// space (`apply(false, false)`) are rejected by finite shape joins, as in Java.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BooleanOp {
    False,
    NotOr,
    OnlySecond,
    NotFirst,
    OnlyFirst,
    NotSecond,
    NotSame,
    NotAnd,
    And,
    Same,
    Second,
    Causes,
    First,
    CausedBy,
    Or,
    True,
}
impl BooleanOp {
    pub const VALUES: [Self; 16] = [
        Self::False,
        Self::NotOr,
        Self::OnlySecond,
        Self::NotFirst,
        Self::OnlyFirst,
        Self::NotSecond,
        Self::NotSame,
        Self::NotAnd,
        Self::And,
        Self::Same,
        Self::Second,
        Self::Causes,
        Self::First,
        Self::CausedBy,
        Self::Or,
        Self::True,
    ];
    pub fn apply(self, first: bool, second: bool) -> bool {
        (self as u8) & (1 << (u8::from(first) * 2 + u8::from(second))) != 0
    }
}

pub fn empty() -> VoxelShape {
    static EMPTY: OnceLock<VoxelShape> = OnceLock::new();
    EMPTY
        .get_or_init(|| {
            VoxelShape::array(
                BitSetDiscreteVoxelShape::new(0, 0, 0).into(),
                std::array::from_fn(|_| Coordinates::array(vec![0.0])),
            )
        })
        .clone()
}
pub fn block() -> VoxelShape {
    static BLOCK: OnceLock<VoxelShape> = OnceLock::new();
    BLOCK
        .get_or_init(|| {
            VoxelShape::cube(
                BitSetDiscreteVoxelShape::with_filled_bounds([1; 3], [0; 3], [1; 3]).into(),
            )
        })
        .clone()
}
pub fn infinity() -> VoxelShape {
    static INFINITY: OnceLock<VoxelShape> = OnceLock::new();
    INFINITY
        .get_or_init(|| {
            box_shape(
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::INFINITY,
                f64::INFINITY,
            )
        })
        .clone()
}

/// Port of Shapes#box. Invalid ordered bounds panic, like Java's exception.
pub fn box_shape(
    min_x: f64,
    min_y: f64,
    min_z: f64,
    max_x: f64,
    max_y: f64,
    max_z: f64,
) -> VoxelShape {
    assert!(
        !(min_x > max_x || min_y > max_y || min_z > max_z),
        "shape minimum exceeds maximum"
    );
    create(min_x, min_y, min_z, max_x, max_y, max_z)
}

/// Port of Block#box; inputs use sixteenths of a block.
pub fn block_box(
    min_x: f64,
    min_y: f64,
    min_z: f64,
    max_x: f64,
    max_y: f64,
    max_z: f64,
) -> VoxelShape {
    box_shape(
        min_x / 16.0,
        min_y / 16.0,
        min_z / 16.0,
        max_x / 16.0,
        max_y / 16.0,
        max_z / 16.0,
    )
}

/// Port of Shapes#create, including snapping to grids of at most eight cells.
pub fn create(
    min_x: f64,
    min_y: f64,
    min_z: f64,
    max_x: f64,
    max_y: f64,
    max_z: f64,
) -> VoxelShape {
    let min = [min_x, min_y, min_z];
    let max = [max_x, max_y, max_z];
    if (0..3).any(|a| max[a] - min[a] < EPSILON) {
        return empty();
    }
    let bits = std::array::from_fn::<_, 3, _>(|a| find_bits(min[a], max[a]));
    if bits.iter().any(|b| b.is_none()) {
        return VoxelShape::array(
            block().discrete().clone(),
            std::array::from_fn(|a| Coordinates::array(vec![min[a], max[a]])),
        );
    }
    let size = bits.map(|b| 1 << b.expect("grid resolution"));
    if size == [1; 3] {
        return block();
    }
    VoxelShape::cube(
        BitSetDiscreteVoxelShape::with_filled_bounds(
            size,
            std::array::from_fn(|a| java_round(min[a] * f64::from(size[a])) as i32),
            std::array::from_fn(|a| java_round(max[a] * f64::from(size[a])) as i32),
        )
        .into(),
    )
}
pub fn create_aabb(aabb: Aabb) -> VoxelShape {
    create(
        aabb.min_x, aabb.min_y, aabb.min_z, aabb.max_x, aabb.max_y, aabb.max_z,
    )
}

fn java_round(value: f64) -> i64 {
    // Math.round uses floor with ties towards positive infinity. Splitting the
    // integer and fraction avoids rounding nextDown(0.5) up during addition.
    let floor = value.floor();
    (floor + if value - floor >= 0.5 { 1.0 } else { 0.0 }) as i64
}

pub fn find_bits(min: f64, max: f64) -> Option<u32> {
    if min < -EPSILON || max > 1.0000001 {
        return None;
    }
    (0..=3).find(|&b| {
        let n = f64::from(1 << b);
        let low = min * n;
        let high = max * n;
        (low - java_round(low) as f64).abs() < EPSILON * n
            && (high - java_round(high) as f64).abs() < EPSILON * n
    })
}

pub fn join(first: &VoxelShape, second: &VoxelShape, op: BooleanOp) -> VoxelShape {
    join_unoptimized(first, second, op).optimize()
}
pub fn or(first: &VoxelShape, second: &VoxelShape) -> VoxelShape {
    join(first, second, BooleanOp::Or)
}

fn mergers(first: &VoxelShape, second: &VoxelShape, op: BooleanOp) -> [IndexMerger; 3] {
    let mut cost = 1;
    Axis::VALUES.map(|a| {
        let merger = IndexMerger::new(
            cost,
            first.coords(a),
            second.coords(a),
            op.apply(true, false),
            op.apply(false, true),
        );
        cost *= merger.coordinates().len() - 1;
        merger
    })
}

/// Port of Shapes#joinUnoptimized. Shared identity is observable in vanilla's
/// shortcuts and is therefore retained by VoxelShape's Arc storage.
pub fn join_unoptimized(first: &VoxelShape, second: &VoxelShape, op: BooleanOp) -> VoxelShape {
    assert!(
        !op.apply(false, false),
        "boolean operation includes infinite empty space"
    );
    if first.ptr_eq(second) {
        return if op.apply(true, true) {
            first.clone()
        } else {
            empty()
        };
    }
    if first.is_empty() {
        return if op.apply(false, true) {
            second.clone()
        } else {
            empty()
        };
    }
    if second.is_empty() {
        return if op.apply(true, false) {
            first.clone()
        } else {
            empty()
        };
    }
    let merged = mergers(first, second, op);
    let size = merged
        .each_ref()
        .map(|m| (m.coordinates().len() - 1) as i32);
    let mut shape = BitSetDiscreteVoxelShape::new(size[0], size[1], size[2]);
    for (x, &[x1, x2]) in merged[0].indexes().iter().enumerate() {
        for (y, &[y1, y2]) in merged[1].indexes().iter().enumerate() {
            for (z, &[z1, z2]) in merged[2].indexes().iter().enumerate() {
                if op.apply(
                    first.discrete().is_full_wide(x1, y1, z1),
                    second.discrete().is_full_wide(x2, y2, z2),
                ) {
                    shape.fill(x as i32, y as i32, z as i32);
                }
            }
        }
    }
    if shape.is_empty() {
        shape.set_bounds([i32::MAX; 3], [i32::MIN + 1; 3]);
    }
    if merged
        .iter()
        .all(|m| m.kind() == IndexMergerKind::DiscreteCube)
    {
        VoxelShape::cube(shape.into())
    } else {
        VoxelShape::array(shape.into(), merged.map(|m| m.coordinates().clone()))
    }
}

pub fn join_is_not_empty(first: &VoxelShape, second: &VoxelShape, op: BooleanOp) -> bool {
    assert!(
        !op.apply(false, false),
        "boolean operation includes infinite empty space"
    );
    let (a, b) = (first.is_empty(), second.is_empty());
    if a || b {
        return op.apply(!a, !b);
    }
    if first.ptr_eq(second) {
        return op.apply(true, true);
    }
    for axis in Axis::VALUES {
        if first.max(axis) < second.min(axis) - EPSILON
            || second.max(axis) < first.min(axis) - EPSILON
        {
            return op.apply(true, false) || op.apply(false, true);
        }
    }
    let merged = mergers(first, second, op);
    merged[0].indexes().iter().any(|&[x1, x2]| {
        merged[1].indexes().iter().any(|&[y1, y2]| {
            merged[2].indexes().iter().any(|&[z1, z2]| {
                op.apply(
                    first.discrete().is_full_wide(x1, y1, z1),
                    second.discrete().is_full_wide(x2, y2, z2),
                )
            })
        })
    })
}

pub fn equal(first: &VoxelShape, second: &VoxelShape) -> bool {
    !join_is_not_empty(first, second, BooleanOp::NotSame)
}

pub fn collide<'a>(
    axis: Axis,
    moving: Aabb,
    shapes: impl IntoIterator<Item = &'a VoxelShape>,
    mut distance: f64,
) -> f64 {
    for shape in shapes {
        if distance.abs() < EPSILON {
            return 0.0;
        }
        distance = shape.collide(axis, moving, distance);
    }
    distance
}

fn fuzzy_equals(a: f64, b: f64) -> bool {
    (a - b).abs() <= EPSILON || a == b || a.is_nan() && b.is_nan()
}

pub fn block_occludes(shape: &VoxelShape, occluder: &VoxelShape, direction: Direction) -> bool {
    if shape.ptr_eq(&block()) && occluder.ptr_eq(&block()) {
        return true;
    }
    if occluder.is_empty() {
        return false;
    }
    let axis = direction.axis();
    let positive = direction.axis_direction() == AxisDirection::Positive;
    let (a, b) = if positive {
        (shape, occluder)
    } else {
        (occluder, shape)
    };
    fuzzy_equals(a.max(axis), 1.0)
        && fuzzy_equals(b.min(axis), 0.0)
        && !join_is_not_empty(
            &a.slice(axis, a.discrete().size(axis) - 1),
            &b.slice(axis, 0),
            if positive {
                BooleanOp::OnlyFirst
            } else {
                BooleanOp::OnlySecond
            },
        )
}

pub fn merged_face_occludes(
    shape: &VoxelShape,
    occluder: &VoxelShape,
    direction: Direction,
) -> bool {
    if shape.ptr_eq(&block()) || occluder.ptr_eq(&block()) {
        return true;
    }
    let axis = direction.axis();
    let (a, b) = if direction.axis_direction() == AxisDirection::Positive {
        (shape, occluder)
    } else {
        (occluder, shape)
    };
    let a = if fuzzy_equals(a.max(axis), 1.0) {
        a.clone()
    } else {
        empty()
    };
    let b = if fuzzy_equals(b.min(axis), 0.0) {
        b.clone()
    } else {
        empty()
    };
    !join_is_not_empty(
        &block(),
        &join_unoptimized(
            &a.slice(axis, a.discrete().size(axis) - 1),
            &b.slice(axis, 0),
            BooleanOp::Or,
        ),
        BooleanOp::OnlyFirst,
    )
}

pub fn face_shape_occludes(shape: &VoxelShape, occluder: &VoxelShape) -> bool {
    if shape.ptr_eq(&block()) || occluder.ptr_eq(&block()) {
        return true;
    }
    if shape.is_empty() && occluder.is_empty() {
        return false;
    }
    !join_is_not_empty(
        &block(),
        &join_unoptimized(shape, occluder, BooleanOp::Or),
        BooleanOp::OnlyFirst,
    )
}

/// Port of Block#isFaceFull and Block#isShapeFullBlock.
pub fn is_face_full(shape: &VoxelShape, direction: Direction) -> bool {
    is_shape_full_block(&shape.get_face_shape(direction))
}
pub fn is_shape_full_block(shape: &VoxelShape) -> bool {
    !join_is_not_empty(&block(), shape, BooleanOp::NotSame)
}

pub fn rotate(shape: &VoxelShape, rotation: OctahedralGroup) -> VoxelShape {
    rotate_around(shape, rotation, Vec3::new(0.5, 0.5, 0.5))
}
pub fn rotate_around(shape: &VoxelShape, rotation: OctahedralGroup, center: Vec3) -> VoxelShape {
    if rotation == OctahedralGroup::IDENTITY {
        return shape.clone();
    }
    let discrete = shape.discrete().rotate(rotation);
    if shape.kind() == VoxelShapeKind::Cube && center == Vec3::new(0.5, 0.5, 0.5) {
        return VoxelShape::cube(discrete);
    }
    let coords = std::array::from_fn(|i| {
        let old_axis = rotation.permutation()[i];
        let source = shape.coords(old_axis);
        let new_relative = center.get(old_axis);
        let old_relative = center.get(Axis::VALUES[i]);
        let flip = rotation.inverts(Axis::VALUES[i]);
        if !flip && new_relative == old_relative {
            return source.clone();
        }
        Coordinates::array(
            (0..source.len())
                .map(|j| {
                    let j = if flip { source.len() - 1 - j } else { j };
                    let relative = source.get(j) - new_relative;
                    (if flip { -relative } else { relative }) + old_relative
                })
                .collect::<Vec<_>>(),
        )
    });
    VoxelShape::array(discrete, coords)
}

#[cfg(test)]
mod tests;
