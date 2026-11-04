//! One tick's players in a small k-d tree, so the per-chunk and per-mob player
//! queries of `NaturalSpawner`, `LocalMobCapCalculator` and `Mob.checkDespawn`
//! read the players near them instead of the whole level.
//!
//! The players must not move while an index is in use. Every query answers
//! exactly what a scan of the players in their original order would, ties
//! included: pruning only skips players a scan would have rejected.
use std::ops::ControlFlow;

/// `ChunkMap.playerIsCloseEnoughForSpawning`: squared blocks from a chunk's centre.
const CLOSE_FOR_SPAWNING_SQR: f64 = 16384.0;
/// Most points a leaf scans.
const LEAF: usize = 8;

/// A player in the index.
#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub position: [f64; 3],
    /// Where the player came in the list given to [`PlayerIndex::new`].
    pub slot: u32,
    label: u8,
}
impl Point {
    pub fn distance_sqr(&self, [x, y, z]: [f64; 3]) -> f64 {
        let p = self.position;
        (p[0] - x).powi(2) + (p[1] - y).powi(2) + (p[2] - z).powi(2)
    }
    fn horizontal_distance_sqr(&self, x: f64, z: f64) -> f64 {
        let p = self.position;
        (p[0] - x).powi(2) + (p[2] - z).powi(2)
    }
}

/// A subtree: the box around its points and the union of their labels. The
/// left child follows its parent; `right` is 0 in a leaf.
#[derive(Clone, Debug)]
struct Node {
    lo: [f64; 3],
    hi: [f64; 3],
    start: u32,
    end: u32,
    right: u32,
    label: u8,
}
impl Node {
    fn gap(&self, axis: usize, v: f64) -> f64 {
        // `max` ignores NaN: a query that is not a number prunes nothing.
        (self.lo[axis] - v).max(v - self.hi[axis]).max(0.0)
    }
    /// Squared distance to the box, which no point in it can round below.
    fn distance_sqr(&self, [x, y, z]: [f64; 3]) -> f64 {
        self.gap(0, x).powi(2) + self.gap(1, y).powi(2) + self.gap(2, z).powi(2)
    }
    fn horizontal_distance_sqr(&self, x: f64, z: f64) -> f64 {
        self.gap(0, x).powi(2) + self.gap(2, z).powi(2)
    }
}

/// The nearest player's slot, position and squared distance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NearestPlayer {
    pub slot: u32,
    pub position: [f64; 3],
    pub distance_sqr: f64,
}

/// [`PlayerIndex::nearest_against`]'s search: the best so far, and how close a
/// player must be to change what it answers.
struct Against {
    distance_sqr: f64,
    limit: f64,
    /// Where the player at `distance_sqr` is, once the search found one.
    position: Option<[f64; 3]>,
}
impl Against {
    /// How close a player must be to change the answer for one `d` away: past
    /// the next cutoff down, or past the last cutoff when `d` is beyond it.
    fn limit_below(d: f64, cutoffs: &[f64; 3]) -> f64 {
        cutoffs
            .iter()
            .rev()
            .find(|&&c| c < d)
            .map_or(f64::NEG_INFINITY, |&c| c)
    }
}

