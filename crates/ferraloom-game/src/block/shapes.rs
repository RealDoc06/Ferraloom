//! Shared block shape dispatch for live simulation and worldgen. Native static
//! caches are authoritative; contextual queries call the actual block family.
use super::{BlockMethod, LevelReaderRef};
use crate::{
    entity::{EntityState, collision_context::EntityCollisionView, player::GameType},
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
impl LevelReaderRef<'_, '_> {
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
        self.required_behavior(state.block(), method)
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
    /// Port of BlockStateBase#getCollisionShape(BlockGetter, BlockPos). This
    /// overload uses the real state cache even for states whose contextual
    /// overload needs a live query. Dynamic states dispatch with empty context.
    pub fn collision_shape_without_context(
        &self,
        state: BlockState,
        pos: BlockPos,
    ) -> Result<VoxelShape, LevelError> {
        if state.cached_collision_full_block().is_some() {
            return Ok(state.cached_shapes().collision.get().clone());
        }
        self.collision_shape(state, pos, &BlockCollisionContext::empty())
    }
    pub fn shape(&self, state: BlockState, pos: BlockPos) -> Result<VoxelShape, LevelError> {
        if let Some(shape) = state.shape() {
            return Ok(shape.clone());
        }
        self.shape_behavior(state, BlockMethod::OutlineShape)?
            .shape(state, pos, self)
    }
    /// Port of BlockStateBase#getShape with an explicit CollisionContext.
    pub fn shape_with_context(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &BlockCollisionContext<'_>,
    ) -> Result<VoxelShape, LevelError> {
        if let Some(shape) = state.shape() {
            return Ok(shape.clone());
        }
        self.shape_behavior(state, BlockMethod::OutlineShape)?
            .shape_with_context(state, pos, context, self)
    }
    pub fn visual_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &BlockCollisionContext<'_>,
    ) -> Result<VoxelShape, LevelError> {
        self.shape_behavior(state, BlockMethod::VisualShape)?
            .visual_shape(state, pos, context, self)
    }
    pub fn interaction_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
    ) -> Result<VoxelShape, LevelError> {
        self.shape_behavior(state, BlockMethod::InteractionShape)?
            .interaction_shape(state, pos, self)
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

    /// Port of CollisionGetter#isUnobstructed(BlockState,BlockPos,CollisionContext)
    /// and EntityGetter#isUnobstructed(null,shape). The context affects the block
    /// shape; its entity is not excluded from the native null-source query.
    pub fn is_unobstructed(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &BlockCollisionContext<'_>,
    ) -> Result<bool, LevelError> {
        let shape = self.collision_shape(state, pos, context)?;
        if shape.is_empty() {
            return Ok(true);
        }
        let shape = shape.move_by(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
        for entity in self.entities_in(shape.bounds(), |_, state| {
            !matches!(state, EntityState::Player(player) if player.game_mode == GameType::Spectator)
        }) {
            let entity = self.entity_state(entity)?.as_entity();
            if !entity.removed
                && entity.blocks_building
                && shapes::join_is_not_empty(
                    &shape,
                    &shapes::create_aabb(entity.bounding_box),
                    shapes::BooleanOp::And,
                )
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
