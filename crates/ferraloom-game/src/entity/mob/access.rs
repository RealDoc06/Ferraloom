//! Mob's live target/malus reads and NodeEvaluator's canonical receiver adapter.
use crate::{
    entity::{
        Entity, EntityData, EntityRef, EntityState, LivingEntity, Mob,
        ai::pathfinder::{MobAccess, PathType},
        player::GameType,
    },
    level::{Level, LevelError},
};
use ferraloom_data::{EntityType, FluidState};
use ferraloom_util::{Aabb, Vec3, mth, random::RandomSource};

/// Port of LivingEntity#canBeSeenAsEnemy's parent body. Player adds its
/// independent abilities.invulnerable gate before entering this body.
pub(crate) fn can_be_seen_as_enemy_parent<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &Level,
) -> Result<bool, LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    Ok(!base.permanently_invulnerable
        && base.invulnerable_time <= 0
        && behavior.can_be_seen_by_anyone(entity, level)?)
}
/// Port of LivingEntity#canAttack and Mob's GHAST exclusion.
pub fn can_attack(entity: EntityRef, target: EntityRef, level: &Level) -> Result<bool, LevelError> {
    level.entity::<LivingEntity>(entity)?;
    let target_data = level.entity::<LivingEntity>(target)?;
    if level.entity_state(entity)?.as_mob().is_some()
        && target_data.entity_type == EntityType::GHAST
    {
        return Ok(false);
    }
    if matches!(level.entity_state(target)?, EntityState::Player(_))
        && level.difficulty() == ferraloom_protocol::play::Difficulty::Peaceful
    {
        return Ok(false);
    }
    level
        .entity_behavior(target)?
        .can_be_seen_as_enemy(target, level)
}
/// Port of Mob#asValidTarget; filtering never rewrites the stored target on read.
fn as_valid_target(
    entity: EntityRef,
    target: Option<EntityRef>,
    level: &Level,
) -> Result<Option<EntityRef>, LevelError> {
    let Some(target) = target else {
        return Ok(None);
    };
    level.entity::<LivingEntity>(target)?;
    if let EntityState::Player(player) = level.entity_state(target)?
        && (player.game_mode == GameType::Creative
            || level.entity_behavior(target)?.is_spectator(target, level)?)
    {
        return Ok(None);
    }
    Ok(level
        .entity_behavior(entity)?
        .can_attack(entity, target, level)?
        .then_some(target))
}
pub fn get_target(entity: EntityRef, level: &Level) -> Result<Option<EntityRef>, LevelError> {
    as_valid_target(entity, level.entity::<Mob>(entity)?.target, level)
}
pub fn get_target_unchecked(
    entity: EntityRef,
    level: &Level,
) -> Result<Option<EntityRef>, LevelError> {
    Ok(level.entity::<Mob>(entity)?.target)
}
pub fn set_target(
    entity: EntityRef,
    target: Option<EntityRef>,
    level: &mut Level,
) -> Result<(), LevelError> {
    level.entity::<Mob>(entity)?;
    let target = as_valid_target(entity, target, level)?;
    level.entity_mut::<Mob>(entity)?.target = target;
    Ok(())
}
/// Port of Entity#getControlledVehicle: merely being mounted is insufficient.
pub fn controlled_vehicle(
    entity: EntityRef,
    level: &Level,
) -> Result<Option<EntityRef>, LevelError> {
    let Some(vehicle) = level.entity::<EntityData>(entity)?.vehicle.get() else {
        return Ok(None);
    };
    Ok((level
        .entity_behavior(vehicle)?
        .controlling_passenger(vehicle, level)?
        == Some(entity))
    .then_some(vehicle))
}
/// Port of Mob#getPathfindingMalus. Only a controlled Mob vehicle with the
/// native inheritance virtual enabled supplies this read; writes stay local.
pub fn get_pathfinding_malus(
    entity: EntityRef,
    path_type: PathType,
    level: &Level,
) -> Result<f32, LevelError> {
    level.entity::<Mob>(entity)?;
    let owner = if let Some(vehicle) = controlled_vehicle(entity, level)?
        && level.entity_state(vehicle)?.as_mob().is_some()
        && level
            .entity_behavior(vehicle)?
            .should_passengers_inherit_malus(vehicle, level)?
    {
        vehicle
    } else {
        entity
    };
    Ok(
        level.entity::<Mob>(owner)?.pathfinding_malus[path_type.id() as usize]
            .unwrap_or_else(|| path_type.malus()),
    )
}
pub fn set_pathfinding_malus(
    entity: EntityRef,
    path_type: PathType,
    cost: f32,
    level: &mut Level,
) -> Result<(), LevelError> {
    level.entity_mut::<Mob>(entity)?.pathfinding_malus[path_type.id() as usize] = Some(cost);
    Ok(())
}
/// Port of Mob#getMaxFallDistance; target validity, live health, difficulty and
/// float arithmetic precede the shared comfortable-distance floor.
pub fn max_fall_distance(entity: EntityRef, level: &Level) -> Result<i32, LevelError> {
    level.entity::<Mob>(entity)?;
    if level
        .entity_behavior(entity)?
        .get_target(entity, level)?
        .is_none()
    {
        return Ok(3);
    }
    let living = level.entity::<LivingEntity>(entity)?;
    let mut sacrifice = (living.health() - living.max_health() * 0.33f32) as i32;
    sacrifice = sacrifice.wrapping_sub((3 - (level.difficulty() as i32)).wrapping_mul(4));
    sacrifice = sacrifice.max(0);
    Ok(mth::floor(f64::from(sacrifice as f32 + 3.0)))
}

