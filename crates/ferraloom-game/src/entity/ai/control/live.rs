use super::runtime::{self, ControlRuntime};
use crate::{
    block::BlockCollisionContext,
    entity::{EntityRef, Mob},
    level::{Level, LevelError},
};
use ferraloom_data::{registries::attribute::Id as Attribute, tags::BlockTags};
use ferraloom_util::Axis;

/// The live controller receiver remains in canonical Mob storage across callbacks.
struct Live<'a> {
    entity: EntityRef,
    level: &'a mut Level,
}
impl ControlRuntime for Live<'_> {
    type Error = LevelError;
    fn mob(&self) -> Result<&Mob, LevelError> {
        self.level.entity::<Mob>(self.entity)
    }
    fn mob_mut(&mut self) -> Result<&mut Mob, LevelError> {
        self.level.entity_mut::<Mob>(self.entity)
    }
    fn movement_speed(&self) -> Result<f64, LevelError> {
        let mob = self.mob()?;
        mob.parent
            .attributes
            .value(Attribute::MOVEMENT_SPEED)
            .map_err(|_| LevelError::MissingEntityAttribute {
                entity_type: mob.parent.parent.entity_type,
                attribute: Attribute::MOVEMENT_SPEED,
            })
    }
    fn max_up_step(&self) -> Result<f32, LevelError> {
        self.level
            .entity_behavior(self.entity)?
            .max_up_step(self.entity, self.level)
    }
    fn is_walkable(&mut self, _dx: f32, _dz: f32) -> Result<bool, LevelError> {
        // PathNavigation owns the concrete NodeEvaluator. Do not substitute a
        // raw block-path type before that canonical navigation adapter lands.
        Err(LevelError::UnsupportedMobAi {
            entity: self.entity,
            entity_type: self.mob()?.parent.parent.entity_type,
            operation: super::super::AiOperation::NavigationPathType,
        })
    }
    fn collision_height(&mut self) -> Result<Option<f64>, LevelError> {
        let pos = self.mob()?.parent.parent.block_position;
        let state = self.level.get_block_state(pos)?;
        // Native BlockState#getCollisionShape(Level, BlockPos) uses empty context.
        let shape = self
            .level
            .collision_shape(state, pos, &BlockCollisionContext::empty())?;
        Ok((!shape.is_empty()).then(|| shape.max(Axis::Y) + f64::from(pos.y)))
    }
    fn collision_is_door_or_fence(&self) -> Result<bool, LevelError> {
        let state = self
            .level
            .get_block_state(self.mob()?.parent.parent.block_position)?;
        let tags = self.level.block_tags()?;
        Ok(tags.contains_key(BlockTags::DOORS, state.block())
            || tags.contains_key(BlockTags::FENCES, state.block()))
    }
    fn is_in_liquid(&self) -> Result<bool, LevelError> {
        let base = &self.mob()?.parent.parent;
        Ok(base.was_touching_water || base.is_in_lava(self.level)?)
    }
    fn is_affected_by_fluids(&self) -> Result<bool, LevelError> {
        self.level
            .entity_behavior(self.entity)?
            .is_affected_by_fluids(self.entity, self.level)
    }
    fn navigation_is_done(&self) -> Result<bool, LevelError> {
        Err(LevelError::UnsupportedMobAi {
            entity: self.entity,
            entity_type: self.mob()?.parent.parent.entity_type,
            operation: super::super::AiOperation::NavigationIsDone,
        })
    }
    fn max_head_y_rot(&self) -> Result<i32, LevelError> {
        self.level
            .entity_behavior(self.entity)?
            .get_max_head_y_rot(self.entity, self.level)
    }
    fn carrying_mob_passenger(&self) -> Result<bool, LevelError> {
        let first = self.mob()?.parent.parent.passengers.first().copied();
        first
            .map(|id| self.level.entity_state(id).map(|s| s.as_mob().is_some()))
            .transpose()
            .map(|v| v.unwrap_or(false))
    }
    fn set_speed(&mut self, speed: f32) -> Result<(), LevelError> {
        self.level
            .entity_behavior(self.entity)?
            .set_speed(self.entity, speed, self.level)
    }
    fn set_zza(&mut self, value: f32) -> Result<(), LevelError> {
        self.level
            .entity_behavior(self.entity)?
            .set_zza(self.entity, value, self.level)
    }
    fn set_xxa(&mut self, value: f32) -> Result<(), LevelError> {
        self.level
            .entity_behavior(self.entity)?
            .set_xxa(self.entity, value, self.level)
    }
    fn set_jumping(&mut self, jumping: bool) -> Result<(), LevelError> {
        self.level
            .entity_behavior(self.entity)?
            .set_jumping(self.entity, jumping, self.level)
    }
}

fn retained(
    entity: EntityRef,
    level: &mut Level,
    tick: impl FnOnce(&mut Live<'_>) -> Result<(), LevelError>,
) -> Result<(), LevelError> {
    level.retain_entity(entity)?;
    let result = tick(&mut Live { entity, level });
    level.release_entity(entity);
    result
}
/// Port of MoveControl#tick's live base-controller entry.
pub fn tick_move(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    retained(entity, level, runtime::tick_move)
}
/// Port of LookControl#tick's live base-controller entry.
pub fn tick_look(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    retained(entity, level, runtime::tick_look)
}
/// Port of JumpControl#tick's live base-controller entry.
pub fn tick_jump(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    retained(entity, level, runtime::tick_jump)
}
/// Port of BodyRotationControl#clientTick's live base-controller entry.
pub fn tick_body_rotation(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    retained(entity, level, runtime::tick_body)
}
