//! Port of PowderSnowBlock collision, contact effects, landing and bucket pickup.
use super::super::{
    BlockBehavior, BlockCollisionContext, LevelAccessRef, LevelReaderRef, UpdateFlags,
};
use crate::{
    entity::{
        EntityData, EntityRef, EntityState,
        inside_block::{self, InsideBlockAction, InsideBlockEffectType},
    },
    level::{Level, LevelError, effects::LevelEffect, emission::ParticleEmission},
};
use ferraloom_data::{Block, BlockState, EntityType, Item, tags::EntityTypeTags};
use ferraloom_protocol::{entity_data::Particle, play::ParticleRandomization};
use ferraloom_util::{
    BlockPos, Vec3, VoxelShape,
    random::RandomSource,
    shapes::{self, CollisionContext},
};
pub struct PowderSnow;
pub static POWDER_SNOW: PowderSnow = PowderSnow;
/// Port of PowderSnowBlock#canEntityWalkOnPowderSnow, including tagged mobs.
pub fn can_walk_on(entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
    can_walk_on_reader(entity, &LevelReaderRef::Server(level))
}
fn can_walk_on_reader(
    entity: EntityRef,
    reader: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    let base = reader.entity_state(entity)?.as_entity();
    if reader
        .entity_type_tags()?
        .contains_key(EntityTypeTags::POWDER_SNOW_WALKABLE_MOBS, base.entity_type)
    {
        return Ok(true);
    }
    Ok(reader
        .entity_state(entity)?
        .as_living()
        .is_some_and(|living| {
            living
                .equipment
                .get(ferraloom_protocol::component::EquipmentSlot::Feet)
                .item()
                == Item::LEATHER_BOOTS
        }))
}

/// Port of PowderSnowBlock#entityInside's captured runBefore(EXTINGUISH) action.
pub(crate) fn before_extinguish(
    entity: EntityRef,
    pos: BlockPos,
    level: &mut Level,
) -> Result<(), LevelError> {
    if inside_block::is_on_fire(entity, level)?
        && (level.rules().mob_griefing
            || matches!(level.entity_state(entity)?, EntityState::Player(_)))
        && level.may_interact(entity, pos)?
    {
        level.destroy_block(pos, false, super::super::UPDATE_LIMIT)?;
    }
    Ok(())
}

/// Port of PowderSnowBlock#entityInside. The supplied callback is the entity's
/// makeStuckInBlock virtual; no canonical entity borrow crosses that callback.
fn entity_inside_with_stuck(
    state: BlockState,
    pos: BlockPos,
    entity: EntityRef,
    level: &mut Level,
    make_stuck: impl FnOnce(EntityRef, BlockState, Vec3, &mut Level) -> Result<(), LevelError>,
) -> Result<(), LevelError> {
    let receiver = level.entity_state(entity)?;
    if receiver.as_living().is_none()
        || receiver.as_entity().in_block_state(level)?.block() == Block::POWDER_SNOW
    {
        make_stuck(
            entity,
            state,
            Vec3::new(0.8999999761581421, 1.5, 0.8999999761581421),
            level,
        )?;
        let movement = level
            .entity_behavior(entity)?
            .known_movement(entity, level)?;
        if (movement.x != 0.0 || movement.z != 0.0) && level.random().next_boolean() {
            let at = level.entity::<EntityData>(entity)?.position;
            let x_speed = (level.random().next_float() * 2.0 - 1.0) * 0.083333336_f32;
            let z_speed = (level.random().next_float() * 2.0 - 1.0) * 0.083333336_f32;
            level.send_particles_with_options(
                Particle::Snowflake,
                ParticleEmission {
                    override_limiter: false,
                    always_show: false,
                    position: Vec3::new(at.x, f64::from(pos.y) + 1.0, at.z),
                    count: 1,
                    distance: Vec3::ZERO,
                    speed: Vec3::new(f64::from(x_speed), 0.05000000074505806, f64::from(z_speed)),
                    randomization: ParticleRandomization::Alternative,
                },
            )?;
        }
    }
    let collector = &mut level
        .entity_mut::<EntityData>(entity)?
        .inside_effect_collector;
    collector.run_before(
        InsideBlockEffectType::Extinguish,
        InsideBlockAction::PowderSnowExtinguish(pos),
    );
    collector.apply(InsideBlockEffectType::Freeze);
    collector.apply(InsideBlockEffectType::Extinguish);
    Ok(())
}
impl BlockBehavior for PowderSnow {
    fn entity_inside(
        &self,
        state: BlockState,
        pos: BlockPos,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        entity_inside_with_stuck(
            state,
            pos,
            entity,
            level,
            crate::entity::make_stuck_in_block,
        )
    }

    /// Port of PowderSnowBlock#fallOn. Powder snow never calls base fall damage.
    fn fall_on(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        entity: EntityRef,
        fall_distance: f64,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if fall_distance < 4.0 || level.entity_state(entity)?.as_living().is_none() {
            return Ok(());
        }
        let behavior = level.entity_behavior(entity)?;
        let sounds = behavior.fall_sounds();
        behavior.play_sound(
            entity,
            if fall_distance < 7.0 {
                sounds.small
            } else {
                sounds.big
            },
            1.0,
            1.0,
            level,
        )
    }

    fn collision_shape(
        &self,
        _state: BlockState,
        pos: BlockPos,
        context: &BlockCollisionContext<'_>,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        if !context.is_placement()
            && let BlockCollisionContext::Entity(entity_context) = context
            && let Some(entity) = entity_context.entity.entity
        {
            let base = reader.entity_state(entity)?.as_entity();
            if base.fall_distance > 2.5 {
                return Ok(shapes::create(0.0, 0.0, 0.0, 1.0, 0.8999999761581421, 1.0));
            }
            if base.entity_type == EntityType::FALLING_BLOCK
                || (can_walk_on_reader(entity, reader)?
                    && context.is_above(&shapes::block(), pos, false)
                    && !context.is_descending())
            {
                return Ok(shapes::block());
            }
        }
        Ok(shapes::empty())
    }
    fn entity_inside_collision_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        entity: EntityRef,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        let context = reader.collision_context(Some(entity), false)?;
        let shape = self.collision_shape(state, pos, &context, reader)?;
        Ok(if shape.is_empty() {
            shapes::block()
        } else {
            shape
        })
    }
    fn pickup_block(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<Item>, LevelError> {
        level.set_block(pos, Block::AIR.default_state(), UpdateFlags::ALL_IMMEDIATE)?;
        match level {
            LevelAccessRef::Server(level) => level.emit(LevelEffect::LevelEvent {
                pos,
                id: 2001,
                data: i32::from(state.id()),
            }),
            LevelAccessRef::Worldgen(_) => { /* WorldGenRegion#levelEvent is a no-op. */ }
        }
        Ok(Some(Item::POWDER_SNOW_BUCKET))
    }
}

#[cfg(test)]
mod tests;
