use ferraloom_data::EntityType;

use crate::level::{Level, LevelError};

pub mod animal;
pub mod player;

/// Server-lifetime identity, independent of storage location and network ID.
/// The server allocator must never reuse a value, including after removal.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EntityRef(pub u64);

impl EntityRef {
    pub fn count_player_passengers(self, level: &Level) -> Result<i32, LevelError> {
        count_player_passengers(self, level)
    }

    pub fn move_by(
        &mut self,
        level: &mut Level,
        mover: movement::MoverType,
        delta: ferraloom_util::Vec3,
    ) -> Result<(), LevelError> {
        let behavior = level.entity_behavior(*self)?;
        behavior.move_by(*self, level, mover, delta)
    }

    /// The mutable receiver is a call handle; canonical state stays in `level`.
    pub fn tick(&mut self, level: &mut Level) -> Result<(), LevelError> {
        let behavior = level.entity_behavior(*self)?;
        behavior.tick(*self, level)
    }

    pub fn hurt_server(
        &mut self,
        level: &mut Level,
        source: DamageSource,
        damage: f32,
    ) -> Result<bool, LevelError> {
        let behavior = level.entity_behavior(*self)?;
        behavior.hurt_server(*self, level, source, damage)
    }
}

mod base;
pub mod collision;
pub mod collision_context;
pub mod fluid;
pub mod inside_block;
mod lifecycle;
pub mod movement;
mod trace;
pub use lifecycle::{
    base_tick, common_tick, count_player_passengers, eject_passengers, position_rider,
    start_riding, stop_riding,
};
pub mod persistence;
pub mod synched_data;
pub use base::{EntityData, EntityDimensions, Movement, RemovalReason};

/// Port of LivingEntity's unarmored damage/cooldown state.
#[derive(Debug)]
pub struct LivingEntity {
    pub parent: EntityData,
    pub health: f32,
    pub last_hurt: f32,
    pub damage_cooldown_time: i32,
    pub hurt_time: i32,
    pub hurt_duration: i32,
}

/// Port of Mob's entity references. Goals/navigation are added here as ported.
#[derive(Debug)]
pub struct Mob {
    pub parent: LivingEntity,
    pub target: Option<EntityRef>,
}

/// Port of PathfinderMob. Navigation belongs to Mob in vanilla.
#[derive(Debug)]
pub struct PathfinderMob {
    pub parent: Mob,
}

/// Port of AgeableMob, which is between Animal and PathfinderMob in 26.3.
#[derive(Debug)]
pub struct AgeableMob {
    pub parent: PathfinderMob,
    pub age: i32,
}

/// Concrete stored subclasses. Parent accessors never copy canonical state.
#[derive(Debug)]
pub enum EntityState {
    Cow(animal::Cow),
    Player(Box<player::ServerPlayer>),
}

impl EntityState {
    pub fn entity_type(&self) -> EntityType {
        match self {
            Self::Cow(_) => EntityType::COW,
            Self::Player(_) => EntityType::PLAYER,
        }
    }

    pub fn as_mob(&self) -> Option<&Mob> {
        match self {
            Self::Cow(cow) => Some(cow.as_mob()),
            Self::Player(_) => None,
        }
    }

    pub fn as_mob_mut(&mut self) -> Option<&mut Mob> {
        match self {
            Self::Cow(cow) => Some(cow.as_mob_mut()),
            Self::Player(_) => None,
        }
    }

    pub fn as_living(&self) -> Option<&LivingEntity> {
        match self {
            Self::Player(p) => Some(&p.parent),
            _ => self.as_mob().map(|mob| &mob.parent),
        }
    }
    pub fn as_living_mut(&mut self) -> Option<&mut LivingEntity> {
        match self {
            Self::Player(p) => Some(&mut p.parent),
            _ => self.as_mob_mut().map(|mob| &mut mob.parent),
        }
    }
    pub fn as_entity(&self) -> &EntityData {
        match self {
            Self::Cow(cow) => &cow.as_living().parent,
            Self::Player(p) => &p.parent.parent,
        }
    }
    pub fn as_entity_mut(&mut self) -> &mut EntityData {
        match self {
            Self::Cow(cow) => &mut cow.as_living_mut().parent,
            Self::Player(p) => &mut p.parent.parent,
        }
    }
}

/// Typed projections are borrowed views, not casts or runtime downcasts.
pub trait EntityDataView: Sized {
    fn view(state: &EntityState) -> Option<&Self>;
    fn view_mut(state: &mut EntityState) -> Option<&mut Self>;
}