/// NodeEvaluator's single live Mob handle. The operation owner retains it;
/// every getter reads the canonical receiver anew and holds no callback borrow.
#[derive(Clone, Copy, Debug)]
pub struct CanonicalMobAccess {
    entity: EntityRef,
}
impl CanonicalMobAccess {
    pub fn new(entity: EntityRef, level: &Level) -> Result<Self, LevelError> {
        level.entity::<Mob>(entity)?;
        Ok(Self { entity })
    }
}
impl MobAccess for CanonicalMobAccess {
    fn position(&self, level: &Level) -> Result<Vec3, LevelError> {
        Ok(level.entity::<EntityData>(self.entity)?.position)
    }
    fn bounding_box(&self, level: &Level) -> Result<Aabb, LevelError> {
        Ok(level.entity::<EntityData>(self.entity)?.bounding_box)
    }
    fn dimensions(&self, level: &Level) -> Result<(f32, f32), LevelError> {
        let d = &level.entity::<EntityData>(self.entity)?.dimensions;
        Ok((d.width, d.height))
    }
    fn on_ground(&self, level: &Level) -> Result<bool, LevelError> {
        Ok(level.entity::<EntityData>(self.entity)?.on_ground)
    }
    fn is_in_water(&self, level: &Level) -> Result<bool, LevelError> {
        Ok(level.entity::<EntityData>(self.entity)?.was_touching_water)
    }
    fn is_in_floatable_fluid(&self, level: &Level) -> Result<bool, LevelError> {
        level
            .entity_behavior(self.entity)?
            .is_in_floatable_fluid(self.entity, level)
    }
    fn can_stand_on_fluid(&self, fluid: FluidState, level: &Level) -> Result<bool, LevelError> {
        level
            .entity_behavior(self.entity)?
            .can_stand_on_fluid(self.entity, fluid, level)
    }
    fn max_up_step(&self, level: &Level) -> Result<f32, LevelError> {
        level
            .entity_behavior(self.entity)?
            .max_up_step(self.entity, level)
    }
    fn max_fall_distance(&self, level: &Level) -> Result<i32, LevelError> {
        level
            .entity_behavior(self.entity)?
            .get_max_fall_distance(self.entity, level)
    }
    fn pathfinding_malus(&self, path_type: PathType, level: &Level) -> Result<f32, LevelError> {
        level
            .entity_behavior(self.entity)?
            .get_pathfinding_malus(self.entity, path_type, level)
    }
    fn set_pathfinding_malus(
        &mut self,
        path_type: PathType,
        value: f32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level.entity_behavior(self.entity)?.set_pathfinding_malus(
            self.entity,
            path_type,
            value,
            level,
        )
    }
    fn on_pathfinding_start(&mut self, level: &mut Level) -> Result<(), LevelError> {
        level
            .entity_behavior(self.entity)?
            .on_pathfinding_start(self.entity, level)
    }
    fn on_pathfinding_done(&mut self, level: &mut Level) -> Result<(), LevelError> {
        level
            .entity_behavior(self.entity)?
            .on_pathfinding_done(self.entity, level)
    }
    fn next_random_int(&mut self, bound: i32, level: &mut Level) -> Result<i32, LevelError> {
        Ok(level
            .entity_mut::<EntityData>(self.entity)?
            .random
            .next_int_bound(bound))
    }
    fn collision_source(&self) -> EntityRef {
        self.entity
    }
}
