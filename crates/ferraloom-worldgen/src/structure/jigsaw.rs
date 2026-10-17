//! Port of JigsawPlacement. Collision spaces are an axis-aligned allowed box
//! minus occupied integer piece boxes, exactly the voxel CSG used by vanilla.
//! Indices replace shared MutableObject references; all spaces and RNGs belong
//! to one start, so independent starts may be generated concurrently.
use super::{
    BoundingBox, JigsawJunction, Projection,
    alias::PoolAliasLookup,
    pool::{PoolHolder, StructurePoolElement, StructurePools},
    processor::shuffle,
    template::{LiquidSettings, StructureTemplateManager},
};
use crate::{Error, Identifier, region::Heightmap};
use ferraloom_util::{Rotation, random::RandomSource};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

/// Port of PoolElementStructurePiece.
#[derive(Clone, Debug)]
pub struct PoolElementStructurePiece {
    pub element: Arc<StructurePoolElement>,
    pub position: [i32; 3],
    pub ground_level_delta: i32,
    pub rotation: Rotation,
    pub bounds: BoundingBox,
    pub liquid: LiquidSettings,
    pub junctions: Vec<JigsawJunction>,
}
impl PoolElementStructurePiece {
    fn moved(&mut self, delta: [i32; 3]) {
        self.position = add(self.position, delta);
        self.bounds = self.bounds.moved(delta);
    }
}
#[derive(Clone, Debug)]
pub struct JigsawStart {
    pub position: [i32; 3],
    pub pieces: Vec<PoolElementStructurePiece>,
}
/// Inputs to JigsawPlacement#addPieces. Dimension limits are inclusive, matching
/// LevelHeightAccessor#getMinY/getMaxY. None padding is the default ZERO object;
/// explicit zero padding still checks the center piece against dimension bounds.
#[derive(Clone, Debug)]
pub struct JigsawPlacement {
    pub start_pool: PoolHolder,
    pub start_jigsaw: Option<Identifier>,
    pub max_depth: i32,
    pub position: [i32; 3],
    pub expansion_hack: bool,
    pub project_start_to_heightmap: Option<Heightmap>,
    pub max_distance: [i32; 2],
    pub dimension_min_y: i32,
    pub dimension_max_y: i32,
    pub padding: Option<[i32; 2]>,
    pub liquid: LiquidSettings,
}
impl JigsawPlacement {
    pub fn add_pieces(
        &self,
        pools: &StructurePools,
        templates: &StructureTemplateManager,
        random: &mut impl RandomSource,
        aliases: &PoolAliasLookup,
        height: &mut impl FnMut(i32, i32, Heightmap) -> Result<i32, Error>,
        could_exist: &mut impl FnMut(i32, i32, i32, i32) -> Result<bool, Error>,
    ) -> Result<Option<JigsawStart>, Error> {
        let rotation = Rotation::VALUES[random.next_int_bound(4) as usize];
        let center_pool = match &self.start_pool {
            PoolHolder::Reference(name) => pools
                .pools
                .get(aliases.lookup(name))
                .unwrap_or(pools.resolve(&self.start_pool)?),
            PoolHolder::Direct(pool) => pool,
        };
        let element = center_pool.random_template(random);
        if matches!(element.as_ref(), StructurePoolElement::Empty) {
            return Ok(None);
        }
        let anchor = if let Some(name) = &self.start_jigsaw {
            let info = element
                .jigsaws(templates, self.position, rotation, random)?
                .into_iter()
                .find(|info| info.name.as_ref() == Some(name));
            let Some(info) = info else {
                return Ok(None);
            };
            info.position
        } else {
            self.position
        };
        let local_anchor = sub(anchor, self.position);
        let adjusted = sub(self.position, local_anchor);
        let bounds = element.bounding_box(templates, adjusted, rotation)?;
        let mut center = PoolElementStructurePiece {
            ground_level_delta: element.ground_level_delta(),
            element,
            position: adjusted,
            rotation,
            bounds,
            liquid: self.liquid,
            junctions: Vec::new(),
        };
        // Java truncates these integer averages toward zero for negative boxes.
        let x = (bounds.max[0] + bounds.min[0]) / 2;
        let z = (bounds.max[2] + bounds.min[2]) / 2;
        let bottom = if let Some(map) = self.project_start_to_heightmap {
            if !could_exist(x, z, self.dimension_min_y, self.dimension_max_y)? {
                return Ok(None);
            }
            self.position[1] + height(x, z, map)?
        } else {
            adjusted[1]
        };
        center.moved([0, bottom - (bounds.min[1] + center.ground_level_delta), 0]);
        if let Some([pad_bottom, pad_top]) = self.padding
            && (center.bounds.min[1] < self.dimension_min_y + pad_bottom
                || center.bounds.max[1] > self.dimension_max_y - pad_top)
        {
            return Ok(None);
        }
        let y = bottom + local_anchor[1];
        let mut result = JigsawStart {
            position: [x, y, z],
            pieces: Vec::new(),
        };
        if self.max_depth > 0 {
            let [pad_bottom, pad_top] = self.padding.unwrap_or([0; 2]);
            let allowed = FloatBox {
                min: [
                    f64::from(x - self.max_distance[0]),
                    f64::from((y - self.max_distance[1]).max(self.dimension_min_y + pad_bottom)),
                    f64::from(z - self.max_distance[0]),
                ],
                max: [
                    f64::from(x + self.max_distance[0] + 1),
                    f64::from(
                        (y + self.max_distance[1] + 1).min(self.dimension_max_y + 1 - pad_top),
                    ),
                    f64::from(z + self.max_distance[0] + 1),
                ],
            };
            let space = CollisionSpace {
                allowed,
                occupied: vec![center.bounds],
            };
            result.pieces.push(center);
            let mut placer = Placer {
                config: self,
                pools,
                templates,
                random,
                height,
                aliases,
                pieces: &mut result.pieces,
                spaces: vec![space],
                queue: SequencedPriorityQueue::default(),
            };
            placer.children(0, 0, 0)?;
            while let Some((index, free, depth)) = placer.queue.next() {
                placer.children(index, free, depth)?;
            }
        }
        Ok(Some(result))
    }
}
fn add(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    std::array::from_fn(|i| a[i].wrapping_add(b[i]))
}
fn sub(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    std::array::from_fn(|i| a[i].wrapping_sub(b[i]))
}
#[derive(Clone, Copy, Debug)]
struct FloatBox {
    min: [f64; 3],
    max: [f64; 3],
}
impl From<BoundingBox> for FloatBox {
    fn from(b: BoundingBox) -> Self {
        Self {
            min: b.min.map(f64::from),
            max: b.max.map(|v| f64::from(v) + 1.0),
        }
    }
}
#[derive(Debug)]
struct CollisionSpace {
    allowed: FloatBox,
    occupied: Vec<BoundingBox>,
}
impl CollisionSpace {
    fn fits(&self, bounds: BoundingBox) -> bool {
        let bounds = FloatBox::from(bounds);
        let min = bounds.min.map(|v| v + 0.25);
        let max = bounds.max.map(|v| v - 0.25);
        if (0..3).any(|i| min[i] < self.allowed.min[i] || max[i] > self.allowed.max[i]) {
            return false;
        }
        !self.occupied.iter().any(|b| {
            let b = FloatBox::from(*b);
            (0..3).all(|i| min[i] < b.max[i] && max[i] > b.min[i])
        })
    }
}
/// Port of SequencedPriorityIterator. FIFO within a priority, highest first.
/// The MIN_VALUE sentinel behavior is retained for custom connector priorities.
#[derive(Debug)]
struct SequencedPriorityQueue<T> {
    queues: BTreeMap<i32, VecDeque<T>>,
    highest: Option<i32>,
}
impl<T> Default for SequencedPriorityQueue<T> {
    fn default() -> Self {
        Self {
            queues: BTreeMap::new(),
            highest: None,
        }
    }
}
impl<T> SequencedPriorityQueue<T> {
    fn add(&mut self, value: T, priority: i32) {
        self.queues.entry(priority).or_default().push_back(value);
        if priority >= self.highest.unwrap_or(i32::MIN) {
            self.highest = Some(priority);
        }
    }
    fn next(&mut self) -> Option<T> {
        let priority = self.highest?;
        let queue = self.queues.get_mut(&priority)?;
        let value = queue.pop_front();
        if queue.is_empty() {
            self.highest = self
                .queues
                .iter()
                .rev()
                .find(|(priority, queue)| **priority > i32::MIN && !queue.is_empty())
                .map(|(&p, _)| p);
        }
        value
    }
}
struct Placer<'a, R, H> {
    config: &'a JigsawPlacement,
    pools: &'a StructurePools,
    templates: &'a StructureTemplateManager,
    random: &'a mut R,
    height: &'a mut H,
    aliases: &'a PoolAliasLookup,
    pieces: &'a mut Vec<PoolElementStructurePiece>,
    spaces: Vec<CollisionSpace>,
    queue: SequencedPriorityQueue<(usize, usize, i32)>,
}
impl<R: RandomSource, H: FnMut(i32, i32, Heightmap) -> Result<i32, Error>> Placer<'_, R, H> {
    fn children(
        &mut self,
        source_index: usize,
        context_free: usize,
        depth: i32,
    ) -> Result<(), Error> {
        let source = self.pieces[source_index].clone();
        let source_projection = source.element.projection();
        let source_rigid = source_projection == Projection::Rigid;
        let mut source_free = None;
        let source_y = source.bounds.min[1];
        'connector: for jigsaw in source.element.jigsaws(
            self.templates,
            source.position,
            source.rotation,
            self.random,
        )? {
            let step = jigsaw.front.step();
            let target_pos = add(jigsaw.position, [step.x, step.y, step.z]);
            let source_local_y = jigsaw.position[1] - source_y;
            let mut base_height = None;
            let pool_name = self.aliases.lookup(&jigsaw.pool);
            let Some(pool) = self.pools.pools.get(pool_name) else {
                continue;
            };
            if pool.size() == 0 && pool_name.to_string() != "minecraft:empty" {
                continue;
            }
            let fallback = self.pools.resolve(&pool.fallback)?;
            let empty_fallback = matches!(&pool.fallback,PoolHolder::Reference(id) if id.to_string()=="minecraft:empty");
            if fallback.size() == 0 && !empty_fallback {
                continue;
            }
            let free = if source.bounds.contains(target_pos) {
                *source_free.get_or_insert_with(|| {
                    let index = self.spaces.len();
                    self.spaces.push(CollisionSpace {
                        allowed: source.bounds.into(),
                        occupied: Vec::new(),
                    });
                    index
                })
            } else {
                context_free
            };
            let mut targets = Vec::new();
            if depth != self.config.max_depth {
                targets.extend(pool.shuffled_templates(self.random));
            }
            targets.extend(fallback.shuffled_templates(self.random));
            for target in targets {
                if matches!(target.as_ref(), StructurePoolElement::Empty) {
                    break;
                }
                let mut rotations = Rotation::VALUES;
                shuffle(&mut rotations, self.random);
                for rotation in rotations {
                    let connectors =
                        target.jigsaws(self.templates, [0; 3], rotation, self.random)?;
                    let hack_box = target.bounding_box(self.templates, [0; 3], rotation)?;
                    let mut expand_to = 0;
                    if self.config.expansion_hack && hack_box.span()[1] <= 16 {
                        for connector in &connectors {
                            let step = connector.front.step();
                            let neighbor = add(connector.position, [step.x, step.y, step.z]);
                            if hack_box.contains(neighbor)
                                && let Some(pool) =
                                    self.pools.pools.get(self.aliases.lookup(&connector.pool))
                            {
                                expand_to = expand_to.max(pool.max_size(self.templates)?).max(
                                    self.pools
                                        .resolve(&pool.fallback)?
                                        .max_size(self.templates)?,
                                );
                            }
                        }
                    }
                    for connector in connectors {
                        if !jigsaw.can_attach(&connector) {
                            continue;
                        }
                        let raw_position = sub(target_pos, connector.position);
                        let raw_bounds =
                            target.bounding_box(self.templates, raw_position, rotation)?;
                        let target_projection = target.projection();
                        let target_rigid = target_projection == Projection::Rigid;
                        let target_local_y = connector.position[1];
                        let delta_y = source_local_y - target_local_y + step.y;
                        let target_y = if source_rigid && target_rigid {
                            source_y + delta_y
                        } else {
                            if base_height.is_none() {
                                base_height = Some((self.height)(
                                    jigsaw.position[0],
                                    jigsaw.position[2],
                                    Heightmap::WorldSurface,
                                )?);
                            }
                            base_height.unwrap() - target_local_y
                        };
                        let y_offset = target_y - raw_bounds.min[1];
                        let mut target_bounds = raw_bounds.moved([0, y_offset, 0]);
                        let position = add(raw_position, [0, y_offset, 0]);
                        if expand_to > 0 {
                            let new_size =
                                (expand_to + 1).max(target_bounds.max[1] - target_bounds.min[1]);
                            target_bounds = target_bounds.encapsulating(BoundingBox::point([
                                target_bounds.min[0],
                                target_bounds.min[1] + new_size,
                                target_bounds.min[2],
                            ]));
                        }
                        if !self.spaces[free].fits(target_bounds) {
                            continue;
                        }
                        self.spaces[free].occupied.push(target_bounds);
                        let ground = if target_rigid {
                            source.ground_level_delta - delta_y
                        } else {
                            target.ground_level_delta()
                        };
                        let junction_y = if source_rigid {
                            source_y + source_local_y
                        } else if target_rigid {
                            target_y + target_local_y
                        } else {
                            if base_height.is_none() {
                                base_height = Some((self.height)(
                                    jigsaw.position[0],
                                    jigsaw.position[2],
                                    Heightmap::WorldSurface,
                                )?);
                            }
                            base_height.unwrap() + delta_y / 2
                        };
                        self.pieces[source_index].junctions.push(JigsawJunction {
                            source: [
                                target_pos[0],
                                junction_y - source_local_y + source.ground_level_delta,
                                target_pos[2],
                            ],
                            delta_y,
                            destination_projection: target_projection,
                        });
                        let child = PoolElementStructurePiece {
                            element: target.clone(),
                            position,
                            ground_level_delta: ground,
                            rotation,
                            bounds: target_bounds,
                            liquid: self.config.liquid,
                            junctions: vec![JigsawJunction {
                                source: [
                                    jigsaw.position[0],
                                    junction_y - target_local_y + ground,
                                    jigsaw.position[2],
                                ],
                                delta_y: -delta_y,
                                destination_projection: source_projection,
                            }],
                        };
                        let index = self.pieces.len();
                        self.pieces.push(child);
                        if depth < self.config.max_depth {
                            self.queue
                                .add((index, free, depth + 1), jigsaw.placement_priority);
                        }
                        continue 'connector;
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "jigsaw_tests.rs"]
mod tests;
