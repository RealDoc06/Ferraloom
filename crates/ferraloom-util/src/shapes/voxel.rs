use std::sync::{Arc, OnceLock};

use crate::{Aabb, Axis, AxisDirection, BlockPos, Direction, Vec3, java, mth};

use super::{
    BooleanOp, Coordinates, DiscreteVoxelShape, EPSILON, block, box_shape, discrete::cycle_point,
    empty, fuzzy_equals, join_unoptimized,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoxelShapeKind {
    Cube,
    Array,
    Slice,
}

/// Port of VoxelShape, CubeVoxelShape, ArrayVoxelShape and SliceShape. Clones
/// share immutable occupancy, coordinate lists and thread-safe face caches.
#[derive(Clone, Debug)]
pub struct VoxelShape(Arc<ShapeData>);

#[derive(Debug)]
struct ShapeData {
    discrete: DiscreteVoxelShape,
    coords: [Coordinates; 3],
    kind: VoxelShapeKind,
    faces: [OnceLock<VoxelShape>; 6],
}

/// Geometry payload of vanilla's BlockHitResult, including the inside flag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeHit {
    pub location: Vec3,
    pub direction: Direction,
    pub block_pos: BlockPos,
    pub inside: bool,
}

impl VoxelShape {
    pub(super) fn new(
        discrete: DiscreteVoxelShape,
        coords: [Coordinates; 3],
        kind: VoxelShapeKind,
    ) -> Self {
        for axis in Axis::VALUES {
            assert_eq!(
                coords[axis as usize].len(),
                discrete.size(axis) as usize + 1,
                "coordinate length differs from discrete size"
            );
        }
        Self(Arc::new(ShapeData {
            discrete,
            coords,
            kind,
            faces: std::array::from_fn(|_| OnceLock::new()),
        }))
    }
    pub fn cube(discrete: DiscreteVoxelShape) -> Self {
        let coords = Axis::VALUES.map(|a| Coordinates::cube(discrete.size(a) as usize));
        Self::new(discrete, coords, VoxelShapeKind::Cube)
    }
    pub fn array(discrete: DiscreteVoxelShape, coords: [Coordinates; 3]) -> Self {
        Self::new(discrete, coords, VoxelShapeKind::Array)
    }
    pub fn kind(&self) -> VoxelShapeKind {
        self.0.kind
    }
    pub fn discrete(&self) -> &DiscreteVoxelShape {
        &self.0.discrete
    }
    pub fn coords(&self, axis: Axis) -> &Coordinates {
        &self.0.coords[axis as usize]
    }
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn is_empty(&self) -> bool {
        self.discrete().is_empty()
    }
    pub fn min(&self, axis: Axis) -> f64 {
        let i = self.discrete().first_full(axis);
        if i >= self.discrete().size(axis) {
            f64::INFINITY
        } else {
            self.coords(axis).get(i as usize)
        }
    }
    pub fn max(&self, axis: Axis) -> f64 {
        let i = self.discrete().last_full(axis);
        if i <= 0 {
            f64::NEG_INFINITY
        } else {
            self.coords(axis).get(i as usize)
        }
    }
    pub fn bounds(&self) -> Aabb {
        assert!(!self.is_empty(), "no bounds for empty shape");
        Aabb::new(
            self.min(Axis::X),
            self.min(Axis::Y),
            self.min(Axis::Z),
            self.max(Axis::X),
            self.max(Axis::Y),
            self.max(Axis::Z),
        )
    }
    pub fn single_encompassing(&self) -> Self {
        if self.is_empty() {
            empty()
        } else {
            super::create_aabb(self.bounds())
        }
    }
    pub fn move_by(&self, dx: f64, dy: f64, dz: f64) -> Self {
        if self.is_empty() {
            return empty();
        }
        let offsets = [dx, dy, dz];
        Self::array(
            self.discrete().clone(),
            std::array::from_fn(|i| self.0.coords[i].offset(offsets[i])),
        )
    }
    pub fn moved(&self, delta: Vec3) -> Self {
        self.move_by(delta.x, delta.y, delta.z)
    }
    pub fn optimize(&self) -> Self {
        let mut result = empty();
        self.for_all_boxes(|[x, y, z, u, v, w]| {
            result = join_unoptimized(&result, &box_shape(x, y, z, u, v, w), BooleanOp::Or);
        });
        result
    }
    fn coordinates_of(&self, line: [i32; 6]) -> [f64; 6] {
        std::array::from_fn(|i| self.0.coords[i % 3].get(line[i] as usize))
    }
    pub fn for_all_boxes(&self, mut consumer: impl FnMut([f64; 6])) {
        self.discrete()
            .for_all_boxes(|line| consumer(self.coordinates_of(line)), true);
    }
    pub fn for_all_edges(&self, mut consumer: impl FnMut([f64; 6])) {
        self.discrete()
            .for_all_edges(|line| consumer(self.coordinates_of(line)), true);
    }
    pub fn to_aabbs(&self) -> Vec<Aabb> {
        let mut boxes = Vec::new();
        self.for_all_boxes(|[x, y, z, u, v, w]| boxes.push(Aabb::new(x, y, z, u, v, w)));
        boxes
    }
    pub fn min_at(&self, axis: Axis, b: f64, c: f64) -> f64 {
        let a = axis as usize;
        let i = self.discrete().first_full_at(
            axis,
            self.find_index(Axis::VALUES[(a + 1) % 3], b),
            self.find_index(Axis::VALUES[(a + 2) % 3], c),
        );
        if i >= self.discrete().size(axis) {
            f64::INFINITY
        } else {
            self.coords(axis).get(i as usize)
        }
    }
    pub fn max_at(&self, axis: Axis, b: f64, c: f64) -> f64 {
        let a = axis as usize;
        let i = self.discrete().last_full_at(
            axis,
            self.find_index(Axis::VALUES[(a + 1) % 3], b),
            self.find_index(Axis::VALUES[(a + 2) % 3], c),
        );
        if i <= 0 {
            f64::NEG_INFINITY
        } else {
            self.coords(axis).get(i as usize)
        }
    }
    /// CubeVoxelShape uses multiply/clamp/floor; all other representations use
    /// vanilla's upper-bound binary search, even when their lists are cubes.
    pub fn find_index(&self, axis: Axis, coord: f64) -> i32 {
        if self.kind() == VoxelShapeKind::Cube {
            let n = f64::from(self.discrete().size(axis));
            return mth::floor(mth::clamp(coord * n, -1.0, n));
        }
        let coords = self.coords(axis);
        let (mut lo, mut count) = (0, coords.len());
        while count > 0 {
            let half = count / 2;
            let mid = lo + half;
            if coord < coords.get(mid) {
                count = half;
            } else {
                lo = mid + 1;
                count -= half + 1;
            }
        }
        lo as i32 - 1
    }
    pub fn clip(&self, from: Vec3, to: Vec3, pos: BlockPos) -> Option<ShapeHit> {
        if self.is_empty() {
            return None;
        }
        let diff = to - from;
        if diff.length_sqr() < EPSILON {
            return None;
        }
        let test = from + diff.scale(0.001);
        if self.discrete().is_full_wide(
            self.find_index(Axis::X, test.x - f64::from(pos.x)),
            self.find_index(Axis::Y, test.y - f64::from(pos.y)),
            self.find_index(Axis::Z, test.z - f64::from(pos.z)),
        ) {
            return Some(ShapeHit {
                location: test,
                direction: Direction::get_approximate_nearest(diff.x, diff.y, diff.z).opposite(),
                block_pos: pos,
                inside: true,
            });
        }
        Aabb::clip_all(self.to_aabbs(), from, to, pos).map(|hit| ShapeHit {
            location: hit.location,
            direction: hit.direction,
            block_pos: pos,
            inside: false,
        })
    }
    pub fn closest_point_to(&self, point: Vec3) -> Option<Vec3> {
        if self.is_empty() {
            return None;
        }
        let mut closest = None;
        self.for_all_boxes(|[x, y, z, u, v, w]| {
            let p = Vec3::new(
                mth::clamp(point.x, x, u),
                mth::clamp(point.y, y, v),
                mth::clamp(point.z, z, w),
            );
            if closest.is_none_or(|old| point.distance_to_sqr(p) < point.distance_to_sqr(old)) {
                closest = Some(p);
            }
        });
        closest
    }
    pub fn slice(&self, axis: Axis, point: i32) -> Self {
        let start = std::array::from_fn(|i| if i == axis as usize { point } else { 0 });
        let end = std::array::from_fn(|i| {
            if i == axis as usize {
                point + 1
            } else {
                self.discrete().size(Axis::VALUES[i])
            }
        });
        let coords = std::array::from_fn(|i| {
            if i == axis as usize {
                Coordinates::cube(1)
            } else {
                self.0.coords[i].clone()
            }
        });
        Self::new(
            self.discrete().sub_shape(start, end),
            coords,
            VoxelShapeKind::Slice,
        )
    }
    fn is_cube_like_along(&self, axis: Axis) -> bool {
        let c = self.coords(axis);
        c.len() == 2 && fuzzy_equals(c.get(0), 0.0) && fuzzy_equals(c.get(1), 1.0)
    }
    pub fn get_face_shape(&self, direction: Direction) -> Self {
        if self.is_empty() || self.ptr_eq(&block()) {
            return self.clone();
        }
        // Returning self from a cache would form an Arc cycle. Vanilla returns
        // it directly too; only distinct slice objects need caching here.
        let axis = direction.axis();
        if self.is_cube_like_along(axis) {
            return self.clone();
        }
        self.0.faces[direction as usize]
            .get_or_init(|| {
                let point = if direction.axis_direction() == AxisDirection::Positive {
                    0.9999999
                } else {
                    EPSILON
                };
                let slice = self.slice(axis, self.find_index(axis, point));
                if slice.is_empty() {
                    empty()
                } else if Axis::VALUES
                    .into_iter()
                    .all(|a| slice.is_cube_like_along(a))
                {
                    block()
                } else {
                    slice
                }
            })
            .clone()
    }
    /// Port of VoxelShape#collideX, with axes cycled to vanilla's scan order.
    pub fn collide(&self, axis: Axis, moving: Aabb, mut distance: f64) -> f64 {
        if self.is_empty() {
            return distance;
        }
        if distance.abs() < EPSILON {
            return 0.0;
        }
        let cycle = axis as usize;
        let b = Axis::VALUES[(cycle + 1) % 3];
        let c = Axis::VALUES[(cycle + 2) % 3];
        let min_a = moving.min(axis);
        let max_a = moving.max(axis);
        let a_min = self.find_index(axis, min_a + EPSILON);
        let a_max = self.find_index(axis, max_a - EPSILON);
        let b_min = self.find_index(b, moving.min(b) + EPSILON).max(0);
        let b_max = (self.find_index(b, moving.max(b) - EPSILON) + 1).min(self.discrete().size(b));
        let c_min = self.find_index(c, moving.min(c) + EPSILON).max(0);
        let c_max = (self.find_index(c, moving.max(c) - EPSILON) + 1).min(self.discrete().size(c));
        let full = |a| {
            (b_min..b_max).any(|b| {
                (c_min..c_max).any(|c| {
                    let [x, y, z] = cycle_point(cycle, [a, b, c]);
                    self.discrete().is_full_wide(x, y, z)
                })
            })
        };
        if distance > 0.0 {
            for a in a_max + 1..self.discrete().size(axis) {
                if full(a) {
                    let next = self.coords(axis).get(a as usize) - max_a;
                    if next >= -EPSILON {
                        distance = java::min(distance, next);
                    }
                    return distance;
                }
            }
        } else if distance < 0.0 {
            for a in (0..a_min).rev() {
                if full(a) {
                    let next = self.coords(axis).get((a + 1) as usize) - min_a;
                    if next <= EPSILON {
                        distance = java::max(distance, next);
                    }
                    return distance;
                }
            }
        }
        distance
    }
}
