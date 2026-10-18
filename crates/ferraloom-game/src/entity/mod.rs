use ferraloom_data::EntityType;

use crate::level::{Level, LevelError};

pub mod animal;
pub mod player;

/// Server-lifetime identity, independent of storage location and network ID.
/// The server allocator must never reuse a value, including after removal.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EntityRef(pub u64);

impl EntityRef {
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
mod lifecycle;
pub use lifecycle::{common_tick, eject_passengers, position_rider, start_riding, stop_riding};
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

    /// Subset of Entity#baseTick: finish the first tick. ServerLevel owns commonTick.
    fn tick(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        level.entity_state_mut(entity)?.as_entity_mut().first_tick = false;
        Ok(())
    }

    fn hurt_server(
        &self,
        entity: EntityRef,
        level: &mut Level,
        source: DamageSource,
        damage: f32,
    ) -> Result<bool, LevelError> {
        hurt_unarmored(entity, level, source, damage)
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
