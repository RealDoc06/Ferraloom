//! Projectile's embedded parent and owner reads. Concrete projectile factories,
//! movement and hit dispatch are separate ports; no abstract factory is installed.
use super::{EntityData, EntityDataView, EntityRef, EntityReference, EntityState};
use crate::level::{Level, LevelError};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::component::{AdventureModePredicate, Persistent};

/// Port of Projectile's canonical state, embedded once by each future concrete
/// projectile. Owner caches reuse EntityReference and its existing lifetime trace.
#[derive(Debug)]
pub struct Projectile {
    pub parent: EntityData,
    owner: Option<EntityReference<EntityData>>,
    pub can_break: Option<AdventureModePredicate>,
    pub left_owner: bool,
    pub left_owner_checked: bool,
    pub has_been_shot: bool,
    pub last_deflected_by: Option<EntityRef>,
}
/// Cold persistence failures keep the required world read and component codec
/// errors distinct; future concrete serializers propagate the original cause.
#[derive(Debug)]
pub enum ProjectileSaveError {
    Level(LevelError),
    Component(ferraloom_protocol::Error),
}
impl std::fmt::Display for ProjectileSaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Level(error) => write!(f, "{error}"),
            Self::Component(error) => write!(f, "{error}"),
        }
    }
}
impl std::error::Error for ProjectileSaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Level(error) => error,
            Self::Component(error) => error,
        })
    }
}
impl From<LevelError> for ProjectileSaveError {
    fn from(error: LevelError) -> Self {
        Self::Level(error)
    }
}
impl Projectile {
    pub fn with_parent(parent: EntityData) -> Self {
        Self {
            parent,
            owner: None,
            can_break: None,
            left_owner: false,
            left_owner_checked: false,
            has_been_shot: false,
            last_deflected_by: None,
        }
    }
    /// Port of Projectile#setOwner(Entity), including clearing an existing owner.
    pub fn set_owner(&mut self, owner: Option<EntityRef>, level: &Level) -> Result<(), LevelError> {
        self.owner = owner
            .map(|entity| EntityReference::of(entity, level))
            .transpose()?;
        Ok(())
    }
    /// Port of Projectile#getOwner. A cached hidden owner remains readable;
    /// observing removal changes the reference to UUID lookup before retrying.
    pub fn get_owner(&mut self, level: &Level) -> Result<Option<EntityRef>, LevelError> {
        self.owner
            .as_mut()
            .map(|owner| owner.resolve(level))
            .transpose()
            .map(Option::flatten)
    }
    /// Port of Projectile#getEffectSource for an owned preparation receiver.
    pub fn get_effect_source(
        &mut self,
        entity: EntityRef,
        level: &Level,
    ) -> Result<EntityRef, LevelError> {
        Ok(self.get_owner(level)?.unwrap_or(entity))
    }
    pub fn owner_uuid(&self, level: &Level) -> Result<Option<ferraloom_util::Uuid>, LevelError> {
        self.owner.map(|owner| owner.uuid(level)).transpose()
    }
    /// Projectile#restoreFrom copies the owner reference after Entity's parent
    /// restore; transient flight state is not copied by this subclass override.
    pub fn restore_owner_from(&mut self, old: &Self) {
        self.owner = old.owner;
    }
    pub fn read_additional_save_data(&mut self, tag: &CompoundTag) {
        self.owner = super::living::damage_state::read_reference(tag, "Owner");
        self.left_owner = tag.get_boolean_or("LeftOwner", false);
        self.has_been_shot = tag.get_boolean_or("HasBeenShot", false);
        self.can_break = tag
            .get("can_break")
            .and_then(|tag| AdventureModePredicate::from_nbt(tag).ok());
    }
    pub fn add_additional_save_data(
        &self,
        tag: &mut CompoundTag,
        level: &Level,
    ) -> Result<(), ProjectileSaveError> {
        tag.remove("Owner");
        tag.remove("LeftOwner");
        tag.remove("can_break");
        if let Some(uuid) = self.owner_uuid(level)? {
            tag.insert("Owner", Tag::IntArray(uuid.to_int_array().to_vec()));
        }
        if self.left_owner {
            tag.insert("LeftOwner", Tag::Byte(1));
        }
        tag.insert("HasBeenShot", Tag::Byte(i8::from(self.has_been_shot)));
        if let Some(predicate) = &self.can_break {
            tag.insert(
                "can_break",
                predicate.to_nbt().map_err(ProjectileSaveError::Component)?,
            );
        }
        Ok(())
    }
    /// Include this parent in the concrete dispatcher's ordinary trace walk.
    pub fn trace_entity_refs(&self, refs: &mut Vec<EntityRef>) {
        super::trace_base_entity_refs(&self.parent, refs);
        refs.extend(self.owner.and_then(EntityReference::cached_entity));
        refs.extend(self.last_deflected_by);
    }
}
impl EntityDataView for Projectile {
    fn view(state: &EntityState) -> Option<&Self> {
        state.as_projectile()
    }
    fn view_mut(state: &mut EntityState) -> Option<&mut Self> {
        state.as_projectile_mut()
    }
}
/// Short-borrow Projectile#getOwner for a canonical concrete receiver.
pub fn get_owner(entity: EntityRef, level: &mut Level) -> Result<Option<EntityRef>, LevelError> {
    let mut reference = level.entity::<Projectile>(entity)?.owner;
    let owner = reference
        .as_mut()
        .map(|owner| owner.resolve(level))
        .transpose()?
        .flatten();
    level.entity_mut::<Projectile>(entity)?.owner = reference;
    Ok(owner)
}

#[cfg(test)]
mod tests;
