//! Recursive entity storage codecs. Owned construction needs shared Level reads;
//! its caller controls root/passenger registration at the native load call site.
use super::{EntityLoadError, vector_tag};
use crate::{
    entity::{EntityData, EntityRef, EntityState, item::falling_block::FallingBlockLoadError},
    level::{Level, LevelError},
};
use ferraloom_data::EntityType;
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_util::Vec3;

#[derive(Debug)]
pub enum EntityPersistenceError {
    Level(LevelError),
    Base(EntityLoadError),
    MissingType,
    UnsupportedType(String),
}

#[cfg(test)]
mod read_tests;
impl From<LevelError> for EntityPersistenceError {
    fn from(error: LevelError) -> Self {
        Self::Level(error)
    }
}
impl From<EntityLoadError> for EntityPersistenceError {
    fn from(error: EntityLoadError) -> Self {
        Self::Base(error)
    }
}
impl std::fmt::Display for EntityPersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for EntityPersistenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Level(error) => Some(error),
            Self::Base(error) => Some(error),
            _ => None,
        }
    }
}

/// Port of EntityType#loadEntitiesRecursive's pre-order constructor/stream order.
/// The resulting owned states already contain riding links when added to Level.
pub(crate) fn load_entity_tree(
    level: &Level,
    tag: CompoundTag,
    loaded: &mut Vec<(EntityRef, EntityState)>,
) -> Result<EntityRef, EntityPersistenceError> {
    let passengers = tag.get_list("Passengers").cloned().unwrap_or_default();
    let name = tag
        .get_string("id")
        .ok_or(EntityPersistenceError::MissingType)?
        .to_string_lossy();
    let name = ferraloom_protocol::Identifier::parse(&name)
        .map_err(|_| EntityPersistenceError::UnsupportedType(name))?;
    let name = name.as_str().to_owned();
    let kind = EntityType::from_name(&name)
        .ok_or_else(|| EntityPersistenceError::UnsupportedType(name.clone()))?;
    let mut state = super::create_entity_state(level, kind)?;
    match &mut state {
        EntityState::Cow(cow) => {
            cow.as_living_mut().load_nbt(tag.clone())?;
            cow.read_cold_data(&tag, level)?;
        }
        EntityState::Item(item) => item.load_nbt(tag)?,
        EntityState::PrimedTnt(tnt) => tnt.load_nbt(tag)?,
        EntityState::FallingBlock(falling) => {
            falling.load_nbt(tag, level).map_err(|error| match error {
                FallingBlockLoadError::Base(error) => EntityPersistenceError::Base(error),
                FallingBlockLoadError::Level(error) => EntityPersistenceError::Level(error),
            })?;
        }
        EntityState::ExperienceOrb(orb) => orb.load_nbt(tag)?,
        EntityState::Player(_) => unreachable!("players have no NBT factory"),
    }
    let entity = level.next_entity_ref();
    let index = loaded.len();
    loaded.push((entity, state));
    for tag in passengers.iter().filter_map(Tag::as_compound) {
        let known = tag
            .get_string("id")
            .and_then(|name| ferraloom_protocol::Identifier::parse(&name.to_string_lossy()).ok())
            .and_then(|name| EntityType::from_name(name.as_str()));
        if known.is_none() {
            eprintln!("Skipping passenger with unknown or missing id while loading entity chunk");
            continue;
        }
        let passenger = match load_entity_tree(level, tag.clone(), loaded) {
            Ok(passenger) => passenger,
            Err(error @ EntityPersistenceError::Base(_)) => {
                eprintln!("Skipping invalid passenger while loading entity chunk: {error}");
                continue;
            }
            Err(error) => return Err(error),
        };
        // Every currently loaded factory inherits couldAcceptPassenger=true;
        // Entity#startRiding does not reject a removed mount.
        let passenger_state = loaded.iter_mut().find(|(id, _)| *id == passenger).unwrap();
        passenger_state.1.as_entity_mut().vehicle.set(Some(entity));
        loaded[index].1.as_entity_mut().passengers.push(passenger);
    }
    Ok(entity)
}

