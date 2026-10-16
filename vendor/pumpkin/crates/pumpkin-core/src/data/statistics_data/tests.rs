use super::*;
use crate::data::player_server::test::{close_fixture, java_client, player_fixture};
use crate::entity::{
    NBTStorage,
    player::statistics::{CustomStatistic, Statistics, StatisticsDataError},
};
use crate::plugin::{
    EventHandler,
    api::events::{EventPriority, player::PlayerLoginEvent},
};
use crate::server::Server;
use futures::future::BoxFuture;
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use serde_json::{Value, json};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const OBSERVED: &[u8] = br#"{
  "stats":{"minecraft:custom":{
    "minecraft:open_barrel":18,"minecraft:trigger_trapped_chest":26,
    "minecraft:open_chest":26,"minecraft:leave_game":2,
    "minecraft:play_time":2838,"minecraft:total_world_time":2838,
    "minecraft:time_since_death":2838,"minecraft:time_since_rest":2838}},
  "DataVersion":5023
}"#;

fn read_stats(manager: &StatisticsManager, uuid: Uuid) -> Value {
    serde_json::from_slice(&fs::read(manager.player_path(uuid)).unwrap()).unwrap()
}

async fn close_client(client: &Arc<crate::net::ClientPlatform>) {
    let java = client.java().unwrap();
    java.close();
    tokio::time::timeout(Duration::from_secs(5), java.await_tasks())
        .await
        .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn symlinked_world_saves_and_fresh_loads_but_refuses_leaf_links() {
    let fixture = player_fixture().await;
    let directory = tempfile::tempdir().unwrap();
    let linked_world = directory.path().join("world");
    std::os::unix::fs::symlink(fixture._directory.path(), &linked_world).unwrap();
    let manager = StatisticsManager::new(linked_world.join("players"), true);
    let uuid = fixture.player.gameprofile.id;
    assert!(!manager.player_path(uuid).exists());
    manager.load_player(&fixture.player).unwrap();
    fixture
        .player
        .increment_custom_stat(CustomStatistic::OpenBarrel, 2);
    manager.save_player(&fixture.player).await.unwrap();

    let path = fixture.server.statistics_manager.player_path(uuid);
    let original = fs::read(&path).unwrap();
    assert_eq!(fs::read(manager.player_path(uuid)).unwrap(), original);
    let saved: Value = serde_json::from_slice(&original).unwrap();
    assert_eq!(
        saved["stats"]["minecraft:custom"]["minecraft:open_barrel"],
        2
    );
    let fresh = Arc::new(Player::new(
        fixture.client.clone(),
        fixture.player.gameprofile.clone(),
        Default::default(),
        &fixture.player.world(),
        pumpkin_util::GameMode::Survival,
    ));
    assert_eq!(fresh.get_custom_stat(CustomStatistic::OpenBarrel), 0);
    let restarted = StatisticsManager::new(linked_world.join("players"), true);
    restarted.load_player(&fresh).unwrap();
    assert_eq!(fresh.get_custom_stat(CustomStatistic::OpenBarrel), 2);

    // Allowing a directory link must not allow either a linked or dangling
    // final file to become a load/save-eligible default statistics history.
    let retained = path.with_extension("json.original");
    let missing = path.with_extension("json.missing");
    fs::rename(&path, &retained).unwrap();
    for target in [&retained, &missing] {
        std::os::unix::fs::symlink(target, &path).unwrap();
        assert!(matches!(
            restarted.load_player(&fresh),
            Err(StatisticsDataError::Io(_))
        ));
        assert!(matches!(
            restarted.save_player(&fresh).await,
            Err(StatisticsDataError::NotLoaded)
        ));
        assert_eq!(fs::read(&retained).unwrap(), original);
        assert!(
            fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(!missing.exists());
        fs::remove_file(&path).unwrap();
    }
    fs::rename(&retained, &path).unwrap();
    close_fixture(&fixture).await;
}

#[tokio::test]
async fn observed_import_live_increment_save_and_actual_rejoin_preserve_unknowns() {
    let fixture = player_fixture().await;
    let manager = &fixture.server.statistics_manager;
    let uuid = fixture.player.gameprofile.id;
    let mut imported: Value = serde_json::from_slice(OBSERVED).unwrap();
    imported["stats"]["minecraft:custom"]["future:counter"] = json!(91);
    imported["stats"]["future:category"] = json!({"future:counter":73});
    imported["extension"] = json!({"producer":"fixture", "payload":[1,true,null]});
    fs::write(
        manager.player_path(uuid),
        serde_json::to_vec(&imported).unwrap(),
    )
    .unwrap();
    manager.load_player(&fixture.player).unwrap();
    assert_eq!(
        fixture.player.get_custom_stat(CustomStatistic::OpenBarrel),
        18
    );
    assert_eq!(
        fixture
            .player
            .stats
            .lock()
            .unwrap()
            .network_entries()
            .count(),
        8
    );
    fixture
        .player
        .increment_custom_stat(CustomStatistic::OpenBarrel, 2);
    manager.save_player(&fixture.player).await.unwrap();
    assert_eq!(
        read_stats(manager, uuid)["stats"]["minecraft:custom"]["minecraft:open_barrel"],
        20
    );
    crate::finish_player_session(&fixture.server, &fixture.player)
        .await
        .unwrap();
    let saved = read_stats(manager, uuid);
    assert_eq!(saved["DataVersion"], 5023);
    assert_eq!(
        saved["stats"]["minecraft:custom"]["minecraft:leave_game"],
        3
    );
    assert_eq!(
        saved["stats"]["minecraft:custom"]["minecraft:trigger_trapped_chest"],
        26
    );
    assert_eq!(saved["stats"]["minecraft:custom"]["future:counter"], 91);
    assert_eq!(
        saved["stats"]["future:category"],
        imported["stats"]["future:category"]
    );
    assert_eq!(saved["extension"], imported["extension"]);
    let player_nbt = fixture
        .server
        .player_data_storage
        .load_data(&uuid)
        .unwrap()
        .unwrap();
    assert!(
        player_nbt.get("Statistics").is_none(),
        "new canonical players never generate private NBT"
    );

    // Real admission, then a separate new manager to model a restarted process.
    let profile = fixture.player.gameprofile.clone();
    let (client, _remote) = java_client(&fixture.server, &profile).await;
    let (rejoined, _) = fixture
        .server
        .add_player(client.clone(), profile, None)
        .await
        .unwrap();
    assert_eq!(rejoined.get_custom_stat(CustomStatistic::OpenBarrel), 20);
    assert_eq!(rejoined.get_custom_stat(CustomStatistic::LeaveGame), 3);
    let restarted = StatisticsManager::new(fixture._directory.path().join("players"), true);
    restarted.load_player(&rejoined).unwrap();
    assert_eq!(rejoined.get_custom_stat(CustomStatistic::OpenBarrel), 20);
    rejoined.increment_custom_stat(CustomStatistic::OpenBarrel, 1);
    crate::finish_player_session(&fixture.server, &rejoined)
        .await
        .unwrap();
    assert_eq!(
        read_stats(manager, uuid)["stats"]["minecraft:custom"]["minecraft:open_barrel"],
        21
    );
    close_client(&client).await;
    close_fixture(&fixture).await;
}

struct ObserveLogin(AtomicBool);

impl EventHandler<PlayerLoginEvent> for ObserveLogin {
    fn handle<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        _event: &'a PlayerLoginEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            self.0.store(true, Ordering::Relaxed);
        })
    }
}

