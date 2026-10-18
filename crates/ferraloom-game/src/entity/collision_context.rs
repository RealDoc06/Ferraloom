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
    pub entity: Option<EntityRef>,
    living: bool,
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
        if !self.living {
            return Ok(None);
        }
        let Some(entity) = self.entity else {
            return Ok(None);
        };
        let behavior = level.entity_behavior(entity)?;
        behavior.liquid_collision_shape(entity, level).map(Some)
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
            entity_bottom: base.position.y,
            always_collide_with_fluid,
            held_item: Some(
                held_item.map_or(Item::AIR, ferraloom_protocol::component::ItemStack::item),
            ),
            entity: Self {
                entity: Some(entity),
                living: level.entity_state(entity)?.as_living().is_some(),
            },
        })
    }
    /// Port of CollisionContext#placementContext. Capture the player's current
    /// main hand and position Y, independently of its bounding-box minimum.
    pub fn placement_context(
        player: Option<EntityRef>,
        level: &Level,
    ) -> Result<EntityCollisionContext<Self>, LevelError> {
        let (descending, entity_bottom, held_item) = match player {
            Some(player) => {
                let player = level.entity::<super::player::ServerPlayer>(player)?;
                (
                    player.shared_flag(1),
                    player.position.y,
                    player.get_main_hand_item().item(),
                )
            }
            // Native ItemStack.EMPTY has AIR as its effective item holder.
            None => (false, -f64::MAX, Item::AIR),
        };
        Ok(EntityCollisionContext {
            descending,
            placement: true,
            entity_bottom,
            always_collide_with_fluid: false,
            held_item: Some(held_item),
            entity: Self {
                entity: player,
                living: player.is_some(),
            },
        })
    }
    /// Constructors can query collisions before addFreshEntity. Their concrete
    /// nonliving type is immutable and requires no lookup in the visible index.
    pub(crate) fn capture_untracked_nonliving(
        entity: EntityRef,
        base: &EntityData,
    ) -> EntityCollisionContext<Self> {
        EntityCollisionContext {
            descending: base.shared_flag(1),
            placement: false,
            entity_bottom: base.position.y,
            always_collide_with_fluid: false,
            held_item: Some(Item::AIR),
            entity: Self {
                entity: Some(entity),
                living: false,
            },
        }
    }
}

#[cfg(test)]
mod tests;
