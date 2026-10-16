//! Source-only clock persistence tests, UNRUN at authoring handoff.
//! Real Server/World lifecycle with the existing stopped-worker fixture pattern;
//! no listeners, players, external authentication, or cwd configuration reads.

use super::*;
use crate::world::level_info::update_level_info;
use pumpkin_nbt::{NbtCompound, nbt_compress::read_gzip_compound_tag, tag::NbtTag};
use pumpkin_util::world_seed::Seed;
use pumpkin_world::world_info::data_files::read_world_clocks;
use std::{fs::File, path::Path};

const OFFICIAL_FILES: [(&str, &[u8]); 3] = [
    (
        "level.dat",
        include_bytes!("clock_persistence_tests/fixtures/official-26.3/level.dat"),
    ),
    (
        "data/minecraft/world_gen_settings.dat",
        include_bytes!(
            "clock_persistence_tests/fixtures/official-26.3/data/minecraft/world_gen_settings.dat"
        ),
    ),
    (
        "data/minecraft/world_clocks.dat",
        include_bytes!(
            "clock_persistence_tests/fixtures/official-26.3/data/minecraft/world_clocks.dat"
        ),
    ),
];

async fn server_at(path: &Path) -> Arc<Server> {
    let basic = BasicConfiguration {
        default_level_name: path.to_string_lossy().into_owned(),
        seed: Seed(0),
        allow_nether: true,
        allow_end: true,
        allow_chat_reports: false,
        use_favicon: false,
        ..BasicConfiguration::default()
    };
    let mut advanced = AdvancedConfiguration::default();
    advanced.networking.java.online_mode = false;
    advanced.networking.bedrock.online_mode = false;
    let data = VanillaData {
        banned_ip_list: std::sync::RwLock::default(),
        banned_player_list: std::sync::RwLock::default(),
        operator_config: std::sync::RwLock::default(),
        user_cache: std::sync::RwLock::default(),
        whitelist_config: std::sync::RwLock::default(),
    };
    let server = Server::new(
        basic,
        advanced,
        TelemetryConfig::default(),
        data,
        Vec::new(),
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(10), server.finish_tasks())
        .await
        .unwrap();
    for world in server.worlds.load().iter() {
        tokio::time::timeout(Duration::from_secs(10), world.level.shutdown())
            .await
            .unwrap()
            .unwrap();
        // No portal work in this fixture; release the Level -> World ownership cycle.
        world.level.world_portal.store(Arc::new(None));
    }
    server
}

fn tick(server: &Arc<Server>) {
    server.tick_rate_manager.tick();
    server.tick();
}

fn assert_game_time(server: &Server, expected: i64) {
    let worlds = server.worlds.load();
    assert_eq!(worlds.len(), 3);
    for world in worlds.iter() {
        assert_eq!(
            world.get_world_age(),
            expected,
            "{}",
            world.dimension.minecraft_name
        );
        assert_eq!(world.level_time.lock().unwrap().query_gametime(), expected);
    }
}

fn level_nbt(path: &Path) -> NbtCompound {
    read_gzip_compound_tag(File::open(path.join("level.dat")).unwrap()).unwrap()
}

#[tokio::test]
async fn official_game_time_survives_ticks_freeze_and_repeated_server_reloads() {
    let folder = tempfile::tempdir().unwrap();
    fs::create_dir_all(folder.path().join("data/minecraft")).unwrap();
    for (name, bytes) in OFFICIAL_FILES {
        fs::write(folder.path().join(name), bytes).unwrap();
    }
    let imported = AnvilLevelInfo.read_world_info(folder.path()).unwrap();
    assert_eq!((imported.game_time, imported.day_time), (1572, 1572));
    let original = level_nbt(folder.path());
    let original_data = original.get_compound("Data").unwrap();

    // Three fresh Server::new calls cover two full restarts without replacing
    // the live age by hand or reusing a World from the previous run.
    for restart in 0..3 {
        let game_time = 1572 + restart * 2;
        let day_time = 1572 + restart;
        let server = server_at(folder.path()).await;
        let overworld = server.get_world_from_dimension(&Dimension::OVERWORLD);
        assert_game_time(&server, game_time);
        assert_eq!(overworld.get_time_of_day(), day_time);

        tick(&server);
        assert_game_time(&server, game_time + 1);
        assert_eq!(overworld.get_time_of_day(), day_time + 1);

        update_level_info(&server.level_info, |info| {
            info.game_rules.advance_time = false;
        });
        server.tick_rate_manager.set_frozen(&server, true);
        tick(&server);
        assert_game_time(&server, game_time + 1);
        assert_eq!(overworld.get_time_of_day(), day_time + 1);
        assert!(server.tick_rate_manager.step_game_if_paused(&server, 1));
        tick(&server);
        assert_game_time(&server, game_time + 2);
        assert_eq!(overworld.get_time_of_day(), day_time + 1);
        update_level_info(&server.level_info, |info| {
            info.game_rules.advance_time = true;
        });

        server.save_world_info().unwrap();
        let saved = AnvilLevelInfo.read_world_info(folder.path()).unwrap();
        assert_eq!(
            (saved.game_time, saved.day_time),
            (game_time + 2, day_time + 1)
        );
        assert_eq!(saved.world_gen_settings, imported.world_gen_settings);
        assert_eq!(saved.level_name, imported.level_name);
        assert_eq!(saved.difficulty, imported.difficulty);
        assert_eq!(
            (saved.spawn_x, saved.spawn_y, saved.spawn_z),
            (imported.spawn_x, imported.spawn_y, imported.spawn_z)
        );
        assert_eq!(saved.data_packs, imported.data_packs);
        assert_eq!(saved.data_version, 5023);
        assert_eq!(
            read_world_clocks(folder.path()).clocks["minecraft:the_end"].total_ticks,
            1572
        );
        let root = level_nbt(folder.path());
        let data = root.get_compound("Data").unwrap();
        assert_eq!(data.get("Time"), Some(&NbtTag::Long(game_time + 2)));
        for name in [
            "ServerBrands",
            "version_history",
            "GameType",
            "initialized",
            "WasModded",
        ] {
            assert!(original_data.get(name).is_some());
            assert_eq!(data.get(name), original_data.get(name), "{name}");
        }
    }
}

