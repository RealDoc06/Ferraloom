//! The tick's mob snapshot indexed by where the boxes are, so a box query
//! (`LivingEntity.pushEntities`) reads the mobs near it instead of every
//! mob, and still meets them in snapshot order. It can also keep which
//! species the snapshot holds, so a search for absent species ends at once.
use super::Species;
use crate::fast_hash::FxHashMap;
use horizon_core::geometry::{Aabb, Axis};

/// Width of a grid column, in blocks.
const CELL: f64 = 4.0;

/// Columns per side of a block of columns, as a shift.
const BLOCK_SHIFT: i32 = 2;

/// A block of columns: the last entry added to each, or [`END`], and how
/// many entries the block holds (a wide query passes an empty block with
/// one lookup instead of one per column).
#[derive(Clone, Copy)]
struct Block {
    heads: [u32; 1 << (2 * BLOCK_SHIFT)],
    count: u32,
}

/// Snapshot indices by the column holding each box's minimum corner, each
/// column's as a list linked through `next` (no allocation per column).
#[derive(Clone, Default)]
pub struct SeenGrid {
    /// Where in `slabs` each block that has held an entry is.
    blocks: FxHashMap<(i32, i32), u32>,
    slabs: Vec<Block>,
    /// The entry added before each one to its column, or [`END`].
    next: Vec<u32>,
    cell_of: Vec<(i32, i32)>,
    /// How many columns hold an entry.
    columns: usize,
    /// The widest box (along x or z) the grid has held: no box reaches
    /// further than this past its minimum corner.
    reach: f64,
    /// Every species an entry has had, when kept ([`SeenGrid::keep_species`]).
    species: Option<SpeciesSet>,
    /// How many times [`SeenGrid::note_species`] added a species.
    species_added: u32,
}
const END: u32 = u32::MAX;

fn column(x: f64, z: f64) -> (i32, i32) {
    ((x / CELL).floor() as i32, (z / CELL).floor() as i32)
}
fn corner(b: Aabb) -> (i32, i32) {
    column(b.min(Axis::X), b.min(Axis::Z))
}
fn block_of((x, z): (i32, i32)) -> (i32, i32) {
    (x >> BLOCK_SHIFT, z >> BLOCK_SHIFT)
}
/// A column's place among its block's `heads`.
fn cell_in((x, z): (i32, i32)) -> usize {
    let mask = (1 << BLOCK_SHIFT) - 1;
    (((x & mask) << BLOCK_SHIFT) | (z & mask)) as usize
}
fn width(b: Aabb) -> f64 {
    (b.max(Axis::X) - b.min(Axis::X)).max(b.max(Axis::Z) - b.min(Axis::Z))
}