impl Level {
    /// Port of EntityAccess#shouldBeSaved. Single-player mounts are persisted by
    /// the player's root-vehicle record rather than by entity chunk storage.
    pub fn entity_should_be_saved(&self, entity: EntityRef) -> Result<bool, LevelError> {
        let base = self.entity::<EntityData>(entity)?;
        Ok(base.entity_type != EntityType::PLAYER
            && base
                .removal_reason
                .is_none_or(|reason| reason.should_save())
            && base.vehicle.get().is_none()
            && (base.passengers.is_empty() || entity.count_player_passengers(self)? != 1))
    }

    /// Port of Entity#save; passengers are serialized only through their vehicle.
    pub fn save_entity(&self, entity: EntityRef) -> Result<Option<CompoundTag>, LevelError> {
        if self.entity::<EntityData>(entity)?.vehicle.get().is_some() {
            return Ok(None);
        }
        self.save_entity_as_passenger(entity)
    }

    fn save_entity_as_passenger(
        &self,
        entity: EntityRef,
    ) -> Result<Option<CompoundTag>, LevelError> {
        let state = self.entity_state(entity)?;
        let base = state.as_entity();
        if matches!(state, EntityState::Player(_))
            || base
                .removal_reason
                .is_some_and(|reason| !reason.should_save())
        {
            return Ok(None);
        }
        let mut tag = self.save_entity_without_id(entity)?;
        tag.insert("id", Tag::String(state.entity_type().name().into()));
        Ok(Some(tag))
    }

    /// Port of NbtPredicate#getEntityTagToCompare. Save eligibility is not
    /// consulted, including for removed objects, passengers and players.
    pub fn entity_nbt_to_compare(&self, entity: EntityRef) -> Result<CompoundTag, LevelError> {
        use ferraloom_protocol::component::Persistent;
        let mut tag = self.save_entity_without_id(entity)?;
        if let EntityState::Player(player) = self.entity_state(entity)? {
            tag.remove("SelectedItem");
            let selected = player.inventory.selected_item();
            if !selected.is_empty() {
                tag.insert(
                    "SelectedItem",
                    selected
                        .to_nbt()
                        .map_err(|_| LevelError::Inventory("invalid selected item NBT"))?,
                );
            }
        }
        Ok(tag)
    }

    /// One full-subclass dispatcher serves both comparison NBT and persistent
    /// saveAsPassenger, so the former does not inherit the latter's filters.
    fn save_entity_without_id(&self, entity: EntityRef) -> Result<CompoundTag, LevelError> {
        let state = self.entity_state(entity)?;
        let base = state.as_entity();
        let mut tag = match state {
            EntityState::Cow(cow) => {
                let mut tag = base.save_without_id();
                cow.as_living().add_additional_save_data(
                    &mut tag,
                    self,
                    crate::entity::LivingEntity::absorption_amount,
                )?;
                cow.add_cow_save_data(&mut tag, self)?;

                tag
            }
            EntityState::PrimedTnt(tnt) => tnt.save_without_id(self)?,
            EntityState::Item(item) => item.save_without_id(self)?,
            EntityState::FallingBlock(block) => block.save_without_id(),
            EntityState::ExperienceOrb(orb) => orb.save_without_id(),
            EntityState::Player(player) => player.save_without_id(self)?,
        };
        if let Some(vehicle) = base.vehicle.get() {
            let mount = self.entity::<EntityData>(vehicle)?;
            tag.insert(
                "Pos",
                vector_tag(Vec3::new(
                    mount.position.x,
                    base.position.y,
                    mount.position.z,
                )),
            );
        }
        let mut passengers = ListTag::default();
        for &passenger in &base.passengers {
            if let Some(tag) = self.save_entity_as_passenger(passenger)? {
                passengers.0.push(Tag::Compound(tag));
            }
        }
        if !passengers.is_empty() {
            tag.insert("Passengers", Tag::List(passengers));
        }
        Ok(tag)
    }
}
