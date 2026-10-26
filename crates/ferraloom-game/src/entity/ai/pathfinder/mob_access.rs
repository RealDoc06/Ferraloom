use super::{Node, NodeId, PathData, PathType, pathfinding_context};
use crate::{
    block::LevelReaderRef,
    entity::EntityRef,
    level::{Level, LevelError, path_navigation_region::PathNavigationRegion},
};
use ferraloom_data::{BlockState, FluidState};
use ferraloom_util::{Aabb, BlockPos, Vec3, collections::Int2ObjectOpenHashMap};

/// The native Mob calls made by NodeEvaluator. Implementations access the
/// canonical receiver through short level borrows; this contract owns no Mob copy.
pub trait MobAccess {
    /// Entity#getX/getY/getZ.
    fn position(&self, level: &Level) -> Result<Vec3, LevelError>;
    /// Entity#getBoundingBox.
    fn bounding_box(&self, level: &Level) -> Result<Aabb, LevelError>;
    /// Entity#getBbWidth/getBbHeight.
    fn dimensions(&self, level: &Level) -> Result<(f32, f32), LevelError>;
    /// Entity#onGround.
    fn on_ground(&self, level: &Level) -> Result<bool, LevelError>;
    /// Entity#isInWater.
    fn is_in_water(&self, level: &Level) -> Result<bool, LevelError>;
    /// Entity#isInFloatableFluid.
    fn is_in_floatable_fluid(&self, level: &Level) -> Result<bool, LevelError>;
    /// LivingEntity#canStandOnFluid.
    fn can_stand_on_fluid(&self, fluid: FluidState, level: &Level) -> Result<bool, LevelError>;
    /// LivingEntity#maxUpStep.
    fn max_up_step(&self, level: &Level) -> Result<f32, LevelError>;
    /// Mob#getMaxFallDistance.
    fn max_fall_distance(&self, level: &Level) -> Result<i32, LevelError>;
    /// Mob#getPathfindingMalus (including subclass overrides).
    fn pathfinding_malus(&self, path_type: PathType, level: &Level) -> Result<f32, LevelError>;
    /// Mob#setPathfindingMalus.
    fn set_pathfinding_malus(
        &mut self,
        path_type: PathType,
        value: f32,
        level: &mut Level,
    ) -> Result<(), LevelError>;
    /// Mob#onPathfindingStart.
    fn on_pathfinding_start(&mut self, level: &mut Level) -> Result<(), LevelError>;
    /// Mob#onPathfindingDone.
    fn on_pathfinding_done(&mut self, level: &mut Level) -> Result<(), LevelError>;
    /// Mob#getRandom().nextInt(bound), for lazy fly-start candidates.
    fn next_random_int(&mut self, bound: i32, level: &mut Level) -> Result<i32, LevelError>;
    /// The original Mob passed to CollisionGetter#noCollision.
    fn collision_source(&self) -> EntityRef;
}

/// NodeEvaluator's configurable capabilities. Defaults are native defaults.
#[derive(Clone, Copy, Debug)]
pub struct Capabilities {
    pub can_pass_doors: bool,
    pub can_open_doors: bool,
    pub can_float: bool,
    pub can_walk_over_fences: bool,
}
impl Default for Capabilities {
    fn default() -> Self {
        Self {
            can_pass_doors: true,
            can_open_doors: false,
            can_float: false,
            can_walk_over_fences: false,
        }
    }
}

/// Prepared NodeEvaluator/PathfindingContext state. Only native prepare-time
/// dimensions and mobPosition are captured; all other receiver reads stay live.
pub(super) struct Context<'a, M> {
    pub region: &'a PathNavigationRegion,
    pub level: &'a mut Level,
    pub mob: &'a mut M,
    pub capabilities: Capabilities,
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    pub mob_position: BlockPos,
    nodes: Int2ObjectOpenHashMap<NodeId>,
}
impl<'a, M: MobAccess> Context<'a, M> {
    pub fn new(region: &'a PathNavigationRegion, level: &'a mut Level, mob: &'a mut M) -> Self {
        Self {
            region,
            level,
            mob,
            capabilities: Capabilities::default(),
            width: 0,
            height: 0,
            depth: 0,
            mob_position: BlockPos::ZERO,
            nodes: Int2ObjectOpenHashMap::new(),
        }
    }
    pub fn prepare(&mut self) -> Result<(), LevelError> {
        self.mob_position = containing(self.mob.position(self.level)?);
        self.nodes.clear();
        let (width, height) = self.mob.dimensions(self.level)?;
        self.width = (width + 1.0).floor() as i32;
        self.height = (height + 1.0).floor() as i32;
        self.depth = self.width;
        Ok(())
    }
    pub fn node(&mut self, pos: BlockPos, data: &mut PathData) -> NodeId {
        let hash = Node::create_hash(pos.x, pos.y, pos.z);
        if let Some(id) = self.nodes.get(&hash) {
            return *id;
        }
        let id = data.add_node(Node::new(pos.x, pos.y, pos.z));
        self.nodes.insert(hash, id);
        id
    }
    pub fn reader(&self) -> Result<LevelReaderRef<'_, 'static>, LevelError> {
        self.region.reader(self.level)
    }
    pub fn state(&self, pos: BlockPos) -> Result<BlockState, LevelError> {
        self.reader()?.get_block_state(pos)
    }
    pub fn raw_type(&mut self, pos: BlockPos) -> Result<PathType, LevelError> {
        pathfinding_context::get_path_type_from_state(self.region, self.level, pos)
    }
    pub fn malus(&self, path_type: PathType) -> Result<f32, LevelError> {
        self.mob.pathfinding_malus(path_type, self.level)
    }
    pub fn no_collision(&self, area: Aabb) -> Result<bool, LevelError> {
        self.reader()?
            .no_collision(Some(self.mob.collision_source()), area)
    }
}
pub(super) fn containing(pos: Vec3) -> BlockPos {
    BlockPos::new(
        pos.x.floor() as i32,
        pos.y.floor() as i32,
        pos.z.floor() as i32,
    )
}
