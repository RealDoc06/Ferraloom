//! Port of Climate's quantized parameter space and stable, 19-way RTree.
use crate::{Error, codec::*};
use serde_json::Value;
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Parameter {
    pub min: i64,
    pub max: i64,
}
impl Parameter {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        let bounded_float = |v: &Value| {
            let n = float(v)?;
            if (-2.0..=2.0).contains(&n) {
                Ok(n)
            } else {
                Err(Error("climate coordinate outside [-2,2]".into()))
            }
        };
        let (min, max) = if v.is_number() {
            let n = bounded_float(v)?;
            (n, n)
        } else if v.is_array() {
            let values = list(v)?;
            if values.len() != 2 {
                return Err(Error("interval needs exactly two coordinates".into()));
            }
            (bounded_float(&values[0])?, bounded_float(&values[1])?)
        } else {
            (
                bounded_float(field(v, "min")?)?,
                bounded_float(field(v, "max")?)?,
            )
        };
        if min.total_cmp(&max).is_gt() {
            return Err(Error("interval min exceeds max".into()));
        }
        Ok(Self {
            min: quantize(min),
            max: quantize(max),
        })
    }
    pub fn distance(self, target: i64) -> i64 {
        let above = target.wrapping_sub(self.max);
        let below = self.min.wrapping_sub(target);
        if above > 0 { above } else { below.max(0) }
    }
    fn span(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
    fn center(self) -> i64 {
        self.min.wrapping_add(self.max) / 2
    }
}
pub fn quantize(coordinate: f32) -> i64 {
    (coordinate * 10000.0) as i64
}
pub fn target(coordinates: [f32; 6]) -> [i64; 6] {
    coordinates.map(quantize)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParameterPoint {
    pub ranges: [Parameter; 6],
    pub offset: i64,
}
impl ParameterPoint {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        let names = [
            "temperature",
            "humidity",
            "continentalness",
            "erosion",
            "depth",
            "weirdness",
        ];
        let ranges = names
            .iter()
            .map(|name| Parameter::decode(field(v, name)?))
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .unwrap();
        let offset = float(field(v, "offset")?)?;
        if offset.total_cmp(&0.0).is_lt() || offset.total_cmp(&1.0).is_gt() {
            return Err(Error("climate offset outside [0,1]".into()));
        }
        Ok(Self {
            ranges,
            offset: quantize(offset),
        })
    }
    fn bounds(self) -> [Parameter; 7] {
        std::array::from_fn(|i| {
            if i == 6 {
                Parameter {
                    min: self.offset,
                    max: self.offset,
                }
            } else {
                self.ranges[i]
            }
        })
    }
    pub fn fitness(self, target: [i64; 6]) -> i64 {
        self.ranges
            .iter()
            .zip(target)
            .fold(self.offset.wrapping_mul(self.offset), |sum, (p, t)| {
                let d = p.distance(t);
                sum.wrapping_add(d.wrapping_mul(d))
            })
    }
}
#[derive(Clone, Debug)]
struct Node {
    bounds: [Parameter; 7],
    kind: NodeKind,
}
#[derive(Clone, Debug)]
enum NodeKind {
    Leaf(usize),
    Branch(Vec<Node>),
}
impl Node {
    fn branch(children: Vec<Node>) -> Self {
        let bounds = std::array::from_fn(|i| {
            children
                .iter()
                .skip(1)
                .fold(children[0].bounds[i], |p, node| p.span(node.bounds[i]))
        });
        Self {
            bounds,
            kind: NodeKind::Branch(children),
        }
    }
    fn distance(&self, target: &[i64; 7]) -> i64 {
        self.bounds.iter().zip(target).fold(0i64, |sum, (p, &t)| {
            let d = p.distance(t);
            sum.wrapping_add(d.wrapping_mul(d))
        })
    }
    fn build(mut children: Vec<Node>) -> Self {
        if children.len() == 1 {
            return children.remove(0);
        }
        if children.len() <= 19 {
            children.sort_by_key(|node| {
                node.bounds
                    .iter()
                    .map(|p| p.center().wrapping_abs())
                    .sum::<i64>()
            });
            return Self::branch(children);
        }
        let mut best_cost = i64::MAX;
        let mut best_axis = 0;
        let mut best = Vec::new();
        let bucket_size =
            19.0_f64.powf(((children.len() as f64 - 0.01).ln() / 19.0_f64.ln()).floor()) as usize;
        for axis in 0..7 {
            children.sort_by(|a, b| compare(a, b, axis, false));
            let buckets = children
                .chunks(bucket_size)
                .map(|chunk| Self::branch(chunk.to_vec()))
                .collect::<Vec<_>>();
            let cost = buckets
                .iter()
                .flat_map(|b| b.bounds)
                .map(|p| p.max.wrapping_sub(p.min).wrapping_abs())
                .sum();
            if cost < best_cost {
                best_cost = cost;
                best_axis = axis;
                best = buckets;
            }
        }
        best.sort_by(|a, b| compare(a, b, best_axis, true));
        Self::branch(
            best.into_iter()
                .map(|bucket| match bucket.kind {
                    NodeKind::Branch(children) => Self::build(children),
                    NodeKind::Leaf(_) => unreachable!(),
                })
                .collect(),
        )
    }
    fn search(
        &self,
        target: &[i64; 7],
        candidate: Option<usize>,
        values: &[(ParameterPoint, impl Sized)],
    ) -> usize {
        match &self.kind {
            NodeKind::Leaf(index) => *index,
            NodeKind::Branch(children) => {
                let mut closest = candidate;
                let mut distance = candidate.map_or(i64::MAX, |index| {
                    values[index].0.fitness(target[..6].try_into().unwrap())
                });
                for child in children {
                    let child_distance = child.distance(target);
                    if distance > child_distance {
                        let leaf = child.search(target, closest, values);
                        let leaf_distance = if matches!(child.kind, NodeKind::Leaf(_)) {
                            child_distance
                        } else {
                            values[leaf].0.fitness(target[..6].try_into().unwrap())
                        };
                        if distance > leaf_distance {
                            distance = leaf_distance;
                            closest = Some(leaf);
                        }
                    }
                }
                closest.expect("nonempty tree")
            }
        }
    }
}
fn compare(a: &Node, b: &Node, axis: usize, absolute: bool) -> Ordering {
    for offset in 0..7 {
        let i = (axis + offset) % 7;
        let center = |node: &Node| {
            let c = node.bounds[i].center();
            if absolute { c.wrapping_abs() } else { c }
        };
        let order = center(a).cmp(&center(b));
        if order != Ordering::Equal {
            return order;
        }
    }
    Ordering::Equal
}

