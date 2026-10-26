use super::{check_neighbour_blocks, get_path_type_static};
use crate::{
    entity::ai::pathfinder::{
        Capabilities, MobAccess, NodeEvaluator, NodeId, PathData, PathType, Target,
        mob_access::{Context, containing},
    },
    level::{Level, LevelError, path_navigation_region::PathNavigationRegion},
};
use ferraloom_data::{Block, tags::FluidTags};
use ferraloom_util::{Aabb, Axis, BlockPos, Direction, Vec3};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum WalkKind {
    Walk,
    Fly,
    Amphibious { prefers_shallow: bool },
}

/// Port of WalkNodeEvaluator. The prepared frame borrows the canonical Mob
/// access and the level; it owns only native per-search caches and node handles.
pub struct WalkNodeEvaluator<'a, M> {
    pub(super) context: Context<'a, M>,
    pub(super) kind: WalkKind,
    types: HashMap<i64, PathType>,
    collisions: HashMap<[u64; 6], bool>,
    old_walkable: f32,
    old_water_border: f32,
}
impl<'a, M: MobAccess> WalkNodeEvaluator<'a, M> {
    pub fn new(region: &'a PathNavigationRegion, level: &'a mut Level, mob: &'a mut M) -> Self {
        Self::with_kind(region, level, mob, WalkKind::Walk)
    }
    pub(super) fn with_kind(
        region: &'a PathNavigationRegion,
        level: &'a mut Level,
        mob: &'a mut M,
        kind: WalkKind,
    ) -> Self {
        Self {
            context: Context::new(region, level, mob),
            kind,
            types: HashMap::new(),
            collisions: HashMap::new(),
            old_walkable: 0.0,
            old_water_border: 0.0,
        }
    }
    pub fn capabilities(&self) -> &Capabilities {
        &self.context.capabilities
    }
    pub fn capabilities_mut(&mut self) -> &mut Capabilities {
        &mut self.context.capabilities
    }
    fn amphibious(&self) -> bool {
        matches!(self.kind, WalkKind::Amphibious { .. })
    }
    /// Port of WalkNodeEvaluator#getPathType, with the two subclass projections.
    pub fn get_path_type(&mut self, pos: BlockPos) -> Result<PathType, LevelError> {
        if self.kind == WalkKind::Fly {
            return self.fly_path_type(pos);
        }
        if self.amphibious() && self.context.raw_type(pos)? == PathType::Water {
            for direction in Direction::VALUES {
                if self.context.raw_type(pos.relative(direction, 1))? == PathType::Blocked {
                    return Ok(PathType::WaterBorder);
                }
            }
            return Ok(PathType::Water);
        }
        get_path_type_static(pos, self.context.region.min_y(), |p| {
            self.context.raw_type(p)
        })
    }
    /// Port of WalkNodeEvaluator#getPathTypeWithinMobBB. EnumSet traversal is
    /// represented by a bit per native ordinal, never a hash-set traversal.
    pub fn get_path_type_within_mob_bb(&mut self, pos: BlockPos) -> Result<u32, LevelError> {
        let mut types = 0;
        for dx in 0..self.context.width {
            for dy in 0..self.context.height {
                for dz in 0..self.context.depth {
                    let mut path_type = self.get_path_type(pos.offset(dx, dy, dz))?;
                    let caps = self.context.capabilities;
                    if path_type == PathType::DoorWoodClosed
                        && caps.can_open_doors
                        && caps.can_pass_doors
                    {
                        path_type = PathType::WalkableDoor;
                    }
                    if path_type == PathType::DoorOpen && !caps.can_pass_doors {
                        path_type = PathType::Blocked;
                    }
                    let mob_pos = containing(self.context.mob.position(self.context.level)?);
                    if path_type == PathType::Rail
                        && self.get_path_type(mob_pos)? != PathType::Rail
                        && self.get_path_type(mob_pos.below(1))? != PathType::Rail
                    {
                        path_type = PathType::UnpassableRail;
                    }
                    types |= 1 << path_type.id();
                }
            }
        }
        Ok(types)
    }
    /// Port of WalkNodeEvaluator#getPathTypeOfMob, including the large-mob cap.
    pub fn get_path_type_of_mob(&mut self, pos: BlockPos) -> Result<PathType, LevelError> {
        let types = self.get_path_type_within_mob_bb(pos)?;
        if types.count_ones() == 1 {
            return Ok(PathType::by_id(types.trailing_zeros() as i32));
        }
        for fixed in [PathType::Fence, PathType::UnpassableRail] {
            if types & (1 << fixed.id()) != 0 {
                return Ok(fixed);
            }
        }
        let mut highest = PathType::Blocked;
        let mut cost = self.context.malus(highest)?;
        for path_type in PathType::ALL {
            if types & (1 << path_type.id()) != 0 {
                let malus = self.context.malus(path_type)?;
                if malus < 0.0 {
                    return Ok(path_type);
                }
                if malus >= cost {
                    cost = malus;
                    highest = path_type;
                }
            }
        }
        let current = self.get_path_type(pos)?;
        Ok(if self.context.width > 1 {
            if self.context.malus(current)? < cost
                && self.context.malus(PathType::BigMobsCloseToDanger)? < cost
            {
                PathType::BigMobsCloseToDanger
            } else {
                highest
            }
        } else if current == PathType::Open && highest != PathType::Open && cost == 0.0 {
            PathType::Open
        } else {
            highest
        })
    }
    pub(super) fn cached_type(&mut self, pos: BlockPos) -> Result<PathType, LevelError> {
        if let Some(value) = self.types.get(&pos.as_long()) {
            return Ok(*value);
        }
        let value = self.get_path_type_of_mob(pos)?;
        self.types.insert(pos.as_long(), value);
        Ok(value)
    }
    pub(super) fn start_node(
        &mut self,
        pos: BlockPos,
        data: &mut PathData,
    ) -> Result<NodeId, LevelError> {
        let node = self.context.node(pos, data);
        // getNode may alias coordinates through its int hash; native classifies
        // the returned node's coordinates, not the requested coordinates.
        data[node].path_type = self.cached_type(data[node].pos)?;
        data[node].cost_malus = self.context.malus(data[node].path_type)?;
        Ok(node)
    }
    pub(super) fn can_start_at(&mut self, pos: BlockPos) -> Result<bool, LevelError> {
        let path_type = self.cached_type(pos)?;
        Ok((self.kind == WalkKind::Fly || path_type != PathType::Open)
            && self.context.malus(path_type)? >= 0.0)
    }
    fn walk_start(&mut self, data: &mut PathData) -> Result<NodeId, LevelError> {
        let position = self.context.mob.position(self.context.level)?;
        let mut y = position.y.floor() as i32;
        let mut state = self
            .context
            .state(containing(Vec3::new(position.x, y as f64, position.z)))?;
        if !self
            .context
            .mob
            .can_stand_on_fluid(state.data().fluid, self.context.level)?
        {
            if self.context.capabilities.can_float
                && self.context.mob.is_in_floatable_fluid(self.context.level)?
            {
                while self
                    .context
                    .reader()?
                    .fluid_tags()?
                    .contains_key(FluidTags::ENTITY_FLOATABLE, state.data().fluid.fluid_type)
                {
                    y = y.wrapping_add(1);
                    state = self
                        .context
                        .state(containing(Vec3::new(position.x, y as f64, position.z)))?;
                }
                y = y.wrapping_sub(1);
            } else if self.context.mob.on_ground(self.context.level)? {
                y = (position.y + 0.5).floor() as i32;
            } else {
                let mut below = containing(Vec3::new(position.x, position.y + 1.0, position.z));
                while below.y > self.context.region.min_y() {
                    y = below.y;
                    below = below.below(1);
                    let state = self.context.state(below)?;
                    if !state.is_air()
                        && !self
                            .context
                            .reader()?
                            .is_pathfindable(state, super::PathComputationType::Land)?
                    {
                        break;
                    }
                }
            }
        } else {
            while self
                .context
                .mob
                .can_stand_on_fluid(state.data().fluid, self.context.level)?
            {
                y = y.wrapping_add(1);
                state = self
                    .context
                    .state(containing(Vec3::new(position.x, y as f64, position.z)))?;
            }
            y = y.wrapping_sub(1);
        }
        let start = containing(Vec3::new(position.x, y as f64, position.z));
        if !self.can_start_at(start)? {
            let bb = self.context.mob.bounding_box(self.context.level)?;
            for (x, z) in [
                (bb.min_x, bb.min_z),
                (bb.min_x, bb.max_z),
                (bb.max_x, bb.min_z),
                (bb.max_x, bb.max_z),
            ] {
                let pos = containing(Vec3::new(x, y as f64, z));
                if self.can_start_at(pos)? {
                    return self.start_node(pos, data);
                }
            }
        }
        self.start_node(start, data)
    }
    pub(super) fn floor_level(&self, pos: BlockPos) -> Result<f64, LevelError> {
        let reader = self.context.reader()?;
        if (self.context.capabilities.can_float || self.amphibious())
            && reader.fluid_tags()?.contains_key(
                FluidTags::ENTITY_FLOATABLE,
                reader.get_fluid_state(pos)?.fluid_type,
            )
        {
            return Ok(pos.y as f64 + 0.5);
        }
        let below = pos.below(1);
        let shape =
            reader.collision_shape_without_context(reader.get_block_state(below)?, below)?;
        Ok(below.y as f64
            + if shape.is_empty() {
                0.0
            } else {
                shape.max(Axis::Y)
            })
    }
    fn has_collisions(&mut self, bb: Aabb) -> Result<bool, LevelError> {
        let key = [
            bb.min_x.to_bits(),
            bb.min_y.to_bits(),
            bb.min_z.to_bits(),
            bb.max_x.to_bits(),
            bb.max_y.to_bits(),
            bb.max_z.to_bits(),
        ];
        if let Some(value) = self.collisions.get(&key) {
            return Ok(*value);
        }
        let value = !self.context.no_collision(bb)?;
        self.collisions.insert(key, value);
        Ok(value)
    }
    fn can_reach_without_collision(
        &mut self,
        node: NodeId,
        data: &PathData,
    ) -> Result<bool, LevelError> {
        let mut bb = self.context.mob.bounding_box(self.context.level)?;
        let position = self.context.mob.position(self.context.level)?;
        let to = data[node].pos;
        let delta = Vec3::new(
            to.x as f64 - position.x + bb.x_size() / 2.0,
            to.y as f64 - position.y + bb.y_size() / 2.0,
            to.z as f64 - position.z + bb.z_size() / 2.0,
        );
        let steps = (delta.length() / bb.size()).ceil() as i32;
        let delta = delta * (1.0f32 / steps as f32) as f64;
        for _ in 1..=steps {
            bb = bb.move_by(delta.x, delta.y, delta.z);
            if self.has_collisions(bb)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn update_node(
        &mut self,
        pos: BlockPos,
        path_type: PathType,
        cost: f32,
        data: &mut PathData,
    ) -> NodeId {
        let node = self.context.node(pos, data);
        data[node].path_type = path_type;
        data[node].cost_malus = java_max(data[node].cost_malus, cost);
        node
    }
    fn blocked_node(&mut self, pos: BlockPos, data: &mut PathData) -> NodeId {
        let node = self.context.node(pos, data);
        data[node].path_type = PathType::Blocked;
        data[node].cost_malus = -1.0;
        node
    }
    pub(super) fn accepted_node(
        &mut self,
        pos: BlockPos,
        jump: i32,
        height: f64,
        direction: Direction,
        current: PathType,
        data: &mut PathData,
    ) -> Result<Option<NodeId>, LevelError> {
        if self.floor_level(pos)? - height
            > ferraloom_util::java::max(
                1.125,
                self.context.mob.max_up_step(self.context.level)? as f64,
            )
        {
            return Ok(None);
        }
        let path_type = self.cached_type(pos)?;
        let cost = self.context.malus(path_type)?;
        let mut best = if cost >= 0.0 {
            Some(self.update_node(pos, path_type, cost, data))
        } else {
            None
        };
        if partial_collision(current)
            && let Some(node) = best
            && data[node].cost_malus >= 0.0
            && !self.can_reach_without_collision(node, data)?
        {
            best = None;
        }
        if path_type == PathType::Walkable || (self.amphibious() && path_type == PathType::Water) {
            return Ok(best);
        }
        if best.is_none_or(|id| data[id].cost_malus < 0.0)
            && jump > 0
            && (path_type != PathType::Fence || self.context.capabilities.can_walk_over_fences)
            && !matches!(
                path_type,
                PathType::UnpassableRail | PathType::Trapdoor | PathType::PowderSnow
            )
        {
            best = self.accepted_node(pos.above(1), jump - 1, height, direction, current, data)?;
            if let Some(node) = best {
                let (width, bb_height) = self.context.mob.dimensions(self.context.level)?;
                if width < 1.0
                    && matches!(data[node].path_type, PathType::Open | PathType::Walkable)
                {
                    let step = direction.step();
                    let x = pos.x as f64 - step.x as f64 + 0.5;
                    let z = pos.z as f64 - step.z as f64 + 0.5;
                    let half = width as f64 / 2.0;
                    let bb = Aabb::new(
                        x - half,
                        self.floor_level(containing(Vec3::new(x, pos.y as f64 + 1.0, z)))? + 0.001,
                        z - half,
                        x + half,
                        bb_height as f64 + self.floor_level(data[node].pos)? - 0.002,
                        z + half,
                    );
                    if self.has_collisions(bb)? {
                        best = None;
                    }
                }
            }
        } else if !self.amphibious()
            && path_type == PathType::Water
            && !self.context.capabilities.can_float
        {
            let mut below = pos.below(1);
            while below.y > self.context.region.min_y() {
                let path_type = self.cached_type(below)?;
                if path_type != PathType::Water {
                    return Ok(best);
                }
                best =
                    Some(self.update_node(below, path_type, self.context.malus(path_type)?, data));
                below = below.below(1);
            }
        } else if path_type == PathType::Open {
            let mut below = pos.below(1);
            while below.y >= self.context.region.min_y() {
                if pos.y.wrapping_sub(below.y)
                    > self.context.mob.max_fall_distance(self.context.level)?
                {
                    return Ok(Some(self.blocked_node(below, data)));
                }
                let path_type = self.cached_type(below)?;
                let cost = self.context.malus(path_type)?;
                if path_type != PathType::Open {
                    return Ok(Some(if cost >= 0.0 {
                        self.update_node(below, path_type, cost, data)
                    } else {
                        self.blocked_node(below, data)
                    }));
                }
                below = below.below(1);
            }
            best = Some(self.blocked_node(pos, data));
        } else if partial_collision(path_type) && best.is_none() {
            let node = self.context.node(pos, data);
            data[node].closed = true;
            data[node].path_type = path_type;
            data[node].cost_malus = path_type.malus();
            best = Some(node);
        }
        Ok(best)
    }
    pub(super) fn neighbor_valid(node: Option<NodeId>, current: NodeId, data: &PathData) -> bool {
        node.is_some_and(|n| {
            !data[n].closed && (data[n].cost_malus >= 0.0 || data[current].cost_malus < 0.0)
        })
    }
    fn diagonal_valid(
        &self,
        current: NodeId,
        a: Option<NodeId>,
        b: Option<NodeId>,
        data: &PathData,
    ) -> Result<bool, LevelError> {
        let (Some(a), Some(b)) = (a, b) else {
            return Ok(false);
        };
        let pos = &data[current];
        let a = &data[a];
        let b = &data[b];
        if a.pos.y > pos.pos.y
            || b.pos.y > pos.pos.y
            || a.path_type == PathType::WalkableDoor
            || b.path_type == PathType::WalkableDoor
        {
            return Ok(false);
        }
        let width = self.context.mob.dimensions(self.context.level)?.0;
        if width > 1.0 && (a.cost_malus > 0.0 || b.cost_malus > 0.0) {
            return Ok(false);
        }
        let posts = a.path_type == PathType::Fence
            && b.path_type == PathType::Fence
            && (width as f64) < 0.5;
        Ok((a.pos.y < pos.pos.y || a.cost_malus >= 0.0 || posts)
            && (b.pos.y < pos.pos.y || b.cost_malus >= 0.0 || posts))
    }
    pub(super) fn walk_neighbors(
        &mut self,
        current: NodeId,
        neighbors: &mut Vec<NodeId>,
        data: &mut PathData,
    ) -> Result<(), LevelError> {
        let pos = data[current].pos;
        let above = self.cached_type(pos.above(1))?;
        let current_type = self.cached_type(pos)?;
        let jump = if self.context.malus(above)? >= 0.0 && current_type != PathType::StickyHoney {
            java_max(1.0, self.context.mob.max_up_step(self.context.level)?).floor() as i32
        } else {
            0
        };
        let height = self.floor_level(pos)?;
        let mut sides = [None; 4];
        for (i, direction) in HORIZONTAL.iter().copied().enumerate() {
            let node = self.accepted_node(
                pos.relative(direction, 1),
                jump,
                height,
                direction,
                current_type,
                data,
            )?;
            sides[i] = node;
            if Self::neighbor_valid(node, current, data) {
                neighbors.push(node.unwrap());
            }
        }
        for (i, direction) in HORIZONTAL.iter().copied().enumerate() {
            if self.diagonal_valid(current, sides[i], sides[(i + 1) % 4], data)? {
                let node = self.accepted_node(
                    pos.relative(direction, 1)
                        .relative(direction.clockwise(), 1),
                    jump,
                    height,
                    direction,
                    current_type,
                    data,
                )?;
                if let Some(node) = node
                    && !data[node].closed
                    && data[node].path_type != PathType::WalkableDoor
                    && data[node].cost_malus >= 0.0
                {
                    neighbors.push(node);
                }
            }
        }
        if self.amphibious() {
            let up = self.accepted_node(
                pos.above(1),
                0.max(jump - 1),
                height,
                Direction::Up,
                current_type,
                data,
            )?;
            let down = self.accepted_node(
                pos.below(1),
                jump,
                height,
                Direction::Down,
                current_type,
                data,
            )?;
            for (node, allowed) in [(up, true), (down, current_type != PathType::Trapdoor)] {
                if allowed
                    && Self::neighbor_valid(node, current, data)
                    && let Some(node) = node
                    && data[node].path_type == PathType::Water
                {
                    neighbors.push(node);
                }
            }
            if self.kind
                == (WalkKind::Amphibious {
                    prefers_shallow: true,
                })
            {
                for &node in neighbors.iter() {
                    if data[node].path_type == PathType::Water
                        && data[node].pos.y < self.context.level.context().sea_level - 10
                    {
                        data[node].cost_malus += 1.0;
                    }
                }
            }
        }
        Ok(())
    }
}
impl<M: MobAccess> NodeEvaluator for WalkNodeEvaluator<'_, M> {
    type Error = LevelError;
    fn prepare(&mut self, _data: &mut PathData) -> Result<(), LevelError> {
        self.context.prepare()?;
        self.context.mob.on_pathfinding_start(self.context.level)?;
        if self.kind == WalkKind::Fly {
            self.types.clear();
            self.context.mob.on_pathfinding_start(self.context.level)?;
        }
        if self.amphibious() {
            self.context
                .mob
                .set_pathfinding_malus(PathType::Water, 0.0, self.context.level)?;
            self.old_walkable = self.context.malus(PathType::Walkable)?;
            self.context
                .mob
                .set_pathfinding_malus(PathType::Walkable, 6.0, self.context.level)?;
            self.old_water_border = self.context.malus(PathType::WaterBorder)?;
            self.context.mob.set_pathfinding_malus(
                PathType::WaterBorder,
                4.0,
                self.context.level,
            )?;
        }
        Ok(())
    }
    fn done(&mut self, _data: &mut PathData) -> Result<(), LevelError> {
        if self.amphibious() {
            self.context.mob.set_pathfinding_malus(
                PathType::Walkable,
                self.old_walkable,
                self.context.level,
            )?;
            self.context.mob.set_pathfinding_malus(
                PathType::WaterBorder,
                self.old_water_border,
                self.context.level,
            )?;
        }
        if self.kind == WalkKind::Fly {
            self.context.mob.on_pathfinding_done(self.context.level)?;
            self.types.clear();
        }
        self.context.mob.on_pathfinding_done(self.context.level)?;
        self.types.clear();
        self.collisions.clear();
        Ok(())
    }
    fn get_start(&mut self, data: &mut PathData) -> Result<Option<NodeId>, LevelError> {
        let node = if self.kind == WalkKind::Fly {
            self.fly_start(data)?
        } else if self.amphibious() && self.context.mob.is_in_water(self.context.level)? {
            let bb = self.context.mob.bounding_box(self.context.level)?;
            self.start_node(
                containing(Vec3::new(bb.min_x, bb.min_y + 0.5, bb.min_z)),
                data,
            )?
        } else {
            self.walk_start(data)?
        };
        Ok(Some(node))
    }
    fn get_target(&mut self, pos: BlockPos, data: &mut PathData) -> Result<Target, LevelError> {
        let node = self.context.node(pos, data);
        Ok(Target::new(node, data))
    }
    fn get_neighbors(
        &mut self,
        node: NodeId,
        neighbors: &mut Vec<NodeId>,
        data: &mut PathData,
    ) -> Result<(), LevelError> {
        if self.kind == WalkKind::Fly {
            self.fly_neighbors(node, neighbors, data)
        } else {
            self.walk_neighbors(node, neighbors, data)
        }
    }
}
pub(super) const HORIZONTAL: [Direction; 4] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
];
fn partial_collision(path_type: PathType) -> bool {
    matches!(
        path_type,
        PathType::Fence | PathType::DoorWoodClosed | PathType::DoorIronClosed
    )
}
pub(super) fn java_max(a: f32, b: f32) -> f32 {
    ferraloom_util::java::max(a as f64, b as f64) as f32
}

