//! Vanilla 26.3 trees and huge plants. Block behavior stays on the game bridge.
use super::{Feature, FeatureContext, FeatureLoader, state_provider::BlockStateProvider};
use crate::{Error, codec::*, generator::is_air, placement::providers::IntProvider};
use ferraloom_data::{Block, BlockState, properties};
use ferraloom_util::{Direction, core::Axis, random::RandomSource};
use serde_json::Value;
use std::{collections::BTreeSet, sync::Arc};
mod decorators;
mod fallen;
mod foliage;
mod plants;
mod positions;
mod roots;
#[cfg(test)]
mod tests;
mod trunk;
type Pos = [i32; 3];
type Provider = Arc<BlockStateProvider>;
use crate::placement::predicate::{move_direction as step, offset as add};
use decorators::Decorator;
use foliage::FoliagePlacer;
use positions::Positions;
use roots::RootPlacer;
use trunk::TrunkPlacer;
const HORIZONTAL: [Direction; 4] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
];

pub fn register(loader: &mut FeatureLoader<'_>) -> Result<(), Error> {
    loader.register("tree".parse()?, |v, l| {
        Ok(Arc::new(TreeFeature::decode(v, l)?))
    })?;
    loader.register("fallen_tree".parse()?, |v, l| {
        Ok(Arc::new(fallen::FallenTree::decode(v, l)?))
    })?;
    loader.register("huge_fungus".parse()?, |v, l| {
        Ok(Arc::new(plants::HugeFungus::decode(v, l)?))
    })?;
    loader.register("huge_brown_mushroom".parse()?, |v, l| {
        Ok(Arc::new(plants::HugeMushroom::decode(v, l, false)?))
    })?;
    loader.register("huge_red_mushroom".parse()?, |v, l| {
        Ok(Arc::new(plants::HugeMushroom::decode(v, l, true)?))
    })?;
    loader.register("root_system".parse()?, |v, l| {
        Ok(Arc::new(roots::RootSystem::decode(v, l)?))
    })?;
    Ok(())
}
fn tag(l: &FeatureLoader<'_>, name: &str) -> Result<Vec<Block>, Error> {
    l.data
        .block_tags
        .get(&format!("minecraft:{name}").parse::<crate::Identifier>()?)
        .cloned()
        .ok_or_else(|| Error(format!("missing tree block tag {name}")))
}
fn int(v: &Value, name: &str, min: i32, max: i32) -> Result<i32, Error> {
    int_range(field(v, name)?, min, max)
}
fn provider_int(v: &Value, name: &str, min: i32, max: i32) -> Result<IntProvider, Error> {
    IntProvider::decode_bounded(field(v, name)?, min, max)
}
fn chance(v: &Value, name: &str) -> Result<f32, Error> {
    let n = float(field(v, name)?)?;
    if (0.0..=1.0).contains(&n) {
        Ok(n)
    } else {
        Err(Error(format!("{name} outside [0,1]")))
    }
}
fn horizontal(random: &mut impl RandomSource) -> Direction {
    HORIZONTAL[random.next_int_bound(4) as usize]
}
fn relative(p: Pos, d: Direction, n: i32) -> Pos {
    let q = step([0; 3], d);
    add(p, q.map(|x| x * n))
}
fn axis_state(s: BlockState, axis: Axis) -> BlockState {
    // RotatedPillarBlock.AXIS is the XYZ property. A horizontal-only portal
    // axis shares its name but is a different vanilla property.
    if s.with(properties::AXIS, properties::Axis::Y).is_none() {
        return s;
    }
    s.with(
        properties::AXIS,
        match axis {
            Axis::X => properties::Axis::X,
            Axis::Y => properties::Axis::Y,
            Axis::Z => properties::Axis::Z,
        },
    )
    .unwrap_or(s)
}
fn shuffle<T>(values: &mut [T], random: &mut impl RandomSource) {
    for i in (2..=values.len()).rev() {
        let j = random.next_int_bound(i as i32) as usize;
        values.swap(i - 1, j);
    }
}
fn manhattan(a: Pos, b: Pos) -> i32 {
    (0..3).map(|i| (a[i] - b[i]).abs()).sum()
}

