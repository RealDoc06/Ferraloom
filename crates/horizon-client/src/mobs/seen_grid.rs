//! The tick's mob snapshot indexed by where the boxes are, so a box query
//! (`LivingEntity.pushEntities`) reads the mobs near it instead of every
//! mob, and still meets them in snapshot order.
use crate::fast_hash::FxHashMap;
use horizon_core::geometry::{Aabb, Axis};

/// Width of a grid column, in blocks.
const CELL: f64 = 4.0;

/// Columns per side of a block of the coarse count, as a shift.
const BLOCK_SHIFT: i32 = 2;

/// Snapshot indices by the column holding each box's minimum corner.
#[derive(Default)]
pub struct SeenGrid {
    cells: FxHashMap<(i32, i32), Vec<u32>>,
    /// How many entries each block of columns holds: a wide query passes
    /// an empty block with one lookup instead of one per column.
    blocks: FxHashMap<(i32, i32), u32>,
    cell_of: Vec<(i32, i32)>,
    /// The widest box (along x or z) the grid has held: no box reaches
    /// further than this past its minimum corner.
    reach: f64,
}

fn column(x: f64, z: f64) -> (i32, i32) {
    ((x / CELL).floor() as i32, (z / CELL).floor() as i32)
}
fn corner(b: Aabb) -> (i32, i32) {
    column(b.min(Axis::X), b.min(Axis::Z))
}
fn block_of((x, z): (i32, i32)) -> (i32, i32) {
    (x >> BLOCK_SHIFT, z >> BLOCK_SHIFT)
}
fn width(b: Aabb) -> f64 {
    (b.max(Axis::X) - b.min(Axis::X)).max(b.max(Axis::Z) - b.min(Axis::Z))
}

impl SeenGrid {
    /// The boxes of the snapshot's entries, in order.
    pub fn new(boxes: impl Iterator<Item = Aabb>) -> Self {
        let mut grid = Self::default();
        for (i, b) in boxes.enumerate() {
            let at = corner(b);
            grid.reach = grid.reach.max(width(b));
            grid.cells.entry(at).or_default().push(i as u32);
            *grid.blocks.entry(block_of(at)).or_default() += 1;
            grid.cell_of.push(at);
        }
        grid
    }
    /// Entry `i`'s box is now `b` (the mob moved or resized).
    pub fn update(&mut self, i: usize, b: Aabb) {
        self.reach = self.reach.max(width(b));
        let at = corner(b);
        let old = std::mem::replace(&mut self.cell_of[i], at);
        if old == at {
            return;
        }
        if let Some(list) = self.cells.get_mut(&old)
            && let Some(k) = list.iter().position(|&j| j == i as u32)
        {
            list.swap_remove(k);
            if let Some(n) = self.blocks.get_mut(&block_of(old)) {
                *n -= 1;
                if *n == 0 {
                    self.blocks.remove(&block_of(old));
                }
            }
        }
        self.cells.entry(at).or_default().push(i as u32);
        *self.blocks.entry(block_of(at)).or_default() += 1;
    }
    /// The indices, ascending, of every entry whose box can meet `area`
    /// (a superset: callers still test it). False when the area spans more
    /// columns than the grid holds (or is not finite): scan them all.
    pub fn candidates(&self, area: Aabb, out: &mut Vec<u32>) -> bool {
        out.clear();
        // A box meets `area` only if its minimum corner is below area.max
        // and within `reach` (plus a margin for rounding) of area.min.
        let margin = self.reach + 1.0;
        let lo = column(area.min(Axis::X) - margin, area.min(Axis::Z) - margin);
        let hi = column(area.max(Axis::X), area.max(Axis::Z));
        let span = |a: i32, b: i32| i64::from(b) - i64::from(a) + 1;
        let (nx, nz) = (span(lo.0, hi.0), span(lo.1, hi.1));
        if nx < 1 || nz < 1 || nx.saturating_mul(nz) > self.cells.len() as i64 {
            return false;
        }
        let (block_lo, block_hi) = (block_of(lo), block_of(hi));
        for bx in block_lo.0..=block_hi.0 {
            for bz in block_lo.1..=block_hi.1 {
                if !self.blocks.contains_key(&(bx, bz)) {
                    continue;
                }
                let xs = lo.0.max(bx << BLOCK_SHIFT)..=hi.0.min(((bx + 1) << BLOCK_SHIFT) - 1);
                for x in xs {
                    let zs = lo.1.max(bz << BLOCK_SHIFT)..=hi.1.min(((bz + 1) << BLOCK_SHIFT) - 1);
                    for z in zs {
                        if let Some(list) = self.cells.get(&(x, z)) {
                            out.extend_from_slice(list);
                        }
                    }
                }
            }
        }
        out.sort_unstable();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_core::geometry::Vec3;

    fn boxed(x: f64, y: f64, z: f64, w: f64) -> Aabb {
        Aabb::new(Vec3::new(x, y, z), Vec3::new(x + w, y + 1.8, z + w))
    }

    #[test]
    fn candidates_hold_every_meeting_box_in_order() {
        // A small LCG keeps the test free of dependencies.
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = |range: f64| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            (seed >> 11) as f64 / (1u64 << 53) as f64 * range
        };
        let mut boxes: Vec<Aabb> = (0..300)
            .map(|_| {
                boxed(
                    next(64.0) - 32.0,
                    next(8.0),
                    next(64.0) - 32.0,
                    0.3 + next(1.2),
                )
            })
            .collect();
        let mut grid = SeenGrid::new(boxes.iter().copied());
        let mut out = Vec::new();
        let mut indexed = 0;
        for round in 0..2000 {
            // Move one box, sometimes far and wider, as mobs do between queries.
            let i = round % boxes.len();
            let w = if round % 97 == 0 { 3.9 } else { 0.6 };
            boxes[i] = boxed(next(80.0) - 40.0, next(8.0), next(80.0) - 40.0, w);
            grid.update(i, boxes[i]);
            let area = boxed(next(70.0) - 35.0, next(8.0), next(70.0) - 35.0, next(2.0));
            let expected: Vec<u32> = (0..boxes.len() as u32)
                .filter(|&j| boxes[j as usize].intersects(area))
                .collect();
            if grid.candidates(area, &mut out) {
                indexed += 1;
                assert!(out.windows(2).all(|p| p[0] < p[1]), "ascending, no repeats");
                let met: Vec<u32> = out
                    .iter()
                    .copied()
                    .filter(|&j| boxes[j as usize].intersects(area))
                    .collect();
                assert_eq!(met, expected);
            }
        }
        assert!(indexed > 1900, "the grid answered {indexed} of 2000");
    }

    #[test]
    fn unbounded_areas_fall_back_to_a_full_scan() {
        let grid = SeenGrid::new([boxed(0.0, 0.0, 0.0, 0.6)].into_iter());
        let mut out = Vec::new();
        let huge = Aabb::new(
            Vec3::new(f64::NEG_INFINITY, 0.0, 0.0),
            Vec3::new(f64::INFINITY, 1.0, 1.0),
        );
        assert!(!grid.candidates(huge, &mut out));
    }
}
