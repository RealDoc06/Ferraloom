use std::sync::Arc;

use crate::{Axis, Direction};

use super::OctahedralGroup;

/// Port of BitSetDiscreteVoxelShape. Z is the contiguous storage axis.
#[derive(Clone, Debug)]
pub struct BitSetDiscreteVoxelShape {
    size: [i32; 3],
    storage: Vec<u64>,
    first: [i32; 3],
    last: [i32; 3],
}

impl BitSetDiscreteVoxelShape {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        assert!(x >= 0 && y >= 0 && z >= 0, "negative discrete shape size");
        let len = (x as usize)
            .checked_mul(y as usize)
            .and_then(|n| n.checked_mul(z as usize))
            .expect("shape too large");
        Self {
            size: [x, y, z],
            storage: vec![0; len.div_ceil(64)],
            first: [x, y, z],
            last: [0; 3],
        }
    }

    pub fn with_filled_bounds(size: [i32; 3], first: [i32; 3], last: [i32; 3]) -> Self {
        let mut shape = Self::new(size[0], size[1], size[2]);
        for x in first[0]..last[0] {
            for y in first[1]..last[1] {
                for z in first[2]..last[2] {
                    shape.fill(x, y, z);
                }
            }
        }
        shape.first = first;
        shape.last = last;
        shape
    }

    fn index(&self, x: i32, y: i32, z: i32) -> usize {
        let index = (i64::from(x) * i64::from(self.size[1]) + i64::from(y))
            * i64::from(self.size[2])
            + i64::from(z);
        usize::try_from(index).expect("negative bit index")
    }

    pub fn is_full(&self, x: i32, y: i32, z: i32) -> bool {
        let i = self.index(x, y, z);
        self.storage
            .get(i / 64)
            .is_some_and(|word| word & (1 << (i % 64)) != 0)
    }

    pub fn is_full_wide(&self, x: i32, y: i32, z: i32) -> bool {
        [x, y, z]
            .into_iter()
            .zip(self.size)
            .all(|(p, n)| p >= 0 && p < n)
            && self.is_full(x, y, z)
    }

    pub fn fill(&mut self, x: i32, y: i32, z: i32) {
        assert!(
            [x, y, z]
                .into_iter()
                .zip(self.size)
                .all(|(p, n)| p >= 0 && p < n)
        );
        let i = self.index(x, y, z);
        self.storage[i / 64] |= 1 << (i % 64);
        for (a, p) in [x, y, z].into_iter().enumerate() {
            self.first[a] = self.first[a].min(p);
            self.last[a] = self.last[a].max(p + 1);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.storage.iter().all(|word| *word == 0)
    }
    pub fn size(&self, axis: Axis) -> i32 {
        self.size[axis as usize]
    }
    pub fn first_full(&self, axis: Axis) -> i32 {
        self.first[axis as usize]
    }
    pub fn last_full(&self, axis: Axis) -> i32 {
        self.last[axis as usize]
    }

    pub(super) fn set_bounds(&mut self, first: [i32; 3], last: [i32; 3]) {
        self.first = first;
        self.last = last;
    }

    pub fn is_interior(&self, x: i32, y: i32, z: i32) -> bool {
        x > 0
            && x < self.size[0] - 1
            && y > 0
            && y < self.size[1] - 1
            && z > 0
            && z < self.size[2] - 1
            && self.is_full(x, y, z)
            && self.is_full(x - 1, y, z)
            && self.is_full(x + 1, y, z)
            && self.is_full(x, y - 1, z)
            && self.is_full(x, y + 1, z)
            && self.is_full(x, y, z - 1)
            && self.is_full(x, y, z + 1)
    }

    fn clear_strip(&mut self, start: i32, end: i32, x: i32, y: i32) {
        for z in start..end {
            let i = self.index(x, y, z);
            self.storage[i / 64] &= !(1 << (i % 64));
        }
    }

    fn strip_full(&self, start: i32, end: i32, x: i32, y: i32) -> bool {
        x < self.size[0] && y < self.size[1] && (start..end).all(|z| self.is_full(x, y, z))
    }

    fn boxes(mut self, mut consumer: impl FnMut([i32; 6]), merge: bool) {
        for y in 0..self.size[1] {
            for x in 0..self.size[0] {
                let mut start = None;
                for z in 0..=self.size[2] {
                    if self.is_full_wide(x, y, z) {
                        if merge {
                            start.get_or_insert(z);
                        } else {
                            consumer([x, y, z, x + 1, y + 1, z + 1]);
                        }
                    } else if let Some(start_z) = start.take() {
                        let mut end_x = x;
                        let mut end_y = y;
                        self.clear_strip(start_z, z, x, y);
                        while self.strip_full(start_z, z, end_x + 1, y) {
                            end_x += 1;
                            self.clear_strip(start_z, z, end_x, y);
                        }
                        while (x..=end_x).all(|cx| self.strip_full(start_z, z, cx, end_y + 1)) {
                            end_y += 1;
                            for cx in x..=end_x {
                                self.clear_strip(start_z, z, cx, end_y);
                            }
                        }
                        consumer([x, y, start_z, end_x + 1, end_y + 1, z]);
                    }
                }
            }
        }
    }
}