/// Port of TwoLayersFeatureSize and ThreeLayersFeatureSize.
#[derive(Debug)]
struct FeatureSize {
    limit: i32,
    lower: i32,
    upper: i32,
    middle: Option<(i32, i32)>,
    min_clipped: Option<i32>,
}
impl FeatureSize {
    fn decode(v: &Value) -> Result<Self, Error> {
        let ty = kind(v)?;
        let three = match ty.to_string().as_str() {
            "minecraft:two_layers_feature_size" => false,
            "minecraft:three_layers_feature_size" => true,
            _ => return Err(Error(format!("unknown feature size {ty}"))),
        };
        Ok(Self {
            limit: optional(v, "limit", 1, |v| {
                int_range(v, 0, if three { 80 } else { 81 })
            })?,
            lower: optional(v, "lower_size", 0, |v| int_range(v, 0, 16))?,
            upper: optional(v, "upper_size", 1, |v| int_range(v, 0, 16))?,
            middle: if three {
                Some((
                    optional(v, "middle_size", 1, |v| int_range(v, 0, 16))?,
                    optional(v, "upper_limit", 1, |v| int_range(v, 0, 80))?,
                ))
            } else {
                None
            },
            min_clipped: v
                .get("min_clipped_height")
                .map(|v| int_range(v, 0, 80))
                .transpose()?,
        })
    }
    fn at(&self, height: i32, y: i32) -> i32 {
        if y < self.limit {
            self.lower
        } else if let Some((middle, limit)) = self.middle {
            if y >= height - limit {
                self.upper
            } else {
                middle
            }
        } else {
            self.upper
        }
    }
}
/// Port of TreeFeature, including clipping, decorators and leaf-distance propagation.
#[derive(Debug)]
struct TreeFeature {
    trunk_provider: Provider,
    trunk: TrunkPlacer,
    foliage_provider: Provider,
    foliage: FoliagePlacer,
    root: Option<RootPlacer>,
    size: FeatureSize,
    decorators: Vec<Decorator>,
    ignore_vines: bool,
    below: Provider,
    replaceable: Vec<Block>,
    leaves: Vec<Block>,
    logs: Vec<Block>,
    leaf_support: Vec<Block>,
}
#[derive(Default)]
struct TreePositions {
    roots: Positions,
    logs: Positions,
    leaves: Positions,
    decorations: Positions,
}
impl TreeFeature {
    fn decode(v: &Value, l: &mut FeatureLoader<'_>) -> Result<Self, Error> {
        Ok(Self {
            trunk_provider: l.state_provider(field(v, "trunk_provider")?)?,
            trunk: TrunkPlacer::decode(field(v, "trunk_placer")?, l)?,
            foliage_provider: l.state_provider(field(v, "foliage_provider")?)?,
            foliage: FoliagePlacer::decode(field(v, "foliage_placer")?)?,
            root: v
                .get("root_placer")
                .map(|v| RootPlacer::decode(v, l))
                .transpose()?,
            size: FeatureSize::decode(field(v, "minimum_size")?)?,
            decorators: Decorator::list(field(v, "decorators")?, l)?,
            ignore_vines: optional(v, "ignore_vines", false, boolean)?,
            below: l.state_provider(field(v, "below_trunk_provider")?)?,
            replaceable: tag(l, "replaceable_by_trees")?,
            leaves: tag(l, "leaves")?,
            logs: tag(l, "logs")?,
            leaf_support: tag(l, "prevents_nearby_leaf_decay")?,
        })
    }
    fn valid(&self, c: &FeatureContext<'_, '_>, p: Pos) -> Result<bool, Error> {
        let s = c.level.block(p)?;
        Ok(is_air(s) || self.replaceable.contains(&s.block()))
    }
    fn air_or_leaves(&self, c: &FeatureContext<'_, '_>, p: Pos) -> Result<bool, Error> {
        let s = c.level.block(p)?;
        Ok(is_air(s) || self.leaves.contains(&s.block()))
    }
    fn below(
        &self,
        c: &mut FeatureContext<'_, '_>,
        p: Pos,
        out: &mut Positions,
    ) -> Result<(), Error> {
        if let Some(s) = self.below.get_optional_state(c, p)? {
            put(c, p, s, out)?;
        }
        Ok(())
    }
    fn place_tree(
        &self,
        c: &mut FeatureContext<'_, '_>,
        out: &mut TreePositions,
    ) -> Result<bool, Error> {
        let height = self.trunk.height(&mut c.random()?);
        let foliage_height = self.foliage.height(&mut c.random()?, height);
        let radius = self
            .foliage
            .radius(&mut c.random()?, height - foliage_height);
        let origin = c.origin;
        let trunk_origin = if let Some(root) = &self.root {
            root.origin(origin, &mut c.random()?)
        } else {
            origin
        };
        if origin[1].min(trunk_origin[1]) < c.level.min_y() + 1
            || origin[1].max(trunk_origin[1]) + height + 1 > c.level.min_y() + c.level.height()
        {
            return Ok(false);
        }
        let mut clipped = height;
        'scan: for y in 0..=height + 1 {
            let r = self.size.at(height, y);
            for x in -r..=r {
                for z in -r..=r {
                    let p = add(trunk_origin, [x, y, z]);
                    if !self.trunk.free(self, c, p)?
                        || !self.ignore_vines && c.level.block(p)?.block() == Block::VINE
                    {
                        clipped = y - 2;
                        break 'scan;
                    }
                }
            }
        }
        if clipped < height && self.size.min_clipped.is_none_or(|min| clipped < min) {
            return Ok(false);
        }
        if let Some(root) = &self.root
            && !root.place(self, c, origin, trunk_origin, &mut out.roots)?
        {
            return Ok(false);
        }
        let attachments = self
            .trunk
            .place(self, c, trunk_origin, clipped, &mut out.logs)?;
        for attachment in attachments {
            self.foliage
                .place(self, c, attachment, foliage_height, radius, &mut out.leaves)?;
        }
        Ok(true)
    }
    fn finish(&self, c: &mut FeatureContext<'_, '_>, out: &TreePositions) -> Result<(), Error> {
        let all = out
            .logs
            .iter()
            .chain(out.leaves.iter())
            .chain(out.roots.iter())
            .chain(out.decorations.iter())
            .collect::<Vec<_>>();
        let min = std::array::from_fn::<_, 3, _>(|i| all.iter().map(|p| p[i]).min().unwrap());
        let max = std::array::from_fn::<_, 3, _>(|i| all.iter().map(|p| p[i]).max().unwrap());
        let inside = |p: Pos| (0..3).all(|i| p[i] >= min[i] && p[i] <= max[i]);
        let mut shape: BTreeSet<Pos> = out.roots.iter().chain(out.decorations.iter()).collect();
        let mut queues: [Positions; 7] = Default::default();
        for p in out.logs.iter() {
            queues[0].insert(p);
        }
        while let Some(mut distance) = (0..7).find(|&i| !queues[i].is_empty()) {
            let p = queues[distance].pop_first().unwrap();
            if !inside(p) {
                continue;
            }
            if distance != 0 {
                let s = c
                    .level
                    .block(p)?
                    .with_integer_property("distance", distance as i32)
                    .map_err(|e| Error(format!("{e:?}")))?;
                c.level.set_block(p, s, 19)?;
            }
            shape.insert(p);
            for dir in Direction::VALUES {
                let q = step(p, dir);
                if inside(q) && !shape.contains(&q) {
                    let s = c.level.block(q)?;
                    let d = optional_leaf_distance(s, &self.leaf_support).map(i32::from);
                    if let Some(d) = d {
                        let d = d.min(distance as i32 + 1);
                        if d < 7 {
                            queues[d as usize].insert(q);
                            // The cursor also changes for the remaining neighbors.
                            distance = distance.min(d as usize);
                        }
                    }
                }
            }
        }
        update_shape_at_edge(c, &shape, min, max)
    }
}
impl Feature for TreeFeature {
    fn tree_minimum_height(&self) -> Option<i32> {
        Some(self.trunk.base_height())
    }
    fn place(&self, c: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let mut out = TreePositions::default();
        if !self.place_tree(c, &mut out)? || out.logs.is_empty() && out.leaves.is_empty() {
            return Ok(false);
        }
        decorators::decorate(
            &self.decorators,
            c,
            &out.logs,
            &out.leaves,
            &out.roots,
            &mut out.decorations,
        )?;
        self.finish(c, &out)?;
        Ok(true)
    }
}
fn put(
    c: &mut FeatureContext<'_, '_>,
    p: Pos,
    s: BlockState,
    out: &mut Positions,
) -> Result<(), Error> {
    out.insert(p);
    c.level.set_block(p, s, 19)?;
    Ok(())
}
/// Port of DiscreteVoxelShape#forAllFaces and StructureTemplate#updateShapeAtEdge.
fn update_shape_at_edge(
    c: &mut FeatureContext<'_, '_>,
    shape: &BTreeSet<Pos>,
    min: Pos,
    max: Pos,
) -> Result<(), Error> {
    for axes in [[0, 1, 2], [2, 0, 1], [1, 2, 0]] {
        let [a, b, axis] = axes;
        for av in min[a]..=max[a] {
            for bv in min[b]..=max[b] {
                let mut last = false;
                for cv in min[axis]..=max[axis] + 1 {
                    let mut p = [0; 3];
                    p[a] = av;
                    p[b] = bv;
                    p[axis] = cv;
                    let full = cv <= max[axis] && shape.contains(&p);
                    let direction = if !last && full {
                        Some(Axis::VALUES[axis].negative())
                    } else if last && !full {
                        p[axis] -= 1;
                        Some(Axis::VALUES[axis].positive())
                    } else {
                        None
                    };
                    if let Some(d) = direction {
                        let q = step(p, d);
                        let old = c.level.block(p)?;
                        let neighbor = c.level.block(q)?;
                        let new = c.level.update_shape(p, d, q, neighbor)?;
                        if new != old {
                            c.level.set_block(p, new, 2)?;
                        }
                        let new_neighbor = c.level.update_shape(q, d.opposite(), p, new)?;
                        if new_neighbor != neighbor {
                            c.level.set_block(q, new_neighbor, 2)?;
                        }
                    }
                    last = full;
                }
            }
        }
    }
    Ok(())
}

/// Port of LeavesBlock#getOptionalDistanceAt. The scaffold property has the
/// same name but a different domain (0..=7), so it is not the leaf property.
fn optional_leaf_distance(state: BlockState, support: &[Block]) -> Option<u8> {
    if support.contains(&state.block()) {
        Some(0)
    } else if state.with(properties::DISTANCE, 0).is_some() {
        None
    } else {
        state.get(properties::DISTANCE)
    }
}
