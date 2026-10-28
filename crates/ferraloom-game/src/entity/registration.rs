//! The sole built-in EntityType constructor/behavior dispatcher.
//!
//! Worked example: COW maps to Cow::with_level(parent, level) and CowBehavior.
//! The leaf defines its metadata after constructing its embedded parent, owns
//! only subclass NBT, and uses the existing AttributeMap supplier for its actual
//! EntityType. The root EntityState::Cow projections borrow that parent chain;
//! persistence::live and persistence::tree invoke its native save/load phases.
//! Cube-like subclasses can have an explicit pre-parent read: registration must
//! preserve their source order rather than assume all subclasses load last.
//!
//! Adding a family means one typed EntityState variant/projection and one entry
//! here, plus its native persistence phases. There is no second leaf registry.
//! Unported factories reject before EntityData construction (IDs/UUID/RNG).
use super::{Entity, EntityData, EntityState};
use crate::level::{Level, LevelError};
use ferraloom_data::EntityType;

type Factory = fn(EntityData, &Level) -> Result<EntityState, LevelError>;

pub(crate) struct Registration {
    /// PLAYER has native createNothing, while unsupported types have no entry.
    pub factory: Option<Factory>,
    pub behavior: &'static dyn Entity,
}

pub(crate) fn for_type(kind: EntityType) -> Option<Registration> {
    use super::{animal::Cow, experience_orb, item, player};
    Some(match kind {
        EntityType::COW => Registration {
            factory: Some(|parent, level| {
                Ok(EntityState::Cow(Box::new(Cow::with_level(parent, level)?)))
            }),
            behavior: &super::animal::CowBehavior,
        },
        EntityType::PLAYER => Registration {
            factory: None,
            behavior: &player::PlayerBehavior,
        },
        EntityType::ITEM => Registration {
            factory: Some(|parent, _| {
                Ok(EntityState::Item(Box::new(item::ItemEntity::with_parent(
                    parent,
                ))))
            }),
            behavior: &item::ItemEntityBehavior,
        },
        EntityType::TNT => Registration {
            factory: Some(|parent, _| {
                Ok(EntityState::PrimedTnt(Box::new(
                    item::PrimedTnt::with_parent(parent),
                )))
            }),
            behavior: &item::PrimedTntBehavior,
        },
        EntityType::FALLING_BLOCK => Registration {
            factory: Some(|parent, _| {
                Ok(EntityState::FallingBlock(Box::new(
                    item::FallingBlockEntity::with_parent(parent),
                )))
            }),
            behavior: &item::FallingBlockBehavior,
        },
        EntityType::EXPERIENCE_ORB => Registration {
            factory: Some(|parent, _| {
                Ok(EntityState::ExperienceOrb(Box::new(
                    experience_orb::ExperienceOrb::with_parent(parent),
                )))
            }),
            behavior: &experience_orb::ExperienceOrbBehavior,
        },
        _ => return None,
    })
}