#[tokio::test]
async fn invalid_statistics_refuse_admission_callbacks_publication_and_saves() {
    let fixture = player_fixture().await;
    let manager = &fixture.server.statistics_manager;
    let uuid = fixture.player.gameprofile.id;
    crate::finish_player_session(&fixture.server, &fixture.player)
        .await
        .unwrap();
    let player_path = fixture
        ._directory
        .path()
        .join("players/data")
        .join(format!("{uuid}.dat"));
    let player_bytes = fs::read(&player_path).unwrap();
    let advancement_path = fixture
        .server
        .advancement_manager
        .advancement_path
        .join(format!("{uuid}.json"));
    let advancement_bytes = fs::read(&advancement_path).unwrap();
    let path = manager.player_path(uuid);
    let login = Arc::new(ObserveLogin(AtomicBool::new(false)));
    fixture
        .server
        .plugin_manager
        .register::<PlayerLoginEvent, _>(login.clone(), EventPriority::Normal, false);
    let cases: &[Option<&[u8]>] = &[
        Some(b"malformed"),
        Some(br#"{"DataVersion":5024,"stats":{}}"#),
        Some(br#"{"DataVersion":5022,"stats":{}}"#),
        Some(br#"{"DataVersion":5023,"stats":{"minecraft:custom":{"minecraft:open_barrel":2,"minecraft:open_chest":"bad"}}}"#),
        None, // A directory at the file path is unreadable as statistics, even as root.
    ];
    for contents in cases {
        fs::remove_file(&path).unwrap();
        if let Some(contents) = contents {
            fs::write(&path, contents).unwrap();
        } else {
            fs::create_dir(&path).unwrap();
            fs::write(path.join("sentinel"), b"untouched").unwrap();
        }
        let profile = fixture.player.gameprofile.clone();
        let (client, _remote) = java_client(&fixture.server, &profile).await;
        assert!(
            fixture
                .server
                .add_player(client.clone(), profile.clone(), None)
                .await
                .is_none()
        );
        assert!(client.closed());
        assert!(client.java().unwrap().player.load().is_none());
        assert!(fixture.server.get_all_players().is_empty());
        assert!(!login.0.load(Ordering::Relaxed));
        drop(
            tokio::time::timeout(
                Duration::from_secs(1),
                fixture
                    .server
                    .player_data_storage
                    .reserve_player_session(uuid),
            )
            .await
            .unwrap(),
        );

        let world = fixture.player.world();
        let unpublished = Arc::new(Player::new(
            client.clone(),
            profile,
            Default::default(),
            &world,
            pumpkin_util::GameMode::Survival,
        ));
        assert!(matches!(
            manager.save_player(&unpublished).await,
            Err(StatisticsDataError::NotLoaded)
        ));
        assert!(manager.load_player(&unpublished).is_err());
        assert!(matches!(
            manager.save_player(&unpublished).await,
            Err(StatisticsDataError::NotLoaded)
        ));
        assert!(unpublished.session.lock().unwrap().is_none());
        fixture
            .server
            .player_data_storage
            .save_all_players(&fixture.server)
            .unwrap();
        fixture
            .server
            .advancement_manager
            .save_all_players(&[])
            .await
            .unwrap();
        manager
            .save_all_players(&fixture.server.get_all_players())
            .await
            .unwrap();
        assert_eq!(fs::read(&player_path).unwrap(), player_bytes);
        assert_eq!(fs::read(&advancement_path).unwrap(), advancement_bytes);
        if let Some(contents) = contents {
            assert_eq!(fs::read(&path).unwrap(), *contents);
        } else {
            assert_eq!(fs::read(path.join("sentinel")).unwrap(), b"untouched");
            fs::remove_file(path.join("sentinel")).unwrap();
            fs::remove_dir(&path).unwrap();
            fs::write(&path, OBSERVED).unwrap();
        }
        close_client(&client).await;
    }
    close_fixture(&fixture).await;
}

#[tokio::test]
async fn legacy_only_and_conflicting_private_history_refuse_real_admission() {
    let fixture = player_fixture().await;
    let manager = &fixture.server.statistics_manager;
    let uuid = fixture.player.gameprofile.id;
    crate::finish_player_session(&fixture.server, &fixture.player)
        .await
        .unwrap();
    let path = manager.player_path(uuid);
    let player_path = fixture
        ._directory
        .path()
        .join("players/data")
        .join(format!("{uuid}.dat"));
    let mut nbt = fixture
        .server
        .player_data_storage
        .load_data(&uuid)
        .unwrap()
        .unwrap();
    let mut legacy = NbtCompound::new();
    legacy.put_int("8:63", 200);
    let login = Arc::new(ObserveLogin(AtomicBool::new(false)));
    fixture
        .server
        .plugin_manager
        .register::<PlayerLoginEvent, _>(login.clone(), EventPriority::Normal, false);
    for payload in [NbtTag::Compound(legacy), NbtTag::Int(0)] {
        for coexist in [false, true] {
            if coexist {
                fs::write(&path, OBSERVED).unwrap();
            } else if path.exists() {
                fs::remove_file(&path).unwrap();
            }
            nbt.put("Statistics", payload.clone());
            pumpkin_world::nbt_io::write_gzip_compound_atomic(&player_path, nbt.clone()).unwrap();
            let before = fs::read(&player_path).unwrap();
            let profile = fixture.player.gameprofile.clone();
            let (client, _remote) = java_client(&fixture.server, &profile).await;
            assert!(
                fixture
                    .server
                    .add_player(client.clone(), profile.clone(), None)
                    .await
                    .is_none()
            );
            assert!(fixture.server.get_all_players().is_empty());
            assert!(!login.0.load(Ordering::Relaxed));
            let mut unpublished = Player::new(
                client.clone(),
                profile,
                Default::default(),
                &fixture.player.world(),
                pumpkin_util::GameMode::Survival,
            );
            unpublished.read_nbt(&mut nbt.clone());
            assert!(matches!(
                manager.load_player(&unpublished),
                Err(StatisticsDataError::LegacyPrivateStatistics)
            ));
            assert!(matches!(
                manager.save_player(&Arc::new(unpublished)).await,
                Err(StatisticsDataError::NotLoaded)
            ));
            assert_eq!(fs::read(&player_path).unwrap(), before);
            if coexist {
                assert_eq!(fs::read(&path).unwrap(), OBSERVED);
            } else {
                assert!(!path.exists());
            }
            close_client(&client).await;
        }
    }
    close_fixture(&fixture).await;
}

#[tokio::test]
async fn final_leave_statistics_save_once_even_when_both_other_families_fail() {
    let fixture = player_fixture().await;
    let uuid = fixture.player.gameprofile.id;
    let player_path = fixture
        ._directory
        .path()
        .join("players/data")
        .join(format!("{uuid}.dat"));
    fs::create_dir(&player_path).unwrap();
    let advancement_path = fixture
        .server
        .advancement_manager
        .advancement_path
        .join(format!("{uuid}.json"));
    fs::create_dir(&advancement_path).unwrap();
    fixture
        .player
        .set_custom_stat(CustomStatistic::LeaveGame, 2);
    assert!(matches!(
        crate::finish_player_session(&fixture.server, &fixture.player).await,
        Err(crate::PlayerSessionSaveError::Both { .. })
    ));
    let saved = read_stats(&fixture.server.statistics_manager, uuid);
    assert_eq!(
        saved["stats"]["minecraft:custom"]["minecraft:leave_game"],
        3
    );
    assert_eq!(
        fixture.player.get_custom_stat(CustomStatistic::LeaveGame),
        3
    );
    crate::finish_player_session(&fixture.server, &fixture.player)
        .await
        .unwrap();
    assert_eq!(read_stats(&fixture.server.statistics_manager, uuid), saved);
    assert_eq!(
        fixture.player.get_custom_stat(CustomStatistic::LeaveGame),
        3
    );
    drop(
        tokio::time::timeout(
            Duration::from_secs(1),
            fixture
                .server
                .player_data_storage
                .reserve_player_session(uuid),
        )
        .await
        .unwrap(),
    );
    close_fixture(&fixture).await;
}

#[tokio::test]
async fn manager_orders_captures_and_admission_waits_for_blocking_statistics_write() {
    let fixture = player_fixture().await;
    let manager = &fixture.server.statistics_manager;
    let uuid = fixture.player.gameprofile.id;
    fixture
        .player
        .set_custom_stat(CustomStatistic::OpenBarrel, 3);
    let guard = manager.save_gate.clone().lock_owned().await;
    let players = [fixture.player.clone()];
    let mut older_save = Box::pin(manager.save_all_players(&players));
    assert!(futures::poll!(&mut older_save).is_pending());
    fixture
        .player
        .increment_custom_stat(CustomStatistic::OpenBarrel, 2);
    drop(guard);
    older_save.await.unwrap();
    assert_eq!(
        read_stats(manager, uuid)["stats"]["minecraft:custom"]["minecraft:open_barrel"],
        5
    );
    crate::finish_player_session(&fixture.server, &fixture.player)
        .await
        .unwrap();

    // Model a cancelled awaiter's still-running worker with the exact same owned
    // gate and production atomic writer. The UUID admission lease is now free.
    let save_guard = manager.save_gate.clone().lock_owned().await;
    fixture
        .player
        .set_custom_stat(CustomStatistic::OpenBarrel, 7);
    let bytes = fixture
        .player
        .stats
        .lock()
        .unwrap()
        .snapshot()
        .unwrap()
        .unwrap();
    let path = manager.player_path(uuid);
    let (release, receive) = std::sync::mpsc::channel();
    let worker = tokio::task::spawn_blocking(move || {
        let _save_guard = save_guard;
        receive.recv_timeout(Duration::from_secs(10)).unwrap();
        write_statistics_snapshots(vec![(path, bytes)])
    });
    let profile = fixture.player.gameprofile.clone();
    let (client, _remote) = java_client(&fixture.server, &profile).await;
    let mut admission = Box::pin(fixture.server.add_player(client.clone(), profile, None));
    assert!(futures::poll!(&mut admission).is_pending());
    assert!(!client.closed());
    assert!(fixture.server.get_all_players().is_empty());
    release.send(()).unwrap();
    worker.await.unwrap().unwrap();
    let (rejoined, _) = tokio::time::timeout(Duration::from_secs(5), admission)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rejoined.get_custom_stat(CustomStatistic::OpenBarrel), 7);
    assert_eq!(rejoined.get_custom_stat(CustomStatistic::LeaveGame), 1);
    crate::finish_player_session(&fixture.server, &rejoined)
        .await
        .unwrap();
    close_client(&client).await;
    close_fixture(&fixture).await;
}

#[tokio::test]
async fn statistics_save_error_keeps_other_families_and_existing_combined_errors() {
    let fixture = player_fixture().await;
    let uuid = fixture.player.gameprofile.id;
    let path = fixture.server.statistics_manager.player_path(uuid);
    fs::create_dir(&path).unwrap();
    let error = crate::finish_player_session(&fixture.server, &fixture.player)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        crate::PlayerSessionSaveError::Statistics(_)
    ));
    assert!(
        fixture
            .server
            .player_data_storage
            .load_data(&uuid)
            .unwrap()
            .is_some()
    );
    assert!(
        fixture
            .server
            .advancement_manager
            .advancement_path
            .join(format!("{uuid}.json"))
            .is_file()
    );
    assert_eq!(
        fixture.player.get_custom_stat(CustomStatistic::LeaveGame),
        1
    );
    close_fixture(&fixture).await;

    let fixture = player_fixture().await;
    let uuid = fixture.player.gameprofile.id;
    fs::create_dir(
        fixture
            ._directory
            .path()
            .join("players/data")
            .join(format!("{uuid}.dat")),
    )
    .unwrap();
    fs::create_dir(
        fixture
            .server
            .advancement_manager
            .advancement_path
            .join(format!("{uuid}.json")),
    )
    .unwrap();
    fs::create_dir(fixture.server.statistics_manager.player_path(uuid)).unwrap();
    let error = crate::finish_player_session(&fixture.server, &fixture.player)
        .await
        .unwrap_err();
    let crate::PlayerSessionSaveError::WithStatistics { previous, .. } = error else {
        panic!("statistics failure must retain errors from both earlier save attempts");
    };
    assert!(matches!(
        *previous,
        crate::PlayerSessionSaveError::Both { .. }
    ));
    close_fixture(&fixture).await;
}

