//! Shared block shape dispatch for live simulation and worldgen. Native static
//! caches are authoritative; contextual queries call the actual block family.
use super::{BlockMethod, LevelReaderRef};
use crate::{
    entity::collision_context::EntityCollisionView,
    level::{Level, LevelError},
};
use ferraloom_data::{BlockState, FluidState, Item};
use ferraloom_util::{
    BlockPos, Direction, VoxelShape,
    shapes::{
        self, CollisionContext, EmptyCollisionContext, EntityCollisionContext,
        PositionCollisionContext,
    },
};

/// Concrete collision context. Entity data is captured by entity-core at the
/// vanilla query point; no world trait or dynamic context dispatch is needed.
pub enum BlockCollisionContext<'a> {
    Empty(EmptyCollisionContext<Item, FluidState>),
    Entity(EntityCollisionContext<EntityCollisionView>),
    Position(&'a PositionCollisionContext<Item, FluidState>),
}
impl BlockCollisionContext<'_> {
    pub const fn empty() -> Self {
        Self::empty_with_fluid_collisions(false)
    }
    pub const fn empty_with_fluid_collisions(always_collide: bool) -> Self {
        Self::Empty(EmptyCollisionContext::new(always_collide))
    }
}
impl CollisionContext for BlockCollisionContext<'_> {
    type Item = Item;
    type Fluid = FluidState;
    fn is_descending(&self) -> bool {
        match self {
            Self::Empty(c) => c.is_descending(),
            Self::Entity(c) => c.is_descending(),
            Self::Position(c) => c.is_descending(),
        }
    }
    fn is_above(&self, shape: &VoxelShape, pos: BlockPos, default: bool) -> bool {
        match self {
            Self::Empty(c) => c.is_above(shape, pos, default),
            Self::Entity(c) => c.is_above(shape, pos, default),
            Self::Position(c) => c.is_above(shape, pos, default),
        }
    }
    fn is_holding_item(&self, item: &Item) -> bool {
        match self {
            Self::Empty(c) => c.is_holding_item(item),
            Self::Entity(c) => c.is_holding_item(item),
            Self::Position(c) => c.is_holding_item(item),
        }
    }
    fn always_collide_with_fluid(&self) -> bool {
        match self {
            Self::Empty(c) => c.always_collide_with_fluid(),
            Self::Entity(c) => c.always_collide_with_fluid(),
            Self::Position(c) => c.always_collide_with_fluid(),
        }
    }
    fn can_stand_on_fluid(&self, above: &FluidState, fluid: &FluidState) -> bool {
        match self {
            Self::Empty(c) => c.can_stand_on_fluid(above, fluid),
            Self::Entity(c) => c.can_stand_on_fluid(above, fluid),
            Self::Position(c) => c.can_stand_on_fluid(above, fluid),
        }
    }
    fn is_placement(&self) -> bool {
        match self {
            Self::Empty(c) => c.is_placement(),
            Self::Entity(c) => c.is_placement(),
            Self::Position(c) => c.is_placement(),
        }
    }
}
impl LevelReaderRef<'_> {
    pub fn entity_inside_collision_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        entity: crate::entity::EntityRef,
    ) -> Result<VoxelShape, LevelError> {
        self.shape_behavior(state, BlockMethod::EntityInsideCollisionShape)?
            .entity_inside_collision_shape(state, pos, entity, self)
    }
    fn shape_behavior(
        &self,
        state: BlockState,
        method: BlockMethod,
    ) -> Result<&'static dyn super::BlockBehavior, LevelError> {
        match self {
            Self::Server(level) => level.required_block_behavior(state.block(), method),
        }
    }
    pub fn collision_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &BlockCollisionContext<'_>,
    ) -> Result<VoxelShape, LevelError> {
        if let Some(shape) = state.collision_shape() {
            return Ok(shape.clone());
        }
        self.shape_behavior(state, BlockMethod::CollisionShape)?
            .collision_shape(state, pos, context, self)
    }
    pub fn shape(&self, state: BlockState, pos: BlockPos) -> Result<VoxelShape, LevelError> {
        if let Some(shape) = state.shape() {
            return Ok(shape.clone());
        }
        self.shape_behavior(state, BlockMethod::OutlineShape)?
            .shape(state, pos, self)
    }
    pub fn is_face_sturdy(
        &self,
        state: BlockState,
        pos: BlockPos,
        face: Direction,
    ) -> Result<bool, LevelError> {
        if let Some(sturdy) = state.cached_sturdy_face(face) {
            return Ok(sturdy);
        }
        let shape = self
            .shape_behavior(state, BlockMethod::IsFaceSturdy)?
            .support_shape(state, pos, self)?;
        Ok(shapes::is_face_full(&shape, face))
    }
    pub fn is_collision_shape_full_block(
        &self,
        state: BlockState,
        pos: BlockPos,
    ) -> Result<bool, LevelError> {
        if let Some(full) = state.cached_collision_full_block() {
            return Ok(full);
        }
        self.shape_behavior(state, BlockMethod::IsCollisionShapeFullBlock)?
            .is_collision_shape_full_block(state, pos, self)
    }
}
impl Level {
    pub fn collision_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &BlockCollisionContext<'_>,
    ) -> Result<VoxelShape, LevelError> {
        LevelReaderRef::Server(self).collision_shape(state, pos, context)
    }
}
