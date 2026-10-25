//! MinecraftServer's dimension lookup and canonical entity membership.
use super::*;
use crate::generation::runtime::LiveLevel;
use ferraloom_game::level::Level;

impl MinecraftServer {
    /// Port of MinecraftServer#getLevel: unknown dimensions are nullable.
    pub fn get_level(&self, dimension: &protocol::Identifier) -> Option<&Level> {
        self.live_levels()
            .find(|level| level.dimension == dimension.as_str())
            .map(|level| &level.level)
    }
    pub fn get_level_mut(&mut self, dimension: &protocol::Identifier) -> Option<&mut Level> {
        self.live_levels_mut()
            .find(|level| level.dimension == dimension.as_str())
            .map(|level| &mut level.level)
    }
    /// Port of MinecraftServer#getAllLevels, in the createLevels insertion order.
    pub fn get_all_levels(&self) -> impl Iterator<Item = &Level> {
        self.live_levels().map(|level| &level.level)
    }
    pub(super) fn live_levels(&self) -> impl Iterator<Item = &LiveLevel> {
        std::iter::once(&self.level).chain(self.additional_levels.iter())
    }
    pub(super) fn live_levels_mut(&mut self) -> impl Iterator<Item = &mut LiveLevel> {
        std::iter::once(&mut self.level).chain(self.additional_levels.iter_mut())
    }
    /// The reference records its birth allocator, not its current dimension.
    /// Resolve the canonical store, including retained removed receivers. A
    /// missing or multiply owned handle is an error, never an overworld fallback.
    pub fn level_for_entity(&self, entity: EntityRef) -> Result<&Level, LevelError> {
        let index = entity_level_index(self.get_all_levels(), entity)?;
        Ok(&self.live_levels().nth(index).expect("resolved level").level)
    }
    pub fn level_for_entity_mut(&mut self, entity: EntityRef) -> Result<&mut Level, LevelError> {
        let index = entity_level_index(self.get_all_levels(), entity)?;
        Ok(&mut self
            .live_levels_mut()
            .nth(index)
            .expect("resolved level")
            .level)
    }
}

fn entity_level_index<'a>(
    levels: impl Iterator<Item = &'a Level>,
    entity: EntityRef,
) -> Result<usize, LevelError> {
    let mut owner = None;
    for (index, level) in levels.enumerate() {
        match level.entity_state(entity) {
            Ok(_) => {
                if owner.replace(index).is_some() {
                    return Err(LevelError::DuplicateEntity(entity));
                }
            }
            Err(LevelError::MissingEntity(_)) => (),
            Err(error) => return Err(error),
        }
    }
    owner.ok_or(LevelError::MissingEntity(entity))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_game::{Behaviors, level::LevelContext};

    fn level() -> Level {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
        level.install_shared(Box::default());
        level
    }
    fn player() -> EntityState {
        EntityState::Player(Box::new(ServerPlayer::new(
            Uuid::new(1, 2),
            "DimensionPlayer".into(),
            1,
            Vec3::ZERO,
            GameType::Survival,
        )))
    }
    #[test]
    fn entity_membership_uses_the_owning_store_and_rejects_absence_and_ambiguity() {
        let mut overworld = level();
        let mut other = level();
        // Deliberately publish a handle allocated elsewhere: its birth does not
        // determine which canonical Level contains the entity now.
        let entity = overworld.next_entity_ref();
        other.add_entity(entity, player()).unwrap();
        assert_eq!(
            entity_level_index([&overworld, &other].into_iter(), entity),
            Ok(1)
        );
        let missing = overworld.next_entity_ref();
        assert_eq!(
            entity_level_index([&overworld, &other].into_iter(), missing),
            Err(LevelError::MissingEntity(missing))
        );
        overworld.add_entity(entity, player()).unwrap();
        assert_eq!(
            entity_level_index([&overworld, &other].into_iter(), entity),
            Err(LevelError::DuplicateEntity(entity))
        );
    }
}
