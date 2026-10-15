//! Lifecycle tests use temporary worlds, no listener, no ticks and no external auth.
use super::*;
use crate::world::scoreboard::{NoTarget, ScoreboardObjective};
use pumpkin_nbt::{Nbt, NbtCompound, deserializer::NbtReadHelperJava};
use pumpkin_protocol::java::client::play::RenderType;
use pumpkin_util::world_seed::Seed;
use std::{io::Cursor, path::Path};

async fn server_at(path: &Path) -> Result<Arc<Server>, WorldInfoError> {
    let basic = BasicConfiguration {
        default_level_name: path.to_string_lossy().into_owned(),
        seed: Seed(0),
        allow_nether: true,
        allow_end: false,
        allow_chat_reports: false,
        use_favicon: false,
        ..BasicConfiguration::default()
    };
    let mut advanced = AdvancedConfiguration::default();
    advanced.networking.java.online_mode = false;
    advanced.networking.bedrock.online_mode = false;
    // Do not load usercache/configuration files from the test runner's cwd.
    let data = VanillaData {
        banned_ip_list: Default::default(),
        banned_player_list: Default::default(),
        operator_config: Default::default(),
        user_cache: Default::default(),
        whitelist_config: Default::default(),
    };
    Server::new(
        basic,
        advanced,
        TelemetryConfig::default(),
        data,
        Vec::new(),
    )
    .await
}

fn save_metadata(path: &Path) {
    AnvilLevelInfo
        .write_world_info(&LevelData::default(Seed(0)), path)
        .unwrap();
}

#[tokio::test]
async fn scoreboard_is_loaded_once_shared_by_dimensions_and_saved_by_server_lifecycle() {
    let folder = tempfile::tempdir().unwrap();
    save_metadata(folder.path());
    // Metadata helpers must never create or replace the server-owned file.
    let path = folder.path().join("data/minecraft/scoreboard.dat");
    assert!(!path.exists());
    let mut initial = Scoreboard::new();
    initial.add_objective(
        &NoTarget,
        ScoreboardObjective::new(
            "parity",
            TextComponent::text("Parity"),
            RenderType::Integer,
            None,
            "dummy",
        ),
    );
    initial.set_score_value(&NoTarget, "Before", "parity", -7);
    initial.save(folder.path()).unwrap();
    let server = server_at(folder.path()).await.unwrap();
    let worlds = server.worlds.load_full();
    assert_eq!(worlds.len(), 2);
    for world in worlds.iter() {
        assert!(Arc::ptr_eq(&server.scoreboard, &world.scoreboard));
        assert_eq!(
            world
                .scoreboard
                .lock()
                .unwrap()
                .get_score_value("Before", "parity"),
            Some(-7)
        );
    }
    worlds[1].scoreboard.lock().unwrap().set_score_value(
        &worlds[1],
        "DimensionScore",
        "parity",
        41,
    );
    assert_eq!(
        worlds[0]
            .scoreboard
            .lock()
            .unwrap()
            .get_score_value("DimensionScore", "parity"),
        Some(41)
    );
    let before = fs::read(&path).unwrap();
    for world in worlds.iter() {
        world.save().await.unwrap();
    }
    server.save_world_info().unwrap();
    assert_eq!(
        fs::read(&path).unwrap(),
        before,
        "world and metadata saves do not own the scoreboard"
    );
    server.save_all(true).await.unwrap();
    assert_eq!(
        Scoreboard::load(folder.path())
            .unwrap()
            .get_score_value("DimensionScore", "parity"),
        Some(41)
    );
    server
        .scoreboard
        .lock()
        .unwrap()
        .set_score_value(&NoTarget, "Shutdown", "parity", 73);
    server.shutdown().await;
    let loaded = Scoreboard::load(folder.path()).unwrap();
    assert_eq!(loaded.get_score_value("Shutdown", "parity"), Some(73));
    assert_eq!(loaded.get_score_value("Before", "parity"), Some(-7));
    for world in worlds.iter() {
        if world.level.level_folder.dim_folder != folder.path() {
            assert!(
                !world
                    .level
                    .level_folder
                    .dim_folder
                    .join("data/minecraft/scoreboard.dat")
                    .exists()
            );
        }
    }
}

#[tokio::test]
async fn scoreboard_save_failure_does_not_skip_independent_dimension_writes() {
    let folder = tempfile::tempdir().unwrap();
    save_metadata(folder.path());
    let server = server_at(folder.path()).await.unwrap();
    let worlds = server.worlds.load_full();
    let path = folder.path().join("data/minecraft/scoreboard.dat");
    // A directory prevents rename even as root, unlike permission-bit tests.
    fs::create_dir(&path).unwrap();
    fs::write(path.join("sentinel"), b"preserve").unwrap();
    for (index, world) in worlds.iter().enumerate() {
        world
            .custom_data
            .lock()
            .unwrap()
            .put_int("save_marker", index as i32 + 1);
    }
    let error = server.save_all(true).await.unwrap_err();
    assert!(error.contains("scoreboard"), "{error}");
    assert_eq!(fs::read(path.join("sentinel")).unwrap(), b"preserve");
    for (index, world) in worlds.iter().enumerate() {
        let bytes = fs::read(
            world
                .level
                .level_folder
                .dim_folder
                .join("pumpkin_custom_data.nbt"),
        )
        .unwrap();
        let saved = Nbt::read(&mut NbtReadHelperJava::new(Cursor::new(bytes))).unwrap();
        assert_eq!(
            saved.root_tag.get_int("save_marker"),
            Some(index as i32 + 1)
        );
    }
    fs::remove_file(path.join("sentinel")).unwrap();
    fs::remove_dir(&path).unwrap();
    // Conversely, a preceding metadata failure must still attempt scoreboard I/O.
    let metadata_temporary = folder.path().join("level.dat_new");
    fs::create_dir(&metadata_temporary).unwrap();
    server.scoreboard.lock().unwrap().add_objective(
        &NoTarget,
        ScoreboardObjective::new(
            "after_failure",
            TextComponent::text("Saved"),
            RenderType::Integer,
            None,
            "dummy",
        ),
    );
    let error = server.save_all(false).await.unwrap_err();
    assert!(error.contains("world info"), "{error}");
    assert!(
        Scoreboard::load(folder.path())
            .unwrap()
            .get_objective("after_failure")
            .is_some()
    );
    fs::remove_dir(&metadata_temporary).unwrap();
    server.shutdown().await;
    assert!(Scoreboard::load(folder.path()).is_ok());
}

#[tokio::test]
async fn malformed_scoreboard_aborts_startup_before_any_world_metadata_write() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("data/minecraft/scoreboard.dat");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut invalid = NbtCompound::new();
    invalid.put_int("DataVersion", 5023);
    invalid.put_string("data", "wrong tag type".into());
    pumpkin_nbt::nbt_compress::write_gzip_compound_tag(invalid, fs::File::create(&path).unwrap())
        .unwrap();
    let before = fs::read(&path).unwrap();
    assert!(server_at(folder.path()).await.is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(!folder.path().join("level.dat").exists());
}