#[tokio::test]
async fn large_game_time_restores_all_dimensions_and_late_world_joins_live_age() {
    let folder = tempfile::tempdir().unwrap();
    let mut initial = LevelData::default(Seed(42));
    initial.game_time = i64::from(i32::MAX) + 42;
    initial.day_time = 12_000;
    AnvilLevelInfo
        .write_world_info(&initial, folder.path())
        .unwrap();
    {
        let server = server_at(folder.path()).await;
        assert_game_time(&server, initial.game_time);
        tick(&server);
        assert_game_time(&server, initial.game_time + 1);
        let nether = server.get_world_from_dimension(&Dimension::THE_NETHER);
        let late = World::load(
            nether.level.clone(),
            server.level_info.clone(),
            Dimension::THE_NETHER,
            nether.block_registry.clone(),
            Arc::downgrade(&server),
            server.scoreboard.clone(),
        );
        assert_eq!(late.get_world_age(), initial.game_time + 1);
        assert_eq!(
            late.get_time_of_day(),
            0,
            "game age must not become the day clock"
        );
        assert_eq!(server.level_info.load().game_time, initial.game_time);
        server.save_world_info().unwrap()
    };
    let reloaded = server_at(folder.path()).await;
    assert_game_time(&reloaded, initial.game_time + 1);
    assert_eq!(
        reloaded
            .get_world_from_dimension(&Dimension::OVERWORLD)
            .get_time_of_day(),
        12_001
    );
}

struct MetadataEditDuringWrite {
    overworld: Arc<World>,
}

impl WorldInfoWriter for MetadataEditDuringWrite {
    fn write_world_info(&self, info: &LevelData, path: &Path) -> Result<(), WorldInfoError> {
        // A writer must never be called with the live clock locked.
        {
            let clock = self
                .overworld
                .level_time
                .try_lock()
                .expect("clock held across IO");
            assert_eq!(
                (info.game_time, info.day_time),
                (clock.world_age, clock.time_of_day)
            )
        };
        update_level_info(&self.overworld.level_info, |live| {
            live.game_rules.keep_inventory = true;
            live.spawn_x = 91;
        });
        AnvilLevelInfo.write_world_info(info, path)
    }
}

#[tokio::test]
async fn save_clock_overlay_releases_mutex_and_retains_concurrent_metadata_edit() {
    let folder = tempfile::tempdir().unwrap();
    let mut initial = LevelData::default(Seed(42));
    initial.day_time = 12_000;
    AnvilLevelInfo
        .write_world_info(&initial, folder.path())
        .unwrap();
    let server = server_at(folder.path()).await;
    assert_game_time(&server, 0);
    tick(&server);
    let overworld = server.get_world_from_dimension(&Dimension::OVERWORLD);
    let mut server = Arc::try_unwrap(server).unwrap_or_else(|_| panic!("fixture retained Server"));
    server.world_info_writer = Arc::new(MetadataEditDuringWrite { overworld });
    update_level_info(&server.level_info, |info| info.map_id = 73);
    server.save_world_info().unwrap();
    let live = server.level_info.load();
    assert!(live.game_rules.keep_inventory);
    assert_eq!(live.spawn_x, 91);
    assert_eq!(live.map_id, 73);
    assert_eq!((live.game_time, live.day_time), (0, 12_000));
    let saved = AnvilLevelInfo.read_world_info(folder.path()).unwrap();
    assert_eq!((saved.game_time, saved.day_time), (1, 12_001));
    assert_eq!(saved.map_id, 73);
}
