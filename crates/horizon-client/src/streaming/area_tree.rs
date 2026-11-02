//! Queries over the areas kept loaded around a server's players, in time
//! logarithmic in the players rather than a scan of all of them.

/// A centre chunk and a radius in chunks.
pub(super) type Area = ((i32, i32), i32);

/// A k-d tree over areas, laid out in place: a slice's middle element splits
/// it, on x and z by turns. Built once per change of the areas (players mostly
/// keep their chunk from one tick to the next) and asked many times per tick.
#[derive(Default)]
pub(super) struct AreaTree {
    nodes: Vec<Area>,
    max_radius: i32,
}

/// The coordinate a slice splits on at this depth of the tree.
fn axis(area: &Area, by_z: bool) -> i32 {
    if by_z { area.0.1 } else { area.0.0 }
}

fn build(nodes: &mut [Area], by_z: bool) {
    if nodes.len() < 2 {
        return;
    }
    let mid = nodes.len() / 2;
    nodes.select_nth_unstable_by_key(mid, |a| axis(a, by_z));
    let (low, high) = nodes.split_at_mut(mid);
    build(low, !by_z);
    build(&mut high[1..], !by_z);
}

fn nearest(nodes: &[Area], x: i64, z: i64, by_z: bool, best: &mut i64) {
    let Some(&node) = nodes.get(nodes.len() / 2) else {
        return;
    };
    let (dx, dz) = (x - i64::from(node.0.0), z - i64::from(node.0.1));
    *best = (*best).min(dx * dx + dz * dz);
    let delta = if by_z { dz } else { dx };
    let (low, high) = nodes.split_at(nodes.len() / 2);
    let (near, far) = if delta < 0 {
        (low, &high[1..])
    } else {
        (&high[1..], low)
    };
    nearest(near, x, z, !by_z, best);
    if delta * delta < *best {
        nearest(far, x, z, !by_z, best);
    }
}

fn within(nodes: &[Area], x: i64, z: i64, margin: i64, reach: i64, by_z: bool) -> bool {
    let Some(&node) = nodes.get(nodes.len() / 2) else {
        return false;
    };
    let (dx, dz) = (x - i64::from(node.0.0), z - i64::from(node.0.1));
    if dx.abs().max(dz.abs()) <= i64::from(node.1) + margin {
        return true;
    }
    let delta = if by_z { dz } else { dx };
    let (low, high) = nodes.split_at(nodes.len() / 2);
    let (near, far) = if delta < 0 {
        (low, &high[1..])
    } else {
        (&high[1..], low)
    };
    within(near, x, z, margin, reach, !by_z)
        || (delta.abs() <= reach && within(far, x, z, margin, reach, !by_z))
}

impl AreaTree {
    pub(super) fn new(areas: impl IntoIterator<Item = Area>) -> Self {
        let mut nodes: Vec<Area> = areas.into_iter().collect();
        let max_radius = nodes.iter().map(|a| a.1).max().unwrap_or(0);
        build(&mut nodes, false);
        Self { nodes, max_radius }
    }

    /// The least of `limit` and the squared distance to the nearest centre.
    pub(super) fn nearest_squared(&self, x: i32, z: i32, limit: i64) -> i64 {
        let mut best = limit;
        nearest(&self.nodes, i64::from(x), i64::from(z), false, &mut best);
        best
    }

    /// Whether (x, z) lies within `margin` (Chebyshev) of an area's square.
    pub(super) fn within(&self, x: i32, z: i32, margin: i32) -> bool {
        let margin = i64::from(margin);
        let reach = i64::from(self.max_radius) + margin;
        within(
            &self.nodes,
            i64::from(x),
            i64::from(z),
            margin,
            reach,
            false,
        )
    }
}

/// How far outside `area`'s square (x, z) lies, clamped to `0..=reach + 1`:
/// every margin up to `reach` answers the same for columns of equal level.
fn level(area: Area, x: i32, z: i32, reach: i32) -> i64 {
    let (dx, dz) = (
        (i64::from(x) - i64::from(area.0.0)).abs(),
        (i64::from(z) - i64::from(area.0.1)).abs(),
    );
    (dx.max(dz) - i64::from(area.1)).clamp(0, i64::from(reach) + 1)
}