/// A real partial write fails in an isolated child process. No other tests
/// inherit its file-size limit or SIGXFSZ handler, including parallel runs.
#[cfg(unix)]
#[test]
fn statistics_atomic_write_failure_preserves_previous_json_and_eligibility() {
    const CHILD_DIRECTORY: &str = "FERRALOOM_STATISTICS_ATOMIC_TEST_DIR";
    if let Some(directory) = std::env::var_os(CHILD_DIRECTORY) {
        let directory = PathBuf::from(directory);
        let path = directory.join("stats.json");
        fs::write(&path, OBSERVED).unwrap();
        let mut stats = Statistics::default();
        stats.load(&path, true).unwrap();
        stats.increment_custom(CustomStatistic::OpenBarrel, 2);
        let mut expanded: Value =
            serde_json::from_slice(&stats.snapshot().unwrap().unwrap()).unwrap();
        expanded["extension"] = json!("x".repeat(8192));
        let bytes = serde_json::to_vec_pretty(&expanded).unwrap();
        let mut previous = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // Assertions before changing process-wide state; always restore it before
        // checking the expected failure or writing the completion sentinel.
        let (limit, previous_signal) = unsafe {
            assert_eq!(libc::getrlimit(libc::RLIMIT_FSIZE, &mut previous), 0);
            let previous_signal = libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
            assert_ne!(previous_signal, libc::SIG_ERR);
            let limit = libc::rlimit {
                rlim_cur: 1024.min(previous.rlim_cur),
                rlim_max: previous.rlim_max,
            };
            (limit, previous_signal)
        };
        unsafe {
            assert_eq!(libc::setrlimit(libc::RLIMIT_FSIZE, &limit), 0);
        }
        let result = write_statistics_snapshots(vec![(path.clone(), bytes)]);
        unsafe {
            assert_eq!(libc::setrlimit(libc::RLIMIT_FSIZE, &previous), 0);
            libc::signal(libc::SIGXFSZ, previous_signal);
        }
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), OBSERVED);
        assert_eq!(
            fs::read_dir(&directory).unwrap().count(),
            1,
            "staging files must be cleaned up"
        );
        // Save failure does not forbid retrying a successfully loaded history.
        write_statistics_snapshots(vec![(path.clone(), stats.snapshot().unwrap().unwrap())])
            .unwrap();
        let mut reloaded = Statistics::default();
        reloaded.load(&path, true).unwrap();
        assert_eq!(
            reloaded.get(
                crate::entity::player::statistics::StatisticCategory::Custom,
                CustomStatistic::OpenBarrel as i32
            ),
            20
        );
        fs::write(directory.join("child-finished"), b"passed\n").unwrap();
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "data::statistics_data::tests::statistics_atomic_write_failure_preserves_previous_json_and_eligibility", "--nocapture", "--test-threads=1"])
        .env(CHILD_DIRECTORY, directory.path())
        .stdin(std::process::Stdio::null())
        .spawn().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                assert!(
                    status.success(),
                    "statistics write-failure child failed: {status}"
                );
                assert_eq!(
                    fs::read(directory.path().join("child-finished")).unwrap(),
                    b"passed\n"
                );
                return;
            }
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("failed to wait for statistics write-failure child: {error}");
            }
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("statistics write-failure child exceeded 20 seconds");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
