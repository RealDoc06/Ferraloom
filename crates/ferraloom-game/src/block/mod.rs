//! Stateless virtual block dispatch. Mutable state always belongs to the level.
use crate::level::{Level, LevelError, redstone::Orientation};
use ferraloom_data::{Block, BlockState, FluidState};
use ferraloom_util::{BlockPos, Direction};
pub mod dispatch;
pub mod flags;
pub mod fluid;
mod foundational;
mod shapes;
pub use shapes::BlockCollisionContext;
mod unsupported;
pub use flags::{UPDATE_LIMIT, UpdateFlags};
pub(crate) use unsupported::UnportedCalls;
pub use unsupported::{BlockMethod, UnportedBehavior, UnportedBehaviorStats};

/// Port of the shared LevelReader/LevelAccessor call surface. The live arm
/// borrows the concrete simulation facade, including during reentrant callbacks.
pub enum LevelAccessRef<'a> {
    Server(&'a mut Level),
    // The Worldgen arm and adapter are added when worldgen slice 2 lands.
}
/// Read-only LevelReader view used by shared survival and shape queries.
pub enum LevelReaderRef<'a> {
    Server(&'a Level),
}
impl LevelReaderRef<'_> {
    pub fn get_block_state(&self, pos: BlockPos) -> Result<BlockState, LevelError> {
        match self {
            Self::Server(level) => level.get_block_state(pos),
        }
    }
    pub fn fluid_tags(
        &self,
    ) -> Result<&ferraloom_data::datapack::TagSet<ferraloom_data::registries::fluid::Id>, LevelError>
    {
        match self {
            Self::Server(level) => level.fluid_tags(),
        }
    }
    pub fn block_tags(&self) -> Result<&ferraloom_data::datapack::TagSet<Block>, LevelError> {
        match self {
            Self::Server(level) => level.block_tags(),
        }
    }
}
impl LevelAccessRef<'_> {
    /// Port of Block#updateFromNeighbourShapes. This query returns a required
    /// placement state, so unsupported calls fail even under Warn policy.
    pub fn update_from_neighbor_shapes(
        &mut self,
        mut state: BlockState,
        pos: BlockPos,
    ) -> Result<BlockState, LevelError> {
        for direction in crate::level::redstone::UPDATE_SHAPE_ORDER {
            let neighbor_pos = pos.relative(direction, 1);
            let neighbor = self.get_block_state(neighbor_pos)?;
            let behavior = match self {
                Self::Server(level) => level.required_block_behavior(
                    state.block(),
                    BlockMethod::UpdateFromNeighbourShapes,
                )?,
            };
            state = behavior.update_shape(state, pos, direction, neighbor_pos, neighbor, self)?;
        }
        Ok(state)
    }
    pub fn as_reader(&self) -> LevelReaderRef<'_> {
        match self {
            Self::Server(level) => LevelReaderRef::Server(level),
        }
    }
    pub fn get_block_state(&self, pos: BlockPos) -> Result<BlockState, LevelError> {
        match self {
            Self::Server(level) => level.get_block_state(pos),
        }
    }
    pub fn set_block(
        &mut self,
        pos: BlockPos,
        state: BlockState,
        flags: UpdateFlags,
    ) -> Result<bool, LevelError> {
        match self {
            Self::Server(level) => level.set_block(pos, state, flags, UPDATE_LIMIT),
        }
    }
    pub fn schedule_block_tick(
        &mut self,
        pos: BlockPos,
        block: Block,
        delay: i32,
    ) -> Result<(), LevelError> {
        match self {
            Self::Server(level) => level.schedule_block_tick_after(pos, block, delay),
        }
    }
    pub fn schedule_fluid_tick(
        &mut self,
        pos: BlockPos,
        fluid: ferraloom_data::registries::fluid::Id,
        delay: i32,
    ) -> Result<(), LevelError> {
        match self {
            Self::Server(level) => level.schedule_fluid_tick_after(pos, fluid, delay),
        }
    }
    pub fn fast_lava(&self) -> bool {
        match self {
            Self::Server(level) => level.rules().fast_lava,
        }
    }
}

