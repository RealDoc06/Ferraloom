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
    pub(super) fn live_level_in(&self, dimension: &protocol::Identifier) -> Result<&LiveLevel> {
        self.live_levels()
            .find(|level| level.dimension == dimension.as_str())
            .with_context(|| format!("Unknown dimension {dimension}"))
    }
    pub(super) fn live_level_in_mut(
        &mut self,
        dimension: &protocol::Identifier,
    ) -> Result<&mut LiveLevel> {
        self.live_levels_mut()
            .find(|level| level.dimension == dimension.as_str())
            .with_context(|| format!("Unknown dimension {dimension}"))
    }
    pub(super) fn level_keys(&self) -> Vec<protocol::Identifier> {
        self.live_levels()
            .map(|level| level.dimension.parse().expect("registered dimension key"))
            .collect()
    }
    pub(super) fn player_connections_in(
        &self,
        dimension: &protocol::Identifier,
    ) -> Result<Vec<ConnectionId>> {
        let mut connections = Vec::new();
        for &id in &self.players {
            let Some(connection) = self.connections.get(&id) else {
                continue;
            };
            if connection.close {
                continue;
            }
            let Phase::Play(entity) = connection.phase else {
                continue;
            };
            let current_dimension = self
                .level_for_entity(entity)
                .map_err(game_error)?
                .dimension_key()
                .map_err(game_error)?;
            if current_dimension.namespace() == dimension.namespace()
                && current_dimension.path() == dimension.path()
            {
                connections.push(id);
            }
        }
        Ok(connections)
    }
    pub(super) fn broadcast_in_level(
        &mut self,
        dimension: &protocol::Identifier,
        packet: cb::Packet,
    ) -> Result<()> {
        for id in self.player_connections_in(dimension)? {
            self.connections
                .get_mut(&id)
                .expect("selected connection")
                .push(packet.clone());
        }
        Ok(())
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

/// Port of ServerGamePacketListenerImpl#tickPlayer after its network bookkeeping.
/// All game reborrows resolve within this connection's one canonical Level.
pub(super) fn tick_player_connection(
    entity: EntityRef,
    level: &mut Level,
    tick_rate: &ServerTickRateManager,
) -> Result<()> {
    let player = level
        .entity_mut::<ServerPlayer>(entity)
        .map_err(game_error)?;
    player.reset_position();
    player.old_position = player.position;
    ferraloom_game::entity::player::player_do_tick_with_tick_rate(entity, level, tick_rate)
        .map_err(game_error)?;
    let player = level.entity::<ServerPlayer>(entity).map_err(game_error)?;
    let (position, yaw, pitch) = (player.first_good, player.yaw, player.pitch);
    ferraloom_game::entity::teleport::abs_snap_to(entity, position, yaw, pitch, level)
        .map_err(game_error)?;
    level
        .entity_mut::<ServerPlayer>(entity)
        .map_err(game_error)?
        .finish_connection_tick();
    Ok(())
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
