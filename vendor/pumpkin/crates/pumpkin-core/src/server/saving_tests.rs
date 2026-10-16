use super::*;
use crate::{
    data::VanillaData,
    world::scoreboard::{NoTarget, Scoreboard, ScoreboardObjective},
};
use pumpkin_config::{AdvancedConfiguration, BasicConfiguration, TelemetryConfig};
use pumpkin_data::{Block, dimension::Dimension};
use pumpkin_protocol::java::client::play::RenderType;
use pumpkin_util::{math::vector2::Vector2, text::TextComponent, world_seed::Seed};
use pumpkin_world::{
    chunk::{
        ChunkData,
        format::anvil::AnvilChunkFile,
        io::{Dirtiable, FileIO, LoadedData, file_manager::ChunkFileManager},
    },
    world_info::{LevelData, WorldInfoWriter, anvil::AnvilLevelInfo},
};
use std::path::Path;

async fn drain_tasks(server: &Server) {
    // TaskTracker::wait requires both closed and empty. Reopen only after every
    // tracked task completes, so subsequent ticks can schedule autosaves again.
    server.tasks.close();
    tokio::time::timeout(std::time::Duration::from_secs(15), server.tasks.wait())
        .await
        .expect("server fixture tasks must finish within 15 seconds");
    server.tasks.reopen();
}

async fn server_at(path: &Path, interval: u64) -> Arc<Server> {
    AnvilLevelInfo
        .write_world_info(&LevelData::default(Seed(0)), path)
        .unwrap();
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
    advanced.world.autosave_ticks = interval;
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
    drain_tasks(&server).await;
    server.tick_rate_manager.set_frozen(&server, true);
    server.tick_rate_manager.tick();
    server
}

fn add_objective(server: &Server) {
    server.scoreboard.lock().unwrap().add_objective(
        &NoTarget,
        ScoreboardObjective::new(
            "autosaved",
            TextComponent::text("Saved"),
            RenderType::Integer,
            None,
            "dummy",
        ),
    );
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "Keep frozen autosave, watched cache and save-off controls in the same fixture."
)]
async fn frozen_autosave_flushes_watched_cache_and_respects_save_off_selection() {
    let folder = tempfile::tempdir().unwrap();
    let server = server_at(folder.path(), 2).await;
    let overworld = server.get_world_from_dimension(&Dimension::OVERWORLD);
    let nether = server.get_world_from_dimension(&Dimension::THE_NETHER);
    nether.level.save_enabled.store(false, Ordering::Relaxed);
    for world in [&overworld, &nether] {
        world
            .custom_data
            .lock()
            .unwrap()
            .put_int("autosave_marker", 17);
    }
    add_objective(&server);
    let position = Vector2::new(0, 0);
    let chunk = ChunkData::empty_sync(0, 0);
    chunk.set_block_absolute_y(3, 200, 11, Block::EMERALD_BLOCK.default_state.id);
    // Raw fixture palette writes bypass Level's normal dirty marking.
    chunk.mark_dirty(true);
    assert!(chunk.is_dirty());
    overworld
        .level
        .chunk_saver
        .watch_chunks(&overworld.level.level_folder, &[position])
        .await;
    // Stage real pending IO in a watched region without generating terrain.
    // The autosave must flush this cache; scheduler holder enumeration is not
    // exercised by this fixture.
    overworld
        .level
        .chunk_saver
        .save_chunks(
            &overworld.level.level_folder,
            vec![(position, chunk.clone())],
        )
        .await
        .unwrap();
    assert!(
        !chunk.is_dirty(),
        "the staged source must have been serialized"
    );
    let region = overworld.level.level_folder.region_folder.join("r.0.0.mca");
    assert!(!region.exists(), "the watched region must still be cached");
    let (send, mut receive) = tokio::sync::mpsc::channel(1);
    overworld
        .level
        .chunk_saver
        .fetch_chunks(&overworld.level.level_folder, &[position], send)
        .await;
    let cached = match receive.recv().await {
        Some(LoadedData::Loaded(cached)) => cached,
        Some(LoadedData::Missing(pos)) => panic!("staged watched cache is missing {pos:?}"),
        Some(LoadedData::Error((pos, error))) => {
            panic!("staged watched cache failed at {pos:?}: {error}")
        }
        None => panic!("staged watched cache reader closed without a result"),
    };
    assert_eq!(
        cached.section.get_block_absolute_y(3, 200, 11),
        Some(Block::EMERALD_BLOCK.default_state.id)
    );
    assert!(!region.exists());
    let age = overworld.level_time.lock().unwrap().world_age;
    let enabled_data = overworld
        .level
        .level_folder
        .dim_folder
        .join("pumpkin_custom_data.nbt");
    let disabled_data = nether
        .level
        .level_folder
        .dim_folder
        .join("pumpkin_custom_data.nbt");
    server.tick();
    drain_tasks(&server).await;
    assert!(
        !enabled_data.exists(),
        "the configured interval has not elapsed"
    );
    assert!(!region.exists());
    server.tick();
    drain_tasks(&server).await;
    assert_eq!(overworld.level_time.lock().unwrap().world_age, age);
    assert!(enabled_data.exists());
    assert!(
        !disabled_data.exists(),
        "save-off skips the world's auxiliary snapshots"
    );
    assert!(
        Scoreboard::load(folder.path())
            .unwrap()
            .get_objective("autosaved")
            .is_some()
    );
    let reader = ChunkFileManager::<AnvilChunkFile<ChunkData>>::new(
        pumpkin_config::chunk::AnvilChunkConfig::default(),
    );
    let (send, mut receive) = tokio::sync::mpsc::channel(1);
    reader
        .fetch_chunks(&overworld.level.level_folder, &[position], send)
        .await;
    let saved = match receive.recv().await {
        Some(LoadedData::Loaded(saved)) => saved,
        Some(LoadedData::Missing(pos)) => panic!(
            "autosave reader is missing {pos:?}; region {} exists={}",
            region.display(),
            region.exists()
        ),
        Some(LoadedData::Error((pos, error))) => panic!(
            "autosave reader failed at {pos:?} in {}: {error}",
            region.display()
        ),
        None => panic!(
            "autosave reader closed without a result for {}",
            region.display()
        ),
    };
    assert_eq!(
        saved.section.get_block_absolute_y(3, 200, 11),
        Some(Block::EMERALD_BLOCK.default_state.id)
    );
    server.save_all(true).await.unwrap();
    assert!(disabled_data.exists(), "manual save overrides save-off");
    server.shutdown().await;
}

