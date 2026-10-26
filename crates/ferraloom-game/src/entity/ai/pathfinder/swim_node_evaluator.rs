use super::{
    Capabilities, MobAccess, NodeEvaluator, NodeId, PathComputationType, PathData, PathType,
    Target,
    mob_access::{Context, containing},
    walk_node_evaluator::prepared::{HORIZONTAL, java_max},
};
use crate::level::{Level, LevelError, path_navigation_region::PathNavigationRegion};
use ferraloom_data::tags::FluidTags;
use ferraloom_util::{BlockPos, Direction, Vec3};
use std::collections::HashMap;

/// Port of SwimNodeEvaluator. Breaching is a constructor capability, distinct
/// from the inherited float/door flags.
pub struct SwimNodeEvaluator<'a, M> {
    context: Context<'a, M>,
    allow_breaching: bool,
    types: HashMap<i64, PathType>,
}
impl<'a, M: MobAccess> SwimNodeEvaluator<'a, M> {
    pub fn new(
        region: &'a PathNavigationRegion,
        level: &'a mut Level,
        mob: &'a mut M,
        allow_breaching: bool,
    ) -> Self {
        Self {
            context: Context::new(region, level, mob),
            allow_breaching,
            types: HashMap::new(),
        }
    }
    pub fn capabilities(&self) -> &Capabilities {
        &self.context.capabilities
    }
    pub fn capabilities_mut(&mut self) -> &mut Capabilities {
        &mut self.context.capabilities
    }
    /// Port of SwimNodeEvaluator#getPathTypeOfMob. BREACH returns immediately
    /// on the first air cell above a WATER-pathfindable block, before later cells.
    pub fn get_path_type(&self, pos: BlockPos) -> Result<PathType, LevelError> {
        let reader = self.context.reader()?;
        let mut last = pos;
        for x in pos.x..pos.x + self.context.width {
            for y in pos.y..pos.y + self.context.height {
                for z in pos.z..pos.z + self.context.depth {
                    last = BlockPos::new(x, y, z);
                    let state = reader.get_block_state(last)?;
                    let below = reader.get_block_state(last.below(1))?;
                    if state.data().fluid.empty
                        && reader.is_pathfindable(below, PathComputationType::Water)?
                        && state.is_air()
                    {
                        return Ok(PathType::Breach);
                    }
                    if !reader
                        .fluid_tags()?
                        .contains_key(FluidTags::WATER, state.data().fluid.fluid_type)
                    {
                        return Ok(PathType::Blocked);
                    }
                }
            }
        }
        Ok(
            if reader.is_pathfindable(reader.get_block_state(last)?, PathComputationType::Water)? {
                PathType::Water
            } else {
                PathType::Blocked
            },
        )
    }
    fn cached_type(&mut self, pos: BlockPos) -> Result<PathType, LevelError> {
        if let Some(value) = self.types.get(&pos.as_long()) {
            return Ok(*value);
        }
        let value = self.get_path_type(pos)?;
        self.types.insert(pos.as_long(), value);
        Ok(value)
    }
    fn accepted_node(
        &mut self,
        pos: BlockPos,
        data: &mut PathData,
    ) -> Result<Option<NodeId>, LevelError> {
        let path_type = self.cached_type(pos)?;
        if !(self.allow_breaching && path_type == PathType::Breach || path_type == PathType::Water)
        {
            return Ok(None);
        }
        let cost = self.context.malus(path_type)?;
        if cost < 0.0 || cost.is_nan() {
            return Ok(None);
        }
        let node = self.context.node(pos, data);
        data[node].path_type = path_type;
        data[node].cost_malus = java_max(data[node].cost_malus, cost);
        if self.context.state(pos)?.data().fluid.empty {
            data[node].cost_malus += 8.0;
        }
        Ok(Some(node))
    }
}
impl<M: MobAccess> NodeEvaluator for SwimNodeEvaluator<'_, M> {
    type Error = LevelError;
    fn prepare(&mut self, _data: &mut PathData) -> Result<(), LevelError> {
        self.context.prepare()?;
        self.types.clear();
        Ok(())
    }
    fn done(&mut self, _data: &mut PathData) -> Result<(), LevelError> {
        self.types.clear();
        Ok(())
    }
    fn get_start(&mut self, data: &mut PathData) -> Result<Option<NodeId>, LevelError> {
        let bb = self.context.mob.bounding_box(self.context.level)?;
        Ok(Some(self.context.node(
            containing(Vec3::new(bb.min_x, bb.min_y + 0.5, bb.min_z)),
            data,
        )))
    }
    fn get_target(&mut self, pos: BlockPos, data: &mut PathData) -> Result<Target, LevelError> {
        let node = self.context.node(pos, data);
        Ok(Target::new(node, data))
    }
    fn get_neighbors(
        &mut self,
        node: NodeId,
        out: &mut Vec<NodeId>,
        data: &mut PathData,
    ) -> Result<(), LevelError> {
        let pos = data[node].pos;
        let mut sides = [None; 6];
        for (index, direction) in Direction::VALUES.into_iter().enumerate() {
            let node = self.accepted_node(pos.relative(direction, 1), data)?;
            sides[index] = node;
            if let Some(node) = node
                && !data[node].closed
            {
                out.push(node);
            }
        }
        for direction in HORIZONTAL {
            let clockwise = direction.clockwise();
            let a = sides[Direction::VALUES
                .iter()
                .position(|d| *d == direction)
                .unwrap()];
            let b = sides[Direction::VALUES
                .iter()
                .position(|d| *d == clockwise)
                .unwrap()];
            if a.is_some_and(|n| data[n].cost_malus >= 0.0)
                && b.is_some_and(|n| data[n].cost_malus >= 0.0)
            {
                let node =
                    self.accepted_node(pos.relative(direction, 1).relative(clockwise, 1), data)?;
                if let Some(node) = node
                    && !data[node].closed
                {
                    out.push(node);
                }
            }
        }
        Ok(())
    }
}