impl EntityDataView for animal::Cow {
    fn view(state: &EntityState) -> Option<&Self> {
        match state {
            EntityState::Cow(cow) => Some(cow),
            EntityState::Player(_) => None,
        }
    }
    fn view_mut(state: &mut EntityState) -> Option<&mut Self> {
        match state {
            EntityState::Cow(cow) => Some(cow),
            EntityState::Player(_) => None,
        }
    }
}

impl EntityDataView for EntityData {
    fn view(state: &EntityState) -> Option<&Self> {
        Some(state.as_entity())
    }
    fn view_mut(state: &mut EntityState) -> Option<&mut Self> {
        Some(state.as_entity_mut())
    }
}
impl EntityDataView for LivingEntity {
    fn view(state: &EntityState) -> Option<&Self> {
        state.as_living()
    }
    fn view_mut(state: &mut EntityState) -> Option<&mut Self> {
        state.as_living_mut()
    }
}
/// Embedded-parent projection; each struct dereferences only to its immediate parent.
impl std::ops::Deref for LivingEntity {
    type Target = EntityData;
    fn deref(&self) -> &Self::Target {
        &self.parent
    }
}
impl std::ops::DerefMut for LivingEntity {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.parent
    }
}

/// Narrow source used by the unarmored damage primitive; tags grow with ports.
#[derive(Clone, Copy, Debug, Default)]
pub struct DamageSource {
    pub attacker: Option<EntityRef>,
    pub bypasses_cooldown: bool,
}