#[tokio::test]
async fn zero_interval_disables_server_autosaves() {
    let folder = tempfile::tempdir().unwrap();
    let server = server_at(folder.path(), 0).await;
    add_objective(&server);
    for _ in 0..10 {
        server.tick();
    }
    drain_tasks(&server).await;
    assert!(!folder.path().join("data/minecraft/scoreboard.dat").exists());
    assert_eq!(server.saving.elapsed.load(Ordering::Relaxed), 0);
    server.shutdown().await;
}

#[tokio::test]
async fn blocked_writer_coalesces_autosaves_and_serializes_manual_save() {
    let folder = tempfile::tempdir().unwrap();
    let server = server_at(folder.path(), 1).await;
    add_objective(&server);
    let guard = server.saving.lock().await;
    let (release, released) = tokio::sync::oneshot::channel();
    let writer_path = folder.path().join("controlled-writer");
    let writer = tokio::spawn(async move {
        let _guard = guard;
        released.await.unwrap();
        std::fs::write(writer_path, b"complete").unwrap();
    });
    let (entered, started) = tokio::sync::oneshot::channel();
    let manual_server = server.clone();
    let manual = tokio::spawn(async move {
        entered.send(()).unwrap();
        manual_server.save_all(true).await
    });
    started.await.unwrap();
    for _ in 0..100 {
        server.tick();
    }
    assert!(!manual.is_finished());
    assert_eq!(
        server.tasks.len(),
        0,
        "busy intervals must not enqueue autosave tasks"
    );
    assert!(!folder.path().join("data/minecraft/scoreboard.dat").exists());
    release.send(()).unwrap();
    writer.await.unwrap();
    manual.await.unwrap().unwrap();
    assert_eq!(
        std::fs::read(folder.path().join("controlled-writer")).unwrap(),
        b"complete"
    );
    assert!(
        Scoreboard::load(folder.path())
            .unwrap()
            .get_objective("autosaved")
            .is_some()
    );
    server.tick();
    assert_eq!(
        server.tasks.len(),
        1,
        "a later interval can spawn one new autosave"
    );
    drain_tasks(&server).await;
    server.shutdown().await;
}

#[tokio::test]
async fn autosave_failure_is_retained_and_independent_world_writes_complete() {
    let folder = tempfile::tempdir().unwrap();
    let server = server_at(folder.path(), 1).await;
    let blocked = folder.path().join("data/minecraft/scoreboard.dat");
    std::fs::create_dir_all(&blocked).unwrap();
    for world in server.worlds.load().iter() {
        world.custom_data.lock().unwrap().put_int("after_error", 23);
    }
    server.tick();
    drain_tasks(&server).await;
    assert!(
        server
            .saving
            .autosave_error
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .contains("scoreboard")
    );
    for world in server.worlds.load().iter() {
        assert!(
            world
                .level
                .level_folder
                .dim_folder
                .join("pumpkin_custom_data.nbt")
                .exists()
        );
    }
    std::fs::remove_dir(blocked).unwrap();
    // The shutdown hook consumes this sticky diagnostic into process exit status;
    // leave that process-global status untouched by this parallel unit test.
    server.saving.autosave_error.lock().unwrap().take();
    server.shutdown().await;
}
