use std::sync::Arc;

use super::{Coordinates, EPSILON};

/// The four vanilla IndexMerger algorithms. The selected algorithm determines
/// coordinate identity as well as which input cells each result cell reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexMergerKind {
    DiscreteCube,
    Identical,
    NonOverlapping,
    Indirect,
}

#[derive(Clone, Debug)]
pub struct IndexMerger {
    kind: IndexMergerKind,
    coords: Coordinates,
    indexes: Vec<[i32; 2]>,
}

impl IndexMerger {
    /// Port of Shapes#createIndexMerger, including its 256-cell cube budget.
    pub fn new(
        cost: usize,
        first: &Coordinates,
        second: &Coordinates,
        first_only: bool,
        second_only: bool,
    ) -> Self {
        let n = first.len() - 1;
        let m = second.len() - 1;
        if first.is_cube() && second.is_cube() {
            let gcd = gcd(n, m);
            let size = n * (m / gcd);
            if cost.saturating_mul(size) <= 256 {
                return Self {
                    kind: IndexMergerKind::DiscreteCube,
                    coords: Coordinates::cube(size),
                    indexes: (0..size)
                        .map(|i| [(i / (m / gcd)) as i32, (i / (n / gcd)) as i32])
                        .collect(),
                };
            }
        }
        if first.get(n) < second.get(0) - EPSILON {
            return Self::non_overlapping(first, second, false);
        }
        if second.get(m) < first.get(0) - EPSILON {
            return Self::non_overlapping(second, first, true);
        }
        // fastutil DoubleList.equals uses Double.doubleToLongBits, including -0.
        if n == m && (0..=n).all(|i| same_double(first.get(i), second.get(i))) {
            return Self {
                kind: IndexMergerKind::Identical,
                coords: first.clone(),
                indexes: (0..n).map(|i| [i as i32; 2]).collect(),
            };
        }
        Self::indirect(first, second, first_only, second_only)
    }

    fn non_overlapping(lower: &Coordinates, upper: &Coordinates, swap: bool) -> Self {
        let n = lower.len();
        let mut indexes: Vec<_> = (0..n)
            .map(|i| [i as i32, -1])
            .chain((0..upper.len() - 1).map(|i| [(n - 1) as i32, i as i32]))
            .collect();
        if swap {
            for pair in &mut indexes {
                pair.swap(0, 1);
            }
        }
        Self {
            kind: IndexMergerKind::NonOverlapping,
            coords: Coordinates::array(
                (0..n)
                    .map(|i| lower.get(i))
                    .chain((0..upper.len()).map(|i| upper.get(i)))
                    .collect::<Vec<_>>(),
            ),
            indexes,
        }
    }

    fn indirect(
        first: &Coordinates,
        second: &Coordinates,
        first_only: bool,
        second_only: bool,
    ) -> Self {
        let mut values = Vec::new();
        let mut indexes = Vec::new();
        let (mut i, mut j) = (0, 0);
        let mut last = f64::NAN;
        loop {
            let end_first = i >= first.len();
            let end_second = j >= second.len();
            if end_first && end_second {
                break;
            }
            let chose_first = !end_first && (end_second || first.get(i) < second.get(j) + EPSILON);
            let next;
            if chose_first {
                i += 1;
                if !first_only && (j == 0 || end_second) {
                    continue;
                }
                next = first.get(i - 1);
            } else {
                j += 1;
                if !second_only && (i == 0 || end_first) {
                    continue;
                }
                next = second.get(j - 1);
            }
            let pair = [i as i32 - 1, j as i32 - 1];
            if last >= next - EPSILON {
                *indexes.last_mut().expect("merged coordinate") = pair;
            } else {
                values.push(next);
                indexes.push(pair);
                last = next;
            }
        }
        indexes.pop();
        if values.len() <= 1 {
            values = vec![0.0];
        }
        Self {
            kind: IndexMergerKind::Indirect,
            coords: Coordinates::Array(Arc::from(values)),
            indexes,
        }
    }

    pub fn kind(&self) -> IndexMergerKind {
        self.kind
    }
    pub fn coordinates(&self) -> &Coordinates {
        &self.coords
    }
    pub fn indexes(&self) -> &[[i32; 2]] {
        &self.indexes
    }
}

fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn same_double(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || a.is_nan() && b.is_nan()
}