/// The previous result affects ties. Keep one search context per biome resolver
/// rather than the JVM's global thread-local, so generation jobs remain isolated.
#[derive(Default, Debug)]
pub struct SearchContext {
    last: Option<usize>,
}
impl SearchContext {
    pub fn clear(&mut self) {
        self.last = None;
    }
}
#[derive(Clone, Debug)]
pub struct ParameterList<T> {
    values: Vec<(ParameterPoint, T)>,
    root: Node,
}
impl<T> ParameterList<T> {
    pub fn new(values: Vec<(ParameterPoint, T)>) -> Result<Self, Error> {
        if values.is_empty() {
            return Err(Error("parameter list cannot be empty".into()));
        }
        let children = values
            .iter()
            .enumerate()
            .map(|(index, (point, _))| Node {
                bounds: point.bounds(),
                kind: NodeKind::Leaf(index),
            })
            .collect();
        Ok(Self {
            values,
            root: Node::build(children),
        })
    }
    pub fn values(&self) -> &[(ParameterPoint, T)] {
        &self.values
    }
    pub fn find(&self, target: [i64; 6], context: &mut SearchContext) -> &T {
        let full = std::array::from_fn(|i| if i == 6 { 0 } else { target[i] });
        let index = self.root.search(&full, context.last, &self.values);
        context.last = Some(index);
        &self.values[index].1
    }
}