impl SeenGrid {
    /// The boxes of the snapshot's entries, in order.
    pub fn new(boxes: impl ExactSizeIterator<Item = Aabb>) -> Self {
        let n = boxes.len();
        let mut grid = Self {
            blocks: FxHashMap::with_capacity_and_hasher(n / 2, Default::default()),
            slabs: Vec::with_capacity(n / 2),
            next: Vec::with_capacity(n),
            cell_of: Vec::with_capacity(n),
            columns: 0,
            reach: 0.0,
            species: None,
            species_added: 0,
        };
        for (i, b) in boxes.enumerate() {
            let at = corner(b);
            grid.reach = grid.reach.max(width(b));
            grid.next.push(END);
            grid.cell_of.push(at);
            grid.link(i, at);
        }
        grid
    }
    /// Keep the species of the entries from now on, starting with these:
    /// then [`SeenGrid::may_hold`] can answer no.
    pub fn keep_species(&mut self, species: impl Iterator<Item = Species>) {
        let set = self.species.get_or_insert_default();
        species.for_each(|s| {
            set.insert(s);
        });
    }
    /// An entry is now of species `s` (kept only while the grid keeps them).
    pub fn note_species(&mut self, s: Species) {
        if let Some(set) = &mut self.species
            && set.insert(s)
        {
            self.species_added += 1;
        }
    }
    /// Changes whenever a species is added: [`SeenGrid::may_hold`]'s
    /// answers stand while it is the same.
    pub fn species_added(&self) -> u32 {
        self.species_added
    }
    /// False only when the grid keeps species and no entry has ever been
    /// of one of `list`.
    pub fn may_hold(&self, list: &[Species]) -> bool {
        self.species
            .as_ref()
            .is_none_or(|set| list.iter().any(|&s| set.has(s)))
    }
    /// Put entry `i` at the head of column `at`.
    fn link(&mut self, i: usize, at: (i32, i32)) {
        let fresh = self.slabs.len() as u32;
        let slot = *self.blocks.entry(block_of(at)).or_insert(fresh);
        if slot == fresh {
            self.slabs.push(Block {
                heads: [END; 1 << (2 * BLOCK_SHIFT)],
                count: 0,
            });
        }
        let block = &mut self.slabs[slot as usize];
        let head = &mut block.heads[cell_in(at)];
        if *head == END {
            self.columns += 1;
        }
        self.next[i] = std::mem::replace(head, i as u32);
        block.count += 1;
    }
    /// Take entry `i` out of column `at`.
    fn unlink(&mut self, i: usize, at: (i32, i32)) {
        let Some(&slot) = self.blocks.get(&block_of(at)) else {
            return;
        };
        let block = &mut self.slabs[slot as usize];
        let head = &mut block.heads[cell_in(at)];
        if *head == END {
            return;
        }
        if *head as usize == i {
            *head = self.next[i];
            if *head == END {
                self.columns -= 1;
            }
        } else {
            let mut k = *head as usize;
            loop {
                match self.next[k] {
                    END => return,
                    j if j as usize == i => {
                        self.next[k] = self.next[i];
                        break;
                    }
                    j => k = j as usize,
                }
            }
        }
        self.next[i] = END;
        block.count -= 1;
    }
    /// Entry `i`'s box is now `b` (the mob moved or resized).
    pub fn update(&mut self, i: usize, b: Aabb) {
        self.reach = self.reach.max(width(b));
        let at = corner(b);
        let old = std::mem::replace(&mut self.cell_of[i], at);
        if old == at {
            return;
        }
        self.unlink(i, old);
        self.link(i, at);
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
        if nx < 1 || nz < 1 || nx.saturating_mul(nz) > self.columns as i64 {
            return false;
        }
        let (block_lo, block_hi) = (block_of(lo), block_of(hi));
        for bx in block_lo.0..=block_hi.0 {
            for bz in block_lo.1..=block_hi.1 {
                let Some(&slot) = self.blocks.get(&(bx, bz)) else {
                    continue;
                };
                let block = &self.slabs[slot as usize];
                if block.count == 0 {
                    continue;
                }
                let xs = lo.0.max(bx << BLOCK_SHIFT)..=hi.0.min(((bx + 1) << BLOCK_SHIFT) - 1);
                for x in xs {
                    let zs = lo.1.max(bz << BLOCK_SHIFT)..=hi.1.min(((bz + 1) << BLOCK_SHIFT) - 1);
                    for z in zs {
                        let mut at = block.heads[cell_in((x, z))];
                        while at != END {
                            out.push(at);
                            at = self.next[at as usize];
                        }
                    }
                }
            }
        }
        out.sort_unstable();
        true
    }
}

/// The species among a list of mobs, as bits (all of them past the 128th).
#[derive(Clone, Debug, Default)]
pub struct SpeciesSet([u64; 2], bool);
impl SpeciesSet {
    /// Adds `s`; true when it was not in yet.
    pub fn insert(&mut self, s: Species) -> bool {
        let before = (self.0, self.1);
        match s as usize {
            i if i < 128 => self.0[i / 64] |= 1 << (i % 64),
            _ => self.1 = true,
        }
        before != (self.0, self.1)
    }
    pub fn has(&self, s: Species) -> bool {
        let i = s as usize;
        self.1 || i >= 128 || self.0[i / 64] & (1 << (i % 64)) != 0
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
    fn kept_species_answer_no_only_when_absent() {
        let mut grid = SeenGrid::new([boxed(0.0, 0.0, 0.0, 0.6)].into_iter());
        assert!(
            grid.may_hold(&[Species::Cat]),
            "not kept: anything may be there"
        );
        grid.keep_species([Species::Zombie].into_iter());
        assert!(!grid.may_hold(&[Species::Cat, Species::Ocelot]));
        grid.note_species(Species::Cat);
        assert!(grid.may_hold(&[Species::Ocelot, Species::Cat]));
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
