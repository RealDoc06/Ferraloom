use super::ServerScoreboard;

/// Server-owned gameplay state lent to a Level for a complete execution phase.
/// Port of MinecraftServer's global gameplay ownership, including ServerScoreboard.
/// Phase 5 parallel regions acquire this owner through the exclusive lane.
#[derive(Debug, Default)]
pub struct ServerShared {
    pub scoreboard: ServerScoreboard,
    pub ops: crate::commands::permissions::ServerOpList,
    pub weather: crate::level::weather::WeatherData,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        level::{Level, LevelContext, LevelError, Region},
    };

    #[test]
    fn lent_owner_is_visible_to_nested_calls_and_returns_without_copying() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
        assert!(matches!(
            level.shared(),
            Err(LevelError::MissingServerShared)
        ));
        assert!(matches!(
            level.shared_mut(),
            Err(LevelError::MissingServerShared)
        ));
        let shared = Box::<ServerShared>::default();
        let identity = std::ptr::from_ref(shared.as_ref());
        level.install_shared(shared);
        fn nested(level: &mut Level) {
            level
                .shared_mut()
                .unwrap()
                .scoreboard
                .add_player_team("red");
            level
                .shared_mut()
                .unwrap()
                .scoreboard
                .add_player_to_team("Doc", "red")
                .unwrap();
        }
        nested(&mut level);
        assert_eq!(
            level
                .shared()
                .unwrap()
                .scoreboard
                .scoreboard()
                .get_players_team("Doc")
                .unwrap()
                .name(),
            "red"
        );
        let shared = level.take_shared().unwrap();
        assert_eq!(std::ptr::from_ref(shared.as_ref()), identity);
        assert_eq!(
            shared
                .scoreboard
                .scoreboard()
                .get_players_team("Doc")
                .unwrap()
                .name(),
            "red"
        );
        assert!(matches!(
            level.shared(),
            Err(LevelError::MissingServerShared)
        ));
        assert!(matches!(
            level.take_shared(),
            Err(LevelError::MissingServerShared)
        ));
    }
}