/// Every column whose answer to "within `margin` of an area?" may differ
/// between the area lists `old` and `new`, for margins up to `reach`. Compared
/// position by position: if any area covers a column in one list but not the
/// other, the areas at some position differ on it, whichever way the list
/// reordered. Callers give areas of a sane radius (`ExtraCover::GRID_RADIUS`).
pub(super) fn changed_columns(
    old: &[Area],
    new: &[Area],
    reach: i32,
    mut changed: impl FnMut(i32, i32),
) {
    for i in 0..old.len().max(new.len()) {
        let (a, b) = (old.get(i).copied(), new.get(i).copied());
        if a == b {
            continue;
        }
        let at = |area: Option<Area>, x, z| {
            area.map_or(i64::from(reach) + 1, |area| level(area, x, z, reach))
        };
        for area in [a, b].into_iter().flatten() {
            let r = area.1 + reach;
            for z in area.0.1 - r..=area.0.1 + r {
                for x in area.0.0 - r..=area.0.0 + r {
                    if at(a, x, z) != at(b, x, z) {
                        changed(x, z);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fast_hash::FxHashSet;

    /// A small deterministic generator: the tests want many shapes, not a
    /// dependency on a random number crate.
    fn lcg(seed: &mut u64, range: i32) -> i32 {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((*seed >> 33) % (2 * range as u64 + 1)) as i32 - range
    }

    fn areas(seed: &mut u64, count: usize, span: i32) -> Vec<Area> {
        (0..count)
            .map(|_| {
                let radius = lcg(seed, 6).abs();
                ((lcg(seed, span), lcg(seed, span)), radius)
            })
            .collect()
    }

    #[test]
    fn tree_answers_as_a_scan_does() {
        let mut seed = 7;
        for (count, span) in [
            (0, 5),
            (1, 5),
            (2, 5),
            (7, 3),
            (64, 40),
            (500, 200),
            (500, 8),
        ] {
            let list = areas(&mut seed, count, span);
            let tree = AreaTree::new(list.iter().copied());
            for _ in 0..2000 {
                let (x, z) = (lcg(&mut seed, span + 20), lcg(&mut seed, span + 20));
                let limit = i64::from(lcg(&mut seed, 2000).abs());
                let scan = list
                    .iter()
                    .map(|&((cx, cz), _)| i64::from(x - cx).pow(2) + i64::from(z - cz).pow(2))
                    .fold(limit, i64::min);
                assert_eq!(tree.nearest_squared(x, z, limit), scan);
                for margin in 0..=5 {
                    let scan = list
                        .iter()
                        .any(|&(c, r)| (x - c.0).abs().max((z - c.1).abs()) <= r + margin);
                    assert_eq!(tree.within(x, z, margin), scan, "({x}, {z}) +{margin}");
                }
            }
        }
    }

    #[test]
    fn changed_columns_finds_every_flip_of_every_margin() {
        const REACH: i32 = 4;
        let mut seed = 11;
        for _ in 0..200 {
            let count = 1 + lcg(&mut seed, 6).unsigned_abs() as usize;
            let old = areas(&mut seed, count, 12);
            let mut new = old.clone();
            // A few of: a player crossing a chunk, teleporting, leaving, joining.
            for _ in 0..=lcg(&mut seed, 2).abs() {
                let i = lcg(&mut seed, 20).unsigned_abs() as usize % new.len().max(1);
                match lcg(&mut seed, 2) {
                    -2 | -1 if !new.is_empty() => new[i].0.0 += 1,
                    0 if !new.is_empty() => new[i].0 = (lcg(&mut seed, 30), lcg(&mut seed, 30)),
                    1 if !new.is_empty() => drop(new.remove(i)),
                    _ => new.push(((lcg(&mut seed, 12), lcg(&mut seed, 12)), 3)),
                }
            }
            let mut changed = FxHashSet::default();
            changed_columns(&old, &new, REACH, |x, z| {
                changed.insert((x, z));
            });
            let covered = |list: &[Area], x: i32, z: i32, m: i32| {
                list.iter()
                    .any(|&(c, r)| (x - c.0).abs().max((z - c.1).abs()) <= r + m)
            };
            for x in -55..=55 {
                for z in -55..=55 {
                    for m in 0..=REACH {
                        if covered(&old, x, z, m) != covered(&new, x, z, m) {
                            assert!(changed.contains(&(x, z)), "({x}, {z}) at margin {m}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_player_crossing_a_chunk_changes_few_columns() {
        let (old, new) = ([((0, 0), 11)], [((1, 0), 11)]);
        let mut changed = FxHashSet::default();
        changed_columns(&old, &new, 4, |x, z| {
            changed.insert((x, z));
        });
        assert!(changed.len() < 350, "{}", changed.len());
    }
}
