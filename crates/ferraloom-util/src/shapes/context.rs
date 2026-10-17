use super::VoxelShape;
use crate::{Axis, BlockPos};

/// Game-layer adapter for BlockState#getCollisionShape. Util deliberately owns
/// no item/fluid/level registry; the caller supplies its actual typed values.
pub trait CollisionShapeProvider<C: CollisionContext + ?Sized> {
    fn collision_shape(&self, pos: BlockPos, context: &C) -> VoxelShape;
}

/// Port of CollisionContext. Associated types keep util independent of game and
/// data while preserving typed held-item and fluid queries.
pub trait CollisionContext {
    type Item: ?Sized;
    type Fluid: ?Sized;
    fn is_descending(&self) -> bool;
    fn is_above(&self, shape: &VoxelShape, pos: BlockPos, default_value: bool) -> bool;
    fn is_holding_item(&self, item: &Self::Item) -> bool;
    fn always_collide_with_fluid(&self) -> bool;
    fn can_stand_on_fluid(&self, above: &Self::Fluid, fluid: &Self::Fluid) -> bool;
    fn is_placement(&self) -> bool {
        false
    }
    fn get_collision_shape(
        &self,
        provider: &impl CollisionShapeProvider<Self>,
        pos: BlockPos,
    ) -> VoxelShape
    where
        Self: Sized,
    {
        provider.collision_shape(pos, self)
    }
}

/// Live game-layer answers used by EntityCollisionContext. Implement this for
/// the entity handle/view; no world or entity snapshot is cloned into util.
pub trait EntityCollisionData {
    type Item: ?Sized;
    type Fluid: ?Sized;
    type HeldItem;
    fn is_holding_item(&self, held_item: &Self::HeldItem, item: &Self::Item) -> bool;
    fn can_stand_on_fluid(&self, fluid: &Self::Fluid) -> bool;
    fn is_same_fluid(&self, first: &Self::Fluid, second: &Self::Fluid) -> bool;
}

/// Port of EntityCollisionContext's captured movement/placement data. `entity`
/// supplies item membership and living-entity fluid-standing behavior. The
/// held stack is captured separately, just as vanilla captures getMainHandItem.
pub struct EntityCollisionContext<E: EntityCollisionData> {
    pub descending: bool,
    pub placement: bool,
    pub entity_bottom: f64,
    pub always_collide_with_fluid: bool,
    pub held_item: E::HeldItem,
    pub entity: E,
}
impl<E: EntityCollisionData> CollisionContext for EntityCollisionContext<E> {
    type Item = E::Item;
    type Fluid = E::Fluid;
    fn is_descending(&self) -> bool {
        self.descending
    }
    fn is_above(&self, shape: &VoxelShape, pos: BlockPos, _default_value: bool) -> bool {
        above(self.entity_bottom, shape, pos)
    }
    fn is_holding_item(&self, item: &Self::Item) -> bool {
        self.entity.is_holding_item(&self.held_item, item)
    }
    fn always_collide_with_fluid(&self) -> bool {
        self.always_collide_with_fluid
    }
    fn can_stand_on_fluid(&self, fluid_above: &Self::Fluid, fluid: &Self::Fluid) -> bool {
        self.entity.can_stand_on_fluid(fluid) && !self.entity.is_same_fluid(fluid_above, fluid)
    }
    fn is_placement(&self) -> bool {
        self.placement
    }
}

/// Port of EntityCollisionContext.Empty. Type parameters are the caller's item
/// and fluid registry types, not dependencies on their game-layer definitions.
#[derive(Clone, Copy, Debug)]
pub struct EmptyCollisionContext<I: ?Sized = (), F: ?Sized = ()> {
    pub fluid_collisions: bool,
    marker: std::marker::PhantomData<fn(&I, &F)>,
}
impl<I: ?Sized, F: ?Sized> EmptyCollisionContext<I, F> {
    pub const fn new(fluid_collisions: bool) -> Self {
        Self {
            fluid_collisions,
            marker: std::marker::PhantomData,
        }
    }
}
impl<I: ?Sized, F: ?Sized> CollisionContext for EmptyCollisionContext<I, F> {
    type Item = I;
    type Fluid = F;
    fn is_descending(&self) -> bool {
        false
    }
    fn is_above(&self, _shape: &VoxelShape, _pos: BlockPos, default_value: bool) -> bool {
        default_value
    }
    fn is_holding_item(&self, _item: &I) -> bool {
        false
    }
    fn always_collide_with_fluid(&self) -> bool {
        self.fluid_collisions
    }
    fn can_stand_on_fluid(&self, _above: &F, _fluid: &F) -> bool {
        false
    }
}

/// Port of PositionCollisionContext.
#[derive(Clone, Copy, Debug)]
pub struct PositionCollisionContext<I: ?Sized = (), F: ?Sized = ()> {
    pub y: f64,
    marker: std::marker::PhantomData<fn(&I, &F)>,
}
impl<I: ?Sized, F: ?Sized> PositionCollisionContext<I, F> {
    pub const fn new(y: f64) -> Self {
        Self {
            y,
            marker: std::marker::PhantomData,
        }
    }
}
impl<I: ?Sized, F: ?Sized> CollisionContext for PositionCollisionContext<I, F> {
    type Item = I;
    type Fluid = F;
    fn is_descending(&self) -> bool {
        false
    }
    fn is_above(&self, shape: &VoxelShape, pos: BlockPos, _default_value: bool) -> bool {
        above(self.y, shape, pos)
    }
    fn is_holding_item(&self, _item: &I) -> bool {
        false
    }
    fn always_collide_with_fluid(&self) -> bool {
        false
    }
    fn can_stand_on_fluid(&self, _above: &F, _fluid: &F) -> bool {
        false
    }
}
fn above(y: f64, shape: &VoxelShape, pos: BlockPos) -> bool {
    y > f64::from(pos.y) + shape.max(Axis::Y) - 9.999999747378752E-6
}