/// Shared port of DiscreteVoxelShape and SubShape. Sub-shape bounds deliberately
/// clamp the parent's bounds rather than recomputing occupied bounds.
#[derive(Clone, Debug)]
pub struct DiscreteVoxelShape(Arc<DiscreteStorage>);

#[derive(Debug)]
enum DiscreteStorage {
    BitSet(BitSetDiscreteVoxelShape),
    Sub {
        parent: DiscreteVoxelShape,
        start: [i32; 3],
        end: [i32; 3],
    },
}

impl From<BitSetDiscreteVoxelShape> for DiscreteVoxelShape {
    fn from(value: BitSetDiscreteVoxelShape) -> Self {
        Self(Arc::new(DiscreteStorage::BitSet(value)))
    }
}

impl DiscreteVoxelShape {
    pub fn sub_shape(&self, start: [i32; 3], end: [i32; 3]) -> Self {
        assert!((0..3).all(|a| end[a] >= start[a]));
        Self(Arc::new(DiscreteStorage::Sub {
            parent: self.clone(),
            start,
            end,
        }))
    }

    pub fn size(&self, axis: Axis) -> i32 {
        match self.0.as_ref() {
            DiscreteStorage::BitSet(shape) => shape.size(axis),
            DiscreteStorage::Sub { start, end, .. } => end[axis as usize] - start[axis as usize],
        }
    }

    pub fn first_full(&self, axis: Axis) -> i32 {
        self.bound(axis, false)
    }
    pub fn last_full(&self, axis: Axis) -> i32 {
        self.bound(axis, true)
    }