/// Built once per pass from a list of player positions; see the module docs.
#[derive(Clone, Debug, Default)]
pub struct PlayerIndex {
    points: Vec<Point>,
    nodes: Vec<Node>,
    /// A position is not finite, so boxes cannot bound it: one leaf holds the
    /// points in list order and every query scans.
    linear: bool,
}
impl PlayerIndex {
    pub fn new(positions: impl IntoIterator<Item = [f64; 3]>) -> Self {
        let mut points: Vec<Point> = positions
            .into_iter()
            .enumerate()
            .map(|(slot, position)| Point {
                position,
                slot: slot as u32,
                label: 0,
            })
            .collect();
        let linear = points
            .iter()
            .any(|p| !p.position.iter().all(|v| v.is_finite()));
        let mut nodes = Vec::with_capacity(points.len() / (LEAF / 2) + 1);
        if linear {
            nodes.push(Node {
                lo: [f64::NEG_INFINITY; 3],
                hi: [f64::INFINITY; 3],
                start: 0,
                end: points.len() as u32,
                right: 0,
                label: 0,
            });
        } else if !points.is_empty() {
            build(&mut points, 0, &mut nodes);
        }
        Self {
            points,
            nodes,
            linear,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
    pub fn len(&self) -> usize {
        self.points.len()
    }
    /// `EntityGetter.getNearestPlayer` over the list: the first of the
    /// nearest, as `d < distance` keeps it.
    pub fn nearest(&self, at: [f64; 3]) -> Option<NearestPlayer> {
        let found = |p: &Point| NearestPlayer {
            slot: p.slot,
            position: p.position,
            distance_sqr: p.distance_sqr(at),
        };
        if self.linear {
            let mut best: Option<NearestPlayer> = None;
            for p in &self.points {
                if best.is_none_or(|b| p.distance_sqr(at) < b.distance_sqr) {
                    best = Some(found(p));
                }
            }
            return best;
        }
        if !at.iter().all(|v| v.is_finite()) {
            // Every distance is NaN or infinite: none beats the first.
            return self.points.iter().min_by_key(|p| p.slot).map(found);
        }
        let first = self.points.first()?;
        let mut best = (f64::INFINITY, u32::MAX, first);
        self.nearest_in(0, at, &mut best);
        Some(found(best.2))
    }
    fn nearest_in<'s>(&'s self, at: usize, q: [f64; 3], best: &mut (f64, u32, &'s Point)) {
        let n = &self.nodes[at];
        if n.right == 0 {
            for p in &self.points[n.start as usize..n.end as usize] {
                let d = p.distance_sqr(q);
                if d < best.0 || d == best.0 && p.slot < best.1 {
                    *best = (d, p.slot, p);
                }
            }
            return;
        }
        let (left, right) = (at + 1, n.right as usize);
        let (dl, dr) = (
            self.nodes[left].distance_sqr(q),
            self.nodes[right].distance_sqr(q),
        );
        let (first, second) = if dl <= dr {
            ((left, dl), (right, dr))
        } else {
            ((right, dr), (left, dl))
        };
        // A box at exactly the best distance can hold an earlier player.
        if first.1 <= best.0 {
            self.nearest_in(first.0, q, best);
        }
        if second.1 <= best.0 {
            self.nearest_in(second.0, q, best);
        }
    }
    /// The squared distance of a player on the same side of each of `cutoffs`
    /// (ascending) as the nearest one, or infinity past the last: for callers
    /// that only compare it with them, which needs far fewer players measured.
    pub fn nearest_against(&self, at: [f64; 3], cutoffs: &[f64; 3]) -> f64 {
        if self.linear || !at.iter().all(|v| v.is_finite()) {
            return self.nearest(at).map_or(f64::INFINITY, |n| n.distance_sqr);
        }
        if self.points.is_empty() {
            return f64::INFINITY;
        }
        let mut found = Against {
            distance_sqr: f64::INFINITY,
            limit: cutoffs[2],
            position: None,
        };
        self.against_in(0, at, cutoffs, &mut found);
        found.distance_sqr
    }
    /// [`Self::nearest_against`] given the squared distance of some player
    /// already measured, which is what its search then has to beat.
    pub fn nearest_against_from(&self, seen: f64, at: [f64; 3], cutoffs: &[f64; 3]) -> f64 {
        self.nearest_point_against_from(seen, at, cutoffs).0
    }
    /// The same, with where the search found a player nearer than `seen`
    /// (`None` when it found none, or the index is linear).
    pub fn nearest_point_against_from(
        &self,
        seen: f64,
        at: [f64; 3],
        cutoffs: &[f64; 3],
    ) -> (f64, Option<[f64; 3]>) {
        if self.linear {
            return (self.nearest_against(at, cutoffs), None);
        }
        let mut found = Against {
            distance_sqr: seen,
            limit: Against::limit_below(seen, cutoffs),
            position: None,
        };
        if found.limit != f64::NEG_INFINITY {
            self.against_in(0, at, cutoffs, &mut found);
        }
        (found.distance_sqr, found.position)
    }
    fn against_leaf(&self, n: &Node, q: [f64; 3], cutoffs: &[f64; 3], found: &mut Against) {
        for p in &self.points[n.start as usize..n.end as usize] {
            let d = p.distance_sqr(q);
            if d <= found.limit {
                found.distance_sqr = d;
                found.limit = Against::limit_below(d, cutoffs);
                found.position = Some(p.position);
            }
        }
    }
    fn against_in(&self, at: usize, q: [f64; 3], cutoffs: &[f64; 3], found: &mut Against) {
        let n = &self.nodes[at];
        if n.right == 0 {
            self.against_leaf(n, q, cutoffs, found);
            return;
        }
        let (left, right) = (at + 1, n.right as usize);
        let (dl, dr) = (
            self.nodes[left].distance_sqr(q),
            self.nodes[right].distance_sqr(q),
        );
        let (first, second) = if dl <= dr {
            ((left, dl), (right, dr))
        } else {
            ((right, dr), (left, dl))
        };
        if first.1 <= found.limit {
            self.against_in(first.0, q, cutoffs, found);
        }
        if second.1 <= found.limit {
            self.against_in(second.0, q, cutoffs, found);
        }
    }
    /// Calls `f` on the players less than `radius_sqr` away horizontally (in
    /// no particular order) until it breaks.
    pub fn visit_within(
        &self,
        x: f64,
        z: f64,
        radius_sqr: f64,
        f: &mut impl FnMut(&Point) -> ControlFlow<()>,
    ) -> ControlFlow<()> {
        if self.nodes.is_empty() {
            return ControlFlow::Continue(());
        }
        self.visit_in(0, x, z, radius_sqr, f)
    }
    fn visit_in(
        &self,
        at: usize,
        x: f64,
        z: f64,
        radius_sqr: f64,
        f: &mut impl FnMut(&Point) -> ControlFlow<()>,
    ) -> ControlFlow<()> {
        let n = &self.nodes[at];
        if n.horizontal_distance_sqr(x, z) >= radius_sqr {
            return ControlFlow::Continue(());
        }
        if n.right == 0 {
            for p in &self.points[n.start as usize..n.end as usize] {
                if p.horizontal_distance_sqr(x, z) < radius_sqr {
                    f(p)?;
                }
            }
            return ControlFlow::Continue(());
        }
        self.visit_in(at + 1, x, z, radius_sqr, f)?;
        self.visit_in(n.right as usize, x, z, radius_sqr, f)
    }
    /// The slots `ChunkMap.getPlayersCloseForSpawning` lists for a chunk, in
    /// no particular order, when the index holds the non-spectators.
    pub fn for_each_close_for_spawning(&self, chunk: (i32, i32), mut f: impl FnMut(usize)) {
        let (x, z) = chunk_centre(chunk);
        let _ = self.visit_within(x, z, CLOSE_FOR_SPAWNING_SQR, &mut |p| {
            f(p.slot as usize);
            ControlFlow::Continue(())
        });
    }
    /// [`Self::for_each_close_for_spawning`], with the players themselves.
    pub fn for_each_point_close_for_spawning(&self, chunk: (i32, i32), mut f: impl FnMut(&Point)) {
        let (x, z) = chunk_centre(chunk);
        let _ = self.visit_within(x, z, CLOSE_FOR_SPAWNING_SQR, &mut |p| {
            f(p);
            ControlFlow::Continue(())
        });
    }
    /// Whether that list has anyone on it.
    pub fn any_close_for_spawning(&self, chunk: (i32, i32)) -> bool {
        let (x, z) = chunk_centre(chunk);
        self.visit_within(
            x,
            z,
            CLOSE_FOR_SPAWNING_SQR,
            &mut |_| ControlFlow::Break(()),
        )
        .is_break()
    }
    /// Gives every player the label `f` returns for their slot.
    pub fn set_labels(&mut self, f: impl Fn(usize) -> u8) {
        for p in &mut self.points {
            p.label = f(p.slot as usize);
        }
        // Children come after their parents.
        for at in (0..self.nodes.len()).rev() {
            let n = &self.nodes[at];
            let label = if n.right == 0 {
                self.points[n.start as usize..n.end as usize]
                    .iter()
                    .fold(0, |union, p| union | p.label)
            } else {
                self.nodes[at + 1].label | self.nodes[n.right as usize].label
            };
            self.nodes[at].label = label;
        }
    }
    /// The union of the labels of the chunk's close players, cut short once
    /// it holds all of `wanted` (bits outside `wanted` may or may not show).
    pub fn close_for_spawning_labels(&self, chunk: (i32, i32), wanted: u8) -> u8 {
        if self.nodes.is_empty() {
            return 0;
        }
        let (x, z) = chunk_centre(chunk);
        let mut union = 0;
        self.labels_in(0, x, z, wanted, &mut union);
        union
    }
    fn labels_in(&self, at: usize, x: f64, z: f64, wanted: u8, union: &mut u8) {
        let n = &self.nodes[at];
        if n.label & wanted & !*union == 0
            || n.horizontal_distance_sqr(x, z) >= CLOSE_FOR_SPAWNING_SQR
        {
            return;
        }
        if n.right == 0 {
            for p in &self.points[n.start as usize..n.end as usize] {
                if p.horizontal_distance_sqr(x, z) < CLOSE_FOR_SPAWNING_SQR {
                    *union |= p.label;
                }
            }
            return;
        }
        self.labels_in(at + 1, x, z, wanted, union);
        self.labels_in(n.right as usize, x, z, wanted, union);
    }
}

/// `ChunkMap.euclideanDistanceSquared`'s reference point.
/// `ChunkMap.playerIsCloseEnoughForSpawning` for a player at `position`.
pub fn close_for_spawning(chunk: (i32, i32), position: [f64; 3]) -> bool {
    let (x, z) = chunk_centre(chunk);
    (position[0] - x).powi(2) + (position[2] - z).powi(2) < CLOSE_FOR_SPAWNING_SQR
}
fn chunk_centre((cx, cz): (i32, i32)) -> (f64, f64) {
    (f64::from(cx * 16 + 8), f64::from(cz * 16 + 8))
}

/// Builds the subtree over `points`, which start at `start` in the whole
/// list, splitting the widest axis at the median. Returns its node.
fn build(points: &mut [Point], start: usize, nodes: &mut Vec<Node>) -> usize {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in points.iter() {
        for axis in 0..3 {
            lo[axis] = lo[axis].min(p.position[axis]);
            hi[axis] = hi[axis].max(p.position[axis]);
        }
    }
    let at = nodes.len();
    nodes.push(Node {
        lo,
        hi,
        start: start as u32,
        end: (start + points.len()) as u32,
        right: 0,
        label: 0,
    });
    if points.len() > LEAF {
        let extent = |axis: usize| hi[axis] - lo[axis];
        let axis = [1, 2].into_iter().fold(0, |widest, axis| {
            if extent(axis) > extent(widest) {
                axis
            } else {
                widest
            }
        });
        let mid = points.len() / 2;
        points.select_nth_unstable_by(mid, |a, b| a.position[axis].total_cmp(&b.position[axis]));
        let (left, right) = points.split_at_mut(mid);
        build(left, start, nodes);
        let right = build(right, start + mid, nodes);
        nodes[at].right = right as u32;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small LCG keeps the test free of dependencies.
    struct Draw(u64);
    impl Draw {
        fn next(&mut self, range: i32) -> i32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            ((self.0 >> 33) as i32).rem_euclid(range)
        }
    }

    /// The scan the index replaces: `d < distance`, first wins.
    fn scan_nearest(players: &[[f64; 3]], at: [f64; 3]) -> Option<(u32, f64)> {
        let mut best = None;
        let mut distance = -1.0;
        for (i, p) in players.iter().enumerate() {
            let d = (p[0] - at[0]).powi(2) + (p[1] - at[1]).powi(2) + (p[2] - at[2]).powi(2);
            if distance == -1.0 || d < distance {
                best = Some((i as u32, d));
                distance = d;
            }
        }
        best
    }

    fn scan_close(players: &[[f64; 3]], (cx, cz): (i32, i32)) -> Vec<usize> {
        let (x, z) = (f64::from(cx * 16 + 8), f64::from(cz * 16 + 8));
        (0..players.len())
            .filter(|&i| {
                let (dx, dz) = (x - players[i][0], z - players[i][2]);
                dx * dx + dz * dz < 16384.0
            })
            .collect()
    }

    /// Players on a coarse lattice, so equal distances and shared positions
    /// are common, spread as tightly or widely as the scenario asks.
    fn lattice(draw: &mut Draw, count: usize, spacing: i32) -> Vec<[f64; 3]> {
        (0..count)
            .map(|_| {
                [
                    f64::from((draw.next(33) - 16) * spacing),
                    f64::from(60 + draw.next(3) * 4),
                    f64::from((draw.next(33) - 16) * spacing),
                ]
            })
            .collect()
    }

    #[test]
    fn queries_match_the_scan_including_ties() {
        let mut draw = Draw(0x2545_f491_4f6c_dd1d);
        for count in [0, 1, 2, 8, 9, 40, 500] {
            for spacing in [1, 8, 64, 1024] {
                let players = lattice(&mut draw, count, spacing);
                let mut index = PlayerIndex::new(players.iter().copied());
                // A label per slot, some unlabelled, as a mob cap's open categories.
                let label = |slot: usize| [0u8, 1, 2, 4, 3][slot % 5];
                index.set_labels(label);
                for _ in 0..200 {
                    let reach = 20 * spacing;
                    let at = [
                        f64::from(draw.next(2 * reach) - reach),
                        f64::from(60 + draw.next(3) * 4),
                        f64::from(draw.next(2 * reach) - reach),
                    ];
                    let nearest = index.nearest(at);
                    assert_eq!(
                        nearest.map(|n| (n.slot, n.distance_sqr)),
                        scan_nearest(&players, at),
                        "{count} {spacing} {at:?}"
                    );
                    if let Some(n) = nearest {
                        assert_eq!(n.position, players[n.slot as usize]);
                        let cutoffs = [
                            f64::from(spacing * spacing),
                            f64::from(spacing * spacing * 9),
                            f64::from(spacing * spacing * 100),
                        ];
                        let d = index.nearest_against(at, &cutoffs);
                        for cutoff in cutoffs {
                            assert_eq!(d <= cutoff, n.distance_sqr <= cutoff, "{at:?} {cutoff}");
                        }
                    }

                    let chunk = (at[0] as i32 >> 4, at[2] as i32 >> 4);
                    let expected = scan_close(&players, chunk);
                    let mut close = Vec::new();
                    index.for_each_close_for_spawning(chunk, |slot| close.push(slot));
                    close.sort_unstable();
                    assert_eq!(close, expected);
                    assert_eq!(index.any_close_for_spawning(chunk), !expected.is_empty());
                    let union = expected.iter().fold(0, |u, &slot| u | label(slot));
                    assert_eq!(index.close_for_spawning_labels(chunk, 7), union);
                    // Cut short, it still answers every wanted bit.
                    assert_eq!(index.close_for_spawning_labels(chunk, 2) & 2, union & 2);
                }
            }
        }
    }

    #[test]
    fn positions_that_are_not_finite_answer_as_the_scan() {
        let players = [[f64::NAN, 64.0, 0.0], [0.0, 64.0, 0.0], [1.0, 64.0, 0.0]];
        let index = PlayerIndex::new(players);
        let nearest = index.nearest([0.0, 64.0, 0.0]).map(|n| n.slot);
        assert_eq!(
            nearest,
            scan_nearest(&players, [0.0, 64.0, 0.0]).map(|n| n.0)
        );
        let mut close = Vec::new();
        index.for_each_close_for_spawning((0, 0), |slot| close.push(slot));
        assert_eq!(close, scan_close(&players, (0, 0)));

        let finite = [[9.0, 64.0, 0.0], [0.0, 64.0, 0.0]];
        let index = PlayerIndex::new(finite);
        for at in [[f64::NAN, 64.0, 0.0], [f64::INFINITY, 64.0, 0.0]] {
            assert_eq!(
                index.nearest(at).map(|n| n.slot),
                scan_nearest(&finite, at).map(|n| n.0)
            );
        }
    }
}