/// Port of BlockBehaviour's virtual methods. Defaults are vanilla base behavior;
/// only classes explicitly registered by dispatch may inherit these defaults.
pub trait BlockBehavior: Sync {
    /// Port of BlockBehaviour#getEntityInsideCollisionShape's base full cube.
    fn entity_inside_collision_shape(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _entity: crate::entity::EntityRef,
        _level: &LevelReaderRef<'_>,
    ) -> Result<ferraloom_util::VoxelShape, LevelError> {
        Ok(ferraloom_util::shapes::block())
    }
    /// Port of BlockBehaviour#getCollisionShape for registered base families.
    /// Dynamic overrides must implement this method; no extracted baseline is
    /// treated as a live contextual answer.
    fn collision_shape(
        &self,
        state: BlockState,
        _pos: BlockPos,
        _context: &BlockCollisionContext<'_>,
        _level: &LevelReaderRef<'_>,
    ) -> Result<ferraloom_util::VoxelShape, LevelError> {
        if state.cached_collision_full_block().is_some() {
            Ok(state.cached_shapes().collision.get().clone())
        } else {
            Err(LevelError::UnsupportedShape(state))
        }
    }
    fn shape(
        &self,
        state: BlockState,
        _pos: BlockPos,
        _level: &LevelReaderRef<'_>,
    ) -> Result<ferraloom_util::VoxelShape, LevelError> {
        state
            .shape()
            .cloned()
            .ok_or(LevelError::UnsupportedShape(state))
    }
    fn support_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_>,
    ) -> Result<ferraloom_util::VoxelShape, LevelError> {
        level.collision_shape(state, pos, &BlockCollisionContext::empty())
    }
    fn on_place(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _old: BlockState,
        _moved: bool,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn affect_neighbors_after_removal(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _moved: bool,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn neighbor_changed(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _source: Block,
        _orientation: Option<Orientation>,
        _moved: bool,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn update_shape(
        &self,
        state: BlockState,
        _pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        _level: &mut LevelAccessRef<'_>,
    ) -> Result<BlockState, LevelError> {
        Ok(state)
    }
    fn update_indirect_neighbor_shapes(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _flags: UpdateFlags,
        _limit: i32,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn can_survive(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &LevelReaderRef<'_>,
    ) -> Result<bool, LevelError> {
        Ok(true)
    }
    fn tick(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn random_tick(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    /// Port of BlockState#entityInside, called with the state captured by
    /// Entity#checkInsideBlocks. Entity data stays canonical in the level.
    fn entity_inside(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _entity: crate::entity::EntityRef,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    /// Port of Block#fallOn, preserving the f32 reduction before multiplying
    /// fall distance in f64 and dispatching to the canonical entity behavior.
    fn fall_on(
        &self,
        state: BlockState,
        _pos: BlockPos,
        entity: crate::entity::EntityRef,
        fall_distance: f64,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let reduced =
            fall_distance * f64::from(1.0_f32 - state.block().data().fall_distance_reduction);
        let behavior = level.entity_behavior(entity)?;
        behavior.cause_fall_damage(entity, level, reduced, 1.0)?;
        Ok(())
    }
    /// Port of Block#stepOn's base no-op.
    fn step_on(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _entity: crate::entity::EntityRef,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    /// Port of Block#bounceOn's base no-op.
    fn bounce_on(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _entity: crate::entity::EntityRef,
        _fall_distance: f64,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn trigger_event(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _a: i32,
        _b: i32,
        _level: &mut Level,
    ) -> Result<bool, LevelError> {
        Ok(false)
    }
    fn place_liquid(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _fluid: FluidState,
        _level: &mut LevelAccessRef<'_>,
    ) -> Result<bool, LevelError> {
        Ok(false)
    }
    fn can_place_liquid(
        &self,
        _state: BlockState,
        _fluid: ferraloom_data::registries::fluid::Id,
    ) -> bool {
        false
    }
    fn pickup_block(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &mut LevelAccessRef<'_>,
    ) -> Result<Option<ferraloom_data::Item>, LevelError> {
        Ok(None)
    }
}

/// Port of BlockBehaviour's base implementation, including AirBlock's unchanged
/// simulation methods. Static physical state facts remain in ferraloom-data.
pub struct BaseBlock;
impl BlockBehavior for BaseBlock {}
pub static BASE_BLOCK: BaseBlock = BaseBlock;
