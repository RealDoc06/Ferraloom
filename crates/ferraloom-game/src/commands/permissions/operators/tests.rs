use super::*;
use crate::{
    Behaviors,
    commands::CommandSourceStack,
    entity::{EntityState, player::GameType},
    level::{LevelContext, Region},
};
use ferraloom_util::Vec3;

#[derive(Deserialize)]
struct Vector {
    entry: Option<serde_json::Value>,
    creative: bool,
    instabuild: bool,
    is_op: bool,
    levels: Vec<bool>,
    selectors: bool,
    can_use_game_master_blocks: bool,
    saved: Option<serde_json::Value>,
}
fn player_level(instabuild: bool, creative: bool) -> (Level, EntityRef) {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    let entity = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    let mut player = ServerPlayer::new(
        Uuid::new(1, 2),
        "metadata".into(),
        1,
        Vec3::ZERO,
        if creative {
            GameType::Creative
        } else {
            GameType::Survival
        },
    );
    player.abilities.instabuild = instabuild;
    level
        .add_entity(entity, EntityState::Player(Box::new(player)))
        .unwrap();
    (level, entity)
}
#[test]
fn dedicated_permissions_entry_defaults_and_independent_abilities_match_java() {
    let vectors: Vec<Vector> = serde_json::from_str(include_str!(
        "../../vanilla/player-permissions-vectors.json"
    ))
    .unwrap();
    assert_eq!(vectors.len(), 36);
    for vector in vectors {
        let (mut level, entity) = player_level(vector.instabuild, vector.creative);
        let mut entries = Vec::new();
        if let Some(entry) = vector.entry {
            let entry: ServerOpListEntry = serde_json::from_value(entry).unwrap();
            assert_eq!(serde_json::to_value(&entry).unwrap(), vector.saved.unwrap());
            entries.push(entry);
        }
        let shared = crate::ServerShared {
            ops: ServerOpList::from_entries(entries),
            ..Default::default()
        };
        level.install_shared(Box::new(shared));
        let permissions = level.player_permissions(entity).unwrap();
        assert_eq!(
            (0..5)
                .map(|level| permissions.has_level(level))
                .collect::<Vec<_>>(),
            vector.levels
        );
        assert_eq!(
            permissions.has(&Permission::Atom(Permission::ENTITY_SELECTORS.into())),
            vector.selectors
        );
        assert_eq!(
            level.shared().unwrap().ops.contains(Uuid::new(1, 2)),
            vector.is_op
        );
        assert_eq!(
            level.can_use_game_master_blocks(entity).unwrap(),
            vector.can_use_game_master_blocks
        );
    }
}
#[test]
fn same_phase_ops_mutations_are_live_and_command_sources_keep_captured_permissions() {
    let (mut level, entity) = player_level(true, false);
    assert!(matches!(
        level.player_permissions(entity),
        Err(LevelError::MissingServerShared)
    ));
    assert!(matches!(
        level.can_use_game_master_blocks(entity),
        Err(LevelError::MissingServerShared)
    ));
    level.install_shared(Box::default());
    let profile = GameProfile {
        id: Uuid::new(1, 2),
        name: "metadata".into(),
        properties: vec![],
    };
    let captured = CommandSourceStack::server(0, Vec3::ZERO)
        .with_permission(level.player_permissions(entity).unwrap());
    level
        .shared_mut()
        .unwrap()
        .ops
        .set_operator(&profile, Some(PermissionLevel::Gamemasters));
    assert!(level.can_use_game_master_blocks(entity).unwrap());
    assert!(!captured.permissions.has(&Permission::COMMANDS_GAMEMASTER));
    let current = CommandSourceStack::server(0, Vec3::ZERO)
        .with_permission(level.player_permissions(entity).unwrap());
    assert!(current.permissions.has(&Permission::COMMANDS_GAMEMASTER));
    level.shared_mut().unwrap().ops.set_operator(&profile, None);
    assert!(!level.can_use_game_master_blocks(entity).unwrap());
    assert!(current.permissions.has(&Permission::COMMANDS_GAMEMASTER));
    let (level, entity) = player_level(false, true);
    assert!(!level.can_use_game_master_blocks(entity).unwrap());
}
