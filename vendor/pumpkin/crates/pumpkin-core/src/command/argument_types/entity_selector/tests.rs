use super::*;
use crate::command::CommandSender;
use crate::command::argument_types::argument_type::ArgumentType;
use crate::command::argument_types::entity::EntityArgumentType;
use crate::command::string_reader::StringReader;
use crate::command::test::CommandFixture;
use crate::entity::r#type::from_type;
use arc_swap::ArcSwap;
use pumpkin_config::world::LevelConfig;
use pumpkin_data::dimension::Dimension;
use pumpkin_util::world_seed::Seed;
use pumpkin_world::{level::Level, world_info::LevelData};

fn parse_selector(input: &str) -> EntitySelector {
    EntityArgumentType::Entities
        .parse(&mut StringReader::new(input))
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn uuid_player_resolves_registered_arc_and_data_get_position() {
    let fixture = CommandFixture::new().await;
    let world = fixture.player.world();
    let uuid = fixture.player.gameprofile.id;
    fixture
        .player
        .get_entity()
        .set_pos(Vector3::new(12.75, 64.0, 0.0));
    assert!(world.get_entity_by_uuid(uuid).is_none());
    assert!(Arc::ptr_eq(
        &world.get_player_by_uuid(uuid).unwrap(),
        &fixture.player
    ));

    let source = CommandSender::Dummy.into_source(&fixture.server);
    let expected: Arc<dyn EntityBase> = fixture.player.clone();
    let all_players = parse_selector("@a").find_entities(&source).unwrap();
    assert_eq!(all_players.len(), 1);
    assert!(Arc::ptr_eq(&all_players[0], &expected));
    assert_eq!(
        fixture.execute(CommandSender::Dummy, "execute if entity @a"),
        1
    );
    assert_eq!(
        fixture.execute(CommandSender::Dummy, "data get entity @a[limit=1] Pos[0] 4"),
        51
    );

    let selected = parse_selector(&uuid.to_string())
        .find_entities(&source)
        .unwrap();
    assert_eq!(selected.len(), 1);
    assert!(Arc::ptr_eq(&selected[0], &expected));
    assert_eq!(
        fixture.execute(
            CommandSender::Dummy,
            &format!("data get entity {uuid} Pos[0]")
        ),
        12
    );
    assert_eq!(
        fixture.execute(
            CommandSender::Dummy,
            &format!("data get entity {uuid} Pos[0] 4")
        ),
        51
    );
    fixture.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn uuid_ordinary_entity_identity_precedence_and_missing_errors_are_preserved() {
    let fixture = CommandFixture::new().await;
    let world = fixture.player.world();
    let source = CommandSender::Dummy.into_source(&fixture.server);
    // Check both an ordinary UUID and a collision with the registered player.
    for uuid in [Uuid::from_u128(103), fixture.player.gameprofile.id] {
        let entity = from_type(
            &EntityType::PIG,
            Vector3::new(-4.5, 64.0, 0.0),
            &world,
            uuid,
        );
        world.add_entity_silent(entity.clone());
        let selected = parse_selector(&uuid.to_string())
            .find_entities(&source)
            .unwrap();
        assert_eq!(selected.len(), 1);
        assert!(Arc::ptr_eq(&selected[0], &entity));
        assert_eq!(
            fixture.execute(
                CommandSender::Dummy,
                &format!("data get entity {uuid} Pos[0]")
            ),
            -5
        );
    }

    let unknown_uuid = Uuid::from_u128(104);
    let unknown = parse_selector(&unknown_uuid.to_string());
    assert!(unknown.find_entities(&source).unwrap().is_empty());
    assert!(
        unknown
            .find_single_entity(&source)
            .is_err_and(|error| error.is(&entity::NO_ENTITIES_ERROR_TYPE))
    );
    let error = fixture
        .server
        .command_dispatcher
        .load()
        .execute_input(&format!("data get entity {unknown_uuid} Pos[0]"), &source)
        .unwrap_err();
    assert!(error.is(&entity::NO_ENTITIES_ERROR_TYPE));
    fixture.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn uuid_player_lookup_crosses_dimensions_and_keeps_ordinary_entity_priority() {
    let fixture = CommandFixture::new().await;
    let overworld = fixture.player.world();
    let directory = tempfile::tempdir().unwrap();
    let level = Level::from_root_folder(
        &LevelConfig::default(),
        directory.path().into(),
        0,
        Dimension::THE_NETHER,
    );
    level.shutdown().await.unwrap();
    let nether = Arc::new(World::load(
        level,
        Arc::new(ArcSwap::from_pointee(LevelData::default(Seed(0)))),
        Dimension::THE_NETHER,
        crate::block::registry::default_registry(),
        Arc::downgrade(&fixture.server),
        fixture.server.scoreboard.clone(),
    ));
    fixture
        .server
        .worlds
        .store(Arc::new(vec![nether.clone(), overworld.clone()]));
    let source = CommandSender::Dummy
        .into_source(&fixture.server)
        .with_world(nether.clone());
    let uuid = fixture.player.gameprofile.id;
    fixture
        .player
        .get_entity()
        .set_pos(Vector3::new(12.75, 64.0, 0.0));
    let selector = parse_selector(&uuid.to_string());
    let selected = selector.find_entities(&source).unwrap();
    let expected: Arc<dyn EntityBase> = fixture.player.clone();
    assert_eq!(selected.len(), 1);
    assert!(Arc::ptr_eq(&selected[0], &expected));
    assert_eq!(
        fixture
            .server
            .command_dispatcher
            .load()
            .execute_input(&format!("data get entity {uuid} Pos[0]"), &source)
            .unwrap(),
        12
    );

    // Preserve current Rust ordering: a later world's ordinary entity wins.
    // Duplicate-UUID precedence here is not a vanilla guarantee.
    let entity = from_type(
        &EntityType::PIG,
        Vector3::new(0.0, 64.0, 0.0),
        &nether,
        uuid,
    );
    nether.add_entity_silent(entity.clone());
    fixture
        .server
        .worlds
        .store(Arc::new(vec![overworld.clone(), nether]));
    let selected = selector.find_entities(&source).unwrap();
    assert_eq!(selected.len(), 1);
    assert!(Arc::ptr_eq(&selected[0], &entity));
    fixture.server.worlds.store(Arc::new(vec![overworld]));
    fixture.close().await;
}
