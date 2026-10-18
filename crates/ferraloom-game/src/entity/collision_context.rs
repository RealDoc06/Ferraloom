//! Concrete util collision-context adapter. Captures values at vanilla's query point.
use super::{EntityData, EntityRef};
use crate::{
    block::fluid::FlowingFluid,
    level::{Level, LevelError},
};
use ferraloom_data::{FluidState, Item};
use ferraloom_util::shapes::{EntityCollisionContext, EntityCollisionData};

/// Current base entities cannot stand on fluids. Living subclasses override that
/// capability when their equipment/locomotion ports arrive, through this view.
pub struct EntityCollisionView {
    pub entity: EntityRef,
}
impl EntityCollisionData for EntityCollisionView {
    type Item = Item;
    type Fluid = FluidState;
    type HeldItem = Option<Item>;
    fn is_holding_item(&self, held: &Self::HeldItem, item: &Item) -> bool {
        *held == Some(*item)
    }
    fn can_stand_on_fluid(&self, _fluid: &FluidState) -> bool {
        false
    }
    fn is_same_fluid(&self, a: &FluidState, b: &FluidState) -> bool {
        if a.fluid_type == b.fluid_type {
            return true;
        }
        match (
            FlowingFluid::from_id(a.fluid_type),
            FlowingFluid::from_id(b.fluid_type),
        ) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }
}
impl EntityCollisionView {
    /// Port of LivingEntity#getLiquidCollisionShape's virtual query. None is a
    /// nonliving entity, distinct from a living entity whose shape is empty.
    pub fn liquid_collision_shape(
        &self,
        level: &Level,
    ) -> Result<Option<ferraloom_util::VoxelShape>, LevelError> {
        if level.entity_state(self.entity)?.as_living().is_none() {
            return Ok(None);
        }
        let behavior = level.entity_behavior(self.entity)?;
        behavior
            .liquid_collision_shape(self.entity, level)
            .map(Some)
    }

    pub fn capture(
        entity: EntityRef,
        level: &Level,
        always_collide_with_fluid: bool,
    ) -> Result<EntityCollisionContext<Self>, LevelError> {
        let base = level.entity::<EntityData>(entity)?;
        let held_item = match level.entity_state(entity)? {
            super::EntityState::Player(player) => Some(player.inventory.selected_item()),
            state => state.as_living().map(|living| {
                living
                    .equipment
                    .get(ferraloom_protocol::component::EquipmentSlot::Mainhand)
            }),
        };
        Ok(EntityCollisionContext {
            descending: base.shared_flag(1),
            placement: false,
            entity_bottom: base.bounding_box.min_y,
            always_collide_with_fluid,
            held_item: held_item
                .filter(|stack| !stack.is_empty())
                .and_then(|stack| stack.0.as_ref())
                .map(|stack| stack.item),
            entity: Self { entity },
        })
    }
}