impl<M: MobAccess> WalkNodeEvaluator<'_, M> {
    /// Port of FlyNodeEvaluator#getPathType, including its mob-position fence exception.
    fn fly_path_type(&mut self, pos: BlockPos) -> Result<PathType, LevelError> {
        let mut path_type = self.context.raw_type(pos)?;
        if path_type == PathType::Open && pos.y >= self.context.region.min_y() + 1 {
            let below = pos.below(1);
            let below_type = self.context.raw_type(below)?;
            match below_type {
                PathType::Fire | PathType::Lava => path_type = PathType::Fire,
                PathType::Damaging => path_type = PathType::Damaging,
                PathType::Cocoa => path_type = PathType::Cocoa,
                PathType::Fence => {
                    if below != self.context.mob_position {
                        path_type = PathType::Fence;
                    }
                }
                _ => {
                    path_type = if !matches!(
                        below_type,
                        PathType::Walkable | PathType::Open | PathType::Water
                    ) {
                        PathType::Walkable
                    } else {
                        PathType::Open
                    }
                }
            }
        }
        if matches!(path_type, PathType::Walkable | PathType::Open) {
            path_type = check_neighbour_blocks(pos, path_type, |p| self.context.raw_type(p))?;
        }
        Ok(path_type)
    }
    /// Port of FlyNodeEvaluator#getStart. Random candidates are drawn lazily:
    /// finding an acceptable candidate stops the canonical RNG immediately.
    fn fly_start(&mut self, data: &mut PathData) -> Result<NodeId, LevelError> {
        let position = self.context.mob.position(self.context.level)?;
        let mut y;
        if self.context.capabilities.can_float
            && self.context.mob.is_in_water(self.context.level)?
        {
            y = position.y.floor() as i32;
            while self
                .context
                .state(containing(Vec3::new(position.x, y as f64, position.z)))?
                .block()
                == Block::WATER
            {
                y = y.wrapping_add(1);
            }
        } else {
            y = (position.y + 0.5).floor() as i32;
        }
        let start = containing(Vec3::new(position.x, y as f64, position.z));
        if !self.can_start_at(start)? {
            let bb = self.context.mob.bounding_box(self.context.level)?;
            if bb.size() < 1.0 {
                let bb = bb.inflate(
                    (1.100000023841858 - bb.x_size()).max(0.0),
                    (1.100000023841858 - bb.y_size()).max(0.0),
                    (1.100000023841858 - bb.z_size()).max(0.0),
                );
                let min = containing(bb.min_position());
                let max = containing(bb.max_position());
                for _ in 0..10 {
                    let x = min.x
                        + self
                            .context
                            .mob
                            .next_random_int(max.x - min.x + 1, self.context.level)?;
                    let y = min.y
                        + self
                            .context
                            .mob
                            .next_random_int(max.y - min.y + 1, self.context.level)?;
                    let z = min.z
                        + self
                            .context
                            .mob
                            .next_random_int(max.z - min.z + 1, self.context.level)?;
                    let candidate = BlockPos::new(x, y, z);
                    if self.can_start_at(candidate)? {
                        return self.start_node(candidate, data);
                    }
                }
            } else {
                for (x, z) in [
                    (bb.min_x, bb.min_z),
                    (bb.min_x, bb.max_z),
                    (bb.max_x, bb.min_z),
                    (bb.max_x, bb.max_z),
                ] {
                    let candidate = containing(Vec3::new(x, position.y.floor(), z));
                    if self.can_start_at(candidate)? {
                        return self.start_node(candidate, data);
                    }
                }
            }
        }
        self.start_node(start, data)
    }
    fn fly_accepted(
        &mut self,
        pos: BlockPos,
        data: &mut PathData,
    ) -> Result<Option<NodeId>, LevelError> {
        let path_type = self.cached_type(pos)?;
        let cost = self.context.malus(path_type)?;
        if cost < 0.0 || cost.is_nan() {
            return Ok(None);
        }
        let node = self.update_node(pos, path_type, cost, data);
        if path_type == PathType::Walkable {
            data[node].cost_malus += 1.0;
        }
        Ok(Some(node))
    }
    /// Port of FlyNodeEvaluator#getNeighbors. Ordering matches the native six
    /// faces, twelve edges and eight corners. Every edge/corner needs all of
    /// its proper nonempty axis subsets, including closed intermediate nodes.
    fn fly_neighbors(
        &mut self,
        current: NodeId,
        neighbors: &mut Vec<NodeId>,
        data: &mut PathData,
    ) -> Result<(), LevelError> {
        const OFFSETS: [(i32, i32, i32); 26] = [
            (0, 0, 1),
            (-1, 0, 0),
            (1, 0, 0),
            (0, 0, -1),
            (0, 1, 0),
            (0, -1, 0),
            (0, 1, 1),
            (-1, 1, 0),
            (1, 1, 0),
            (0, 1, -1),
            (0, -1, 1),
            (-1, -1, 0),
            (1, -1, 0),
            (0, -1, -1),
            (1, 0, -1),
            (1, 0, 1),
            (-1, 0, -1),
            (-1, 0, 1),
            (1, 1, -1),
            (1, 1, 1),
            (-1, 1, -1),
            (-1, 1, 1),
            (1, -1, -1),
            (1, -1, 1),
            (-1, -1, -1),
            (-1, -1, 1),
        ];
        let mut nodes = [None; 26];
        let pos = data[current].pos;
        for (i, &(x, y, z)) in OFFSETS.iter().enumerate() {
            let node = self.fly_accepted(pos.offset(x, y, z), data)?;
            nodes[i] = node;
            let Some(node) = node else { continue };
            if data[node].closed {
                continue;
            }
            let prerequisites = OFFSETS[..i].iter().enumerate().filter(|(_, &(a, b, c))| {
                (a == 0 || a == x)
                    && (b == 0 || b == y)
                    && (c == 0 || c == z)
                    && (a, b, c) != (x, y, z)
            });
            if prerequisites
                .into_iter()
                .all(|(index, _)| nodes[index].is_some_and(|id| data[id].cost_malus >= 0.0))
            {
                neighbors.push(node);
            }
        }
        Ok(())
    }
}