/// Vanilla virtual methods, separated from canonical embedded-parent state.
///
/// Dispatch objects are stateless. Never hold a state borrow across a world call.
pub trait Entity: Sync {
    fn fire_immune_ticks(&self) -> i32 {
        0
    }
    fn apply_effects_from_blocks(
        &self,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let movements = inside_block::collect_movements(entity, level)?;
        inside_block::apply_effects(self, entity, &movements, level)
    }
    fn can_freeze(&self, _entity: EntityRef, _level: &Level) -> Result<bool, LevelError> {
        Ok(true)
    }
    fn should_play_lava_hurt_sound(
        &self,
        _entity: EntityRef,
        _level: &Level,
    ) -> Result<bool, LevelError> {
        Ok(true)
    }
    fn on_inside_block(
        &self,
        _entity: EntityRef,
        _state: ferraloom_data::BlockState,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn move_by(
        &self,
        entity: EntityRef,
        level: &mut Level,
        mover: movement::MoverType,
        delta: ferraloom_util::Vec3,
    ) -> Result<(), LevelError> {
        movement::move_entity(self, entity, mover, delta, level)
    }
    fn max_up_step(&self, entity: EntityRef, level: &Level) -> Result<f32, LevelError> {
        let state = level.entity_state(entity)?;
        if state.as_living().is_none() {
            return Ok(0.0);
        }
        Ok(state
            .entity_type()
            .data()
            .attributes
            .iter()
            .find(|(id, _)| *id == ferraloom_data::registries::attribute::Id::STEP_HEIGHT)
            .map_or(0.6, |(_, value)| *value as f32))
    }
    fn maybe_back_off_from_edge(
        &self,
        _entity: EntityRef,
        _level: &Level,
        delta: ferraloom_util::Vec3,
        _mover: movement::MoverType,
    ) -> Result<ferraloom_util::Vec3, LevelError> {
        Ok(delta)
    }
    fn is_local_instance_authoritative(
        &self,
        entity: EntityRef,
        level: &Level,
    ) -> Result<bool, LevelError> {
        Ok(!matches!(
            level.entity_state(entity)?,
            EntityState::Player(_)
        ))
    }
    fn is_horizontal_collision_minor(
        &self,
        _entity: EntityRef,
        _level: &Level,
        _movement: ferraloom_util::Vec3,
    ) -> Result<bool, LevelError> {
        Ok(false)
    }
    fn movement_emission(&self) -> movement::MovementEmission {
        movement::MovementEmission::All
    }
    fn bounciness(&self, _entity: EntityRef, _level: &Level) -> Result<f64, LevelError> {
        Ok(0.0)
    }
    fn default_gravity(&self) -> f64 {
        0.0
    }
    fn effective_gravity(&self, entity: EntityRef, level: &Level) -> Result<f64, LevelError> {
        Ok(
            if *level
                .entity::<EntityData>(entity)?
                .entity_data
                .get(synched_data::NO_GRAVITY)
            {
                0.0
            } else {
                self.default_gravity()
            },
        )
    }
    fn air_drag(&self) -> f32 {
        0.98
    }

    /// Port of LivingEntity#getLiquidCollisionShape's inherited empty result.
    fn liquid_collision_shape(
        &self,
        _entity: EntityRef,
        _level: &Level,
    ) -> Result<ferraloom_util::VoxelShape, LevelError> {
        Ok(ferraloom_util::shapes::empty())
    }

    /// Port of Entity#causeFallDamage's passenger propagation. LivingEntity
    /// overrides the damage calculation; the base entity itself takes none.
    fn cause_fall_damage(
        &self,
        entity: EntityRef,
        level: &mut Level,
        fall_distance: f64,
        damage_modifier: f32,
    ) -> Result<bool, LevelError> {
        let passengers = level.entity::<EntityData>(entity)?.passengers.clone();
        for passenger in passengers {
            let behavior = level.entity_behavior(passenger)?;
            behavior.cause_fall_damage(passenger, level, fall_distance, damage_modifier)?;
        }
        Ok(false)
    }

    fn can_be_collided_with(
        &self,
        _entity: EntityRef,
        _other: Option<EntityRef>,
        _level: &Level,
    ) -> Result<bool, LevelError> {
        Ok(false)
    }
    fn passenger_riding_position(
        &self,
        entity: EntityRef,
        _passenger: EntityRef,
        level: &Level,
    ) -> Result<ferraloom_util::Vec3, LevelError> {
        let base = level.entity::<EntityData>(entity)?;
        Ok(base
            .position
            .add_xyz(0.0, f64::from(base.dimensions.height), 0.0))
    }
    fn vehicle_attachment_point(
        &self,
        _entity: EntityRef,
        _vehicle: EntityRef,
        _level: &Level,
    ) -> Result<ferraloom_util::Vec3, LevelError> {
        Ok(ferraloom_util::Vec3::ZERO)
    }

    fn fluid_interaction_box(
        &self,
        entity: EntityRef,
        level: &Level,
    ) -> Result<Option<ferraloom_util::Aabb>, LevelError> {
        Ok(Some(
            level
                .entity::<EntityData>(entity)?
                .bounding_box
                .deflate(0.001, 0.001, 0.001),
        ))
    }
    fn is_pushed_by_fluid(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        Ok(match level.entity_state(entity)? {
            EntityState::Player(player) => !player.flying,
            _ => true,
        })
    }
    /// Port of Entity#tick; ServerLevel owns commonTick separately.
    fn tick(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        base_tick(self, entity, level)
    }

    fn hurt_server(
        &self,
        entity: EntityRef,
        level: &mut Level,
        source: DamageSource,
        damage: f32,
    ) -> Result<bool, LevelError> {
        if level.entity_state(entity)?.as_living().is_some() {
            hurt_unarmored(entity, level, source, damage)
        } else {
            Ok(false)
        }
    }
}

/// Port of LivingEntity#hurtServer's cooldown branch and unarmored health change.
///
/// Defense, effects, death, knockback and criteria remain separate porting work.
/// This helper must only be called after those earlier damage modifiers are applied.
pub fn hurt_unarmored(
    entity: EntityRef,
    level: &mut Level,
    source: DamageSource,
    damage: f32,
) -> Result<bool, LevelError> {
    let living = level
        .entity_state_mut(entity)?
        .as_living_mut()
        .ok_or(LevelError::WrongEntityType(entity))?;
    if living.parent.removed || living.health <= 0.0 {
        return Ok(false);
    }
    let damage = if damage < 0.0 { 0.0 } else { damage };
    let damage = if damage.is_finite() { damage } else { f32::MAX };
    let applied = if living.damage_cooldown_time > 10 && !source.bypasses_cooldown {
        if damage <= living.last_hurt {
            return Ok(false);
        }
        damage - living.last_hurt
    } else {
        living.damage_cooldown_time = 20;
        living.hurt_duration = 10;
        living.hurt_time = living.hurt_duration;
        damage
    };
    living.last_hurt = damage;
    living.health = (living.health - applied).max(0.0);
    Ok(true)
}

/// Built-in stateless dispatchers. Explicit behavior tables still override these.
pub fn vanilla_behavior(kind: EntityType) -> Option<&'static dyn Entity> {
    match kind {
        EntityType::COW => Some(&animal::CowBehavior),
        EntityType::PLAYER => Some(&player::PlayerBehavior),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