    fn bound(&self, axis: Axis, last: bool) -> i32 {
        match self.0.as_ref() {
            DiscreteStorage::BitSet(shape) => {
                if last {
                    shape.last_full(axis)
                } else {
                    shape.first_full(axis)
                }
            }
            DiscreteStorage::Sub { parent, start, end } => {
                let a = axis as usize;
                parent.bound(axis, last).clamp(start[a], end[a]) - start[a]
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        match self.0.as_ref() {
            DiscreteStorage::BitSet(shape) => shape.is_empty(),
            DiscreteStorage::Sub { .. } => Axis::VALUES
                .into_iter()
                .any(|a| self.first_full(a) >= self.last_full(a)),
        }
    }

    pub fn is_full(&self, x: i32, y: i32, z: i32) -> bool {
        match self.0.as_ref() {
            DiscreteStorage::BitSet(shape) => shape.is_full(x, y, z),
            DiscreteStorage::Sub { parent, start, .. } => {
                parent.is_full(x + start[0], y + start[1], z + start[2])
            }
        }
    }

    pub fn is_full_wide(&self, x: i32, y: i32, z: i32) -> bool {
        [x, y, z]
            .into_iter()
            .zip(Axis::VALUES)
            .all(|(p, a)| p >= 0 && p < self.size(a))
            && self.is_full(x, y, z)
    }

    pub fn first_full_at(&self, axis: Axis, b: i32, c: i32) -> i32 {
        (0..self.size(axis))
            .find(|&a| self.full_cycled(axis as usize, a, b, c))
            .unwrap_or(self.size(axis))
    }

    pub fn last_full_at(&self, axis: Axis, b: i32, c: i32) -> i32 {
        (0..self.size(axis))
            .rev()
            .find(|&a| self.full_cycled(axis as usize, a, b, c))
            .map_or(0, |a| a + 1)
    }

    // `cycle` is AxisCycle.NONE/FORWARD/BACKWARD in vanilla's order.
    pub(super) fn full_cycled(&self, cycle: usize, a: i32, b: i32, c: i32) -> bool {
        let p = cycle_point(cycle, [a, b, c]);
        self.is_full_wide(p[0], p[1], p[2])
    }

    pub fn to_bit_set(&self) -> BitSetDiscreteVoxelShape {
        if let DiscreteStorage::BitSet(shape) = self.0.as_ref() {
            return shape.clone();
        }
        let mut shape = BitSetDiscreteVoxelShape::new(
            self.size(Axis::X),
            self.size(Axis::Y),
            self.size(Axis::Z),
        );
        for x in 0..shape.size[0] {
            for y in 0..shape.size[1] {
                for z in 0..shape.size[2] {
                    if self.is_full(x, y, z) {
                        shape.fill(x, y, z);
                    }
                }
            }
        }
        shape.first = Axis::VALUES.map(|a| self.first_full(a));
        shape.last = Axis::VALUES.map(|a| self.last_full(a));
        shape
    }

    pub fn for_all_boxes(&self, consumer: impl FnMut([i32; 6]), merge_neighbors: bool) {
        self.to_bit_set().boxes(consumer, merge_neighbors);
    }

    pub fn for_all_edges(&self, mut consumer: impl FnMut([i32; 6]), merge_neighbors: bool) {
        for cycle in [0, 2, 1] {
            let size = cycle_point((3 - cycle) % 3, Axis::VALUES.map(|a| self.size(a)));
            for a in 0..=size[0] {
                for b in 0..=size[1] {
                    let mut start = None;
                    for c in 0..=size[2] {
                        let mut count = 0;
                        let mut odd = 0;
                        for da in 0..=1 {
                            for db in 0..=1 {
                                if self.full_cycled(cycle, a + da - 1, b + db - 1, c) {
                                    count += 1;
                                    odd ^= da ^ db;
                                }
                            }
                        }
                        if count == 1 || count == 3 || count == 2 && odd & 1 == 0 {
                            if merge_neighbors {
                                start.get_or_insert(c);
                            } else {
                                consumer(line(cycle, [a, b, c], [a, b, c + 1]));
                            }
                        } else if let Some(s) = start.take() {
                            consumer(line(cycle, [a, b, s], [a, b, c]));
                        }
                    }
                }
            }
        }
    }

    pub fn for_all_faces(&self, mut consumer: impl FnMut(Direction, [i32; 3])) {
        for cycle in [0, 2, 1] {
            let size = cycle_point((3 - cycle) % 3, Axis::VALUES.map(|a| self.size(a)));
            let axis = Axis::VALUES[(2 + cycle) % 3];
            for a in 0..size[0] {
                for b in 0..size[1] {
                    let mut last = false;
                    for c in 0..=size[2] {
                        let full = c != size[2] && self.full_cycled(cycle, a, b, c);
                        if !last && full {
                            consumer(axis.negative(), cycle_point(cycle, [a, b, c]));
                        }
                        if last && !full {
                            consumer(axis.positive(), cycle_point(cycle, [a, b, c - 1]));
                        }
                        last = full;
                    }
                }
            }
        }
    }

    pub fn rotate(&self, rotation: OctahedralGroup) -> Self {
        if rotation == OctahedralGroup::IDENTITY {
            return self.clone();
        }
        let old_size = Axis::VALUES.map(|a| self.size(a));
        let size = rotation.permutation().map(|a| old_size[a as usize]);
        let mut result = BitSetDiscreteVoxelShape::new(size[0], size[1], size[2]);
        for x in 0..old_size[0] {
            for y in 0..old_size[1] {
                for z in 0..old_size[2] {
                    if self.is_full(x, y, z) {
                        let old = [x, y, z];
                        let p: [i32; 3] = std::array::from_fn(|i| {
                            let p = old[rotation.permutation()[i] as usize];
                            if rotation.inverts(Axis::VALUES[i]) {
                                size[i] - 1 - p
                            } else {
                                p
                            }
                        });
                        result.fill(p[0], p[1], p[2]);
                    }
                }
            }
        }
        result.into()
    }
}

pub(super) fn cycle_point<T: Copy>(cycle: usize, [x, y, z]: [T; 3]) -> [T; 3] {
    match cycle {
        0 => [x, y, z],
        1 => [z, x, y],
        2 => [y, z, x],
        _ => unreachable!(),
    }
}

fn line(cycle: usize, start: [i32; 3], end: [i32; 3]) -> [i32; 6] {
    let [x, y, z] = cycle_point(cycle, start);
    let [u, v, w] = cycle_point(cycle, end);
    [x, y, z, u, v, w]
}
