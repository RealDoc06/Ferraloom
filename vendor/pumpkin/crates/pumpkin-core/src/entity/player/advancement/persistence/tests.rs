use super::*;
use crate::data::advancement_data::AdvancementManager;
use crate::entity::player::advancement::PlayerAdvancement;
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;

// Values transcribed from the retained official 26.3 ZeroCommand save in
// chat-throttle-wuoqvsb3/vanilla/production-defaults/world/players/advancements.
const OFFICIAL: &str = r#"{
    "minecraft:adventure/adventuring_time": {
        "criteria":{"minecraft:snowy_taiga":"2026-10-16 03:33:08 +0200"},
        "done":false
    },
    "minecraft:recipes/decorations/crafting_table": {
        "criteria":{"unlock_right_away":"2026-10-16 03:33:08 +0200"},
        "done":true
    },
    "DataVersion":5023
}"#;

fn player(directory: &Path, enabled: bool) -> PlayerAdvancement {
    PlayerAdvancement::new(
        Arc::new(AdvancementManager::new(directory, enabled)),
        Uuid::new_v4(),
    )
}

fn saved(player: &PlayerAdvancement) -> Value {
    let (_, json) = player.snapshot().unwrap().unwrap();
    serde_json::from_str(&json).unwrap()
}

#[tokio::test]
async fn official_complete_and_partial_progress_bootstrap_and_roundtrip() {
    let directory = tempfile::tempdir().unwrap();
    let mut player = player(directory.path(), true);
    fs::write(&player.path, OFFICIAL).unwrap();
    player.load().unwrap();
    let complete = Advancement::RECIPES_DECORATIONS_CRAFTING_TABLE;
    let partial = Advancement::ADVENTURE_ADVENTURING_TIME;
    assert!(player.progress.map[complete].is_done());
    let progress = &player.progress.map[partial];
    assert!(!progress.is_done());
    assert!(progress.criteria["minecraft:snowy_taiga"].is_done());
    assert!(!progress.criteria["minecraft:plains"].is_done());
    assert!(player.is_first_packet);
    assert!(player.visible.is_empty());
    assert!(player.progress_changed.contains(complete));
    assert!(player.progress_changed.contains(partial));
    assert!(!player.roots_to_update.is_empty());
    assert_eq!(
        saved(&player),
        serde_json::from_str::<Value>(OFFICIAL).unwrap()
    );
    player.save().await.unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&player.path).unwrap()).unwrap(),
        saved(&player)
    );
}

#[tokio::test]
async fn known_requirements_control_done_without_losing_unknown_progress_or_fields() {
    let directory = tempfile::tempdir().unwrap();
    let mut player = player(directory.path(), true);
    let original = json!({
        "DataVersion": MAXIMUM_SUPPORTED_WORLD_DATA_VERSION,
        "minecraft:story/root": {
            "criteria": {
                "crafting_table": "2026-10-16 03:33:08 +0200",
                "datapack_only": "1969-12-31 23:59:59 +0000"
            },
            "done": false,
            "extension": {"nested": [1, null, "retain"]}
        },
        "example:unknown": {
            "criteria": {"future": "2026-10-16 07:03:08 +0530"},
            "done": false,
            "extension": {"number": 17}
        },
        "example:completed": {"done": true},
        "example:default_done": {}
    });
    fs::write(&player.path, original.to_string()).unwrap();
    player.load().unwrap();
    assert_eq!(player.progress.len(), 1);
    assert!(player.progress.map[Advancement::STORY_ROOT].is_done());
    assert!(
        !player.progress.map[Advancement::STORY_ROOT]
            .criteria
            .contains_key("datapack_only")
    );
    let snapshot = saved(&player);
    assert_eq!(snapshot["minecraft:story/root"]["done"], true);
    assert_eq!(snapshot["example:unknown"], original["example:unknown"]);
    assert_eq!(snapshot["example:completed"], original["example:completed"]);
    assert_eq!(snapshot["example:default_done"]["done"], true);
    assert!(player.revoke(Advancement::STORY_ROOT, "crafting_table"));
    player.save().await.unwrap();
    let snapshot = saved(&player);
    assert_eq!(snapshot["minecraft:story/root"]["done"], false);
    assert!(
        snapshot["minecraft:story/root"]["criteria"]
            .get("crafting_table")
            .is_none()
    );
    assert_eq!(
        snapshot["minecraft:story/root"]["criteria"]["datapack_only"],
        original["minecraft:story/root"]["criteria"]["datapack_only"]
    );
    assert_eq!(
        snapshot["minecraft:story/root"]["extension"],
        original["minecraft:story/root"]["extension"]
    );
    player.reload().unwrap();
    assert_eq!(saved(&player), snapshot);
}

#[tokio::test]
async fn legacy_rust_is_identified_but_never_replaced_without_a_migration() {
    let directory = tempfile::tempdir().unwrap();
    let mut player = player(directory.path(), true);
    let legacy = br#"{"minecraft:adventure/adventuring_time":{
        "minecraft:badlands":{"secs_since_epoch":1792114435,"nanos_since_epoch":681690534},
        "minecraft:plains":null
    }}"#;
    fs::write(&player.path, legacy).unwrap();
    assert!(matches!(
        player.load(),
        Err(AdvancementDataError::LegacyRustFormat)
    ));
    assert!(matches!(
        player.save().await,
        Err(AdvancementDataError::NotLoaded)
    ));
    assert!(matches!(
        player.snapshot(),
        Err(AdvancementDataError::NotLoaded)
    ));
    assert_eq!(fs::read(&player.path).unwrap(), legacy);
    for ambiguous in ["{}", r#"{"minecraft:story/root":{"crafting_table":null}}"#] {
        assert!(matches!(
            decode(ambiguous.as_bytes()),
            Err(AdvancementDataError::Schema(_))
        ));
    }
}

#[tokio::test]
async fn whole_file_failures_keep_runtime_and_bytes_and_disable_all_snapshots() {
    let directory = tempfile::tempdir().unwrap();
    let mut player = player(directory.path(), true);
    fs::write(&player.path, OFFICIAL).unwrap();
    player.load().unwrap();
    let original = saved(&player);
    let invalid = [
        "not JSON".to_owned(),
        "[]".to_owned(),
        "{}".to_owned(),
        OFFICIAL.replace("5023", "5022"),
        OFFICIAL.replace("5023", "5024"),
        OFFICIAL.replace("2026-10-16 03:33:08 +0200", "2026-02-30 03:33:08 +0200"),
        OFFICIAL.replace("\"done\":true", "\"done\":\"yes\""),
        OFFICIAL.replace(
            "\"DataVersion\":5023",
            "\"DataVersion\":5023,\"DataVersion\":5023",
        ),
        OFFICIAL.replace("\"done\":true", "\"done\":true,\"done\":false"),
        OFFICIAL.replace(
            "\"unlock_right_away\":",
            "\"unlock_right_away\":\"2026-10-16 00:00:00 +0000\",\"unlock_right_away\":",
        ),
        OFFICIAL.replace(
            "\"DataVersion\":5023",
            "\"DataVersion\":5023,\"invalid id\":{}",
        ),
    ];
    for bytes in invalid {
        player.visible.insert(Advancement::STORY_ROOT);
        player.last_selected_tab = Some(Advancement::STORY_ROOT);
        player.is_first_packet = false;
        let pending = player.progress_changed.clone();
        fs::write(&player.path, &bytes).unwrap();
        assert!(player.reload().is_err(), "must reject {bytes}");
        assert!(player.progress.map[Advancement::RECIPES_DECORATIONS_CRAFTING_TABLE].is_done());
        assert!(player.visible.contains(Advancement::STORY_ROOT));
        assert!(player.last_selected_tab == Some(Advancement::STORY_ROOT));
        assert!(!player.is_first_packet);
        assert!(player.progress_changed == pending);
        assert!(matches!(
            player.save().await,
            Err(AdvancementDataError::NotLoaded)
        ));
        assert!(matches!(
            player.snapshot(),
            Err(AdvancementDataError::NotLoaded)
        ));
        assert!(serde_json::to_string(&player).is_err());
        assert_eq!(fs::read(&player.path).unwrap(), bytes.as_bytes());
    }
    fs::write(&player.path, OFFICIAL).unwrap();
    player.reload().unwrap();
    assert_eq!(saved(&player), original);
    assert!(player.is_first_packet);
    assert!(player.visible.is_empty());
    assert!(player.last_selected_tab.is_none());
    fs::remove_file(&player.path).unwrap();
    assert!(matches!(player.reload(), Err(AdvancementDataError::Io(_))));
    assert!(matches!(
        player.save().await,
        Err(AdvancementDataError::NotLoaded)
    ));
    assert!(!player.path.exists());
}

#[tokio::test]
async fn only_confirmed_absence_enables_new_player_writes_and_disabled_never_reads() {
    let directory = tempfile::tempdir().unwrap();
    let mut new = player(directory.path(), true);
    assert!(matches!(
        new.save().await,
        Err(AdvancementDataError::NotLoaded)
    ));
    new.load().unwrap();
    new.save().await.unwrap();
    assert_eq!(
        saved(&new),
        json!({"DataVersion": MAXIMUM_SUPPORTED_WORLD_DATA_VERSION})
    );
    new.progress
        .get_mut_or_start_progress(Advancement::STORY_ROOT)
        .grant_progress("crafting_table");
    fs::remove_file(&new.path).unwrap();
    assert!(matches!(new.reload(), Err(AdvancementDataError::Io(_))));
    assert!(new.progress.map[Advancement::STORY_ROOT].is_done());
    assert!(matches!(
        new.save().await,
        Err(AdvancementDataError::NotLoaded)
    ));
    let mut disabled = player(directory.path(), false);
    fs::write(&disabled.path, b"corrupt, deliberately unread").unwrap();
    disabled.load().unwrap();
    disabled
        .progress
        .get_mut_or_start_progress(Advancement::STORY_ROOT)
        .grant_progress("crafting_table");
    disabled.reload().unwrap();
    assert!(disabled.progress.map[Advancement::STORY_ROOT].is_done());
    disabled.save().await.unwrap();
    assert!(disabled.snapshot().unwrap().is_none());
    assert_eq!(
        fs::read(&disabled.path).unwrap(),
        b"corrupt, deliberately unread"
    );
}

#[tokio::test]
async fn directory_and_missing_ancestor_are_errors_not_empty_players() {
    let directory = tempfile::tempdir().unwrap();
    let mut player = player(directory.path(), true);
    fs::create_dir(&player.path).unwrap();
    let sentinel = player.path.join("retained");
    fs::write(&sentinel, b"leave me alone").unwrap();
    assert!(matches!(player.load(), Err(AdvancementDataError::Io(_))));
    assert!(matches!(
        player.save().await,
        Err(AdvancementDataError::NotLoaded)
    ));
    assert_eq!(fs::read(sentinel).unwrap(), b"leave me alone");
    player.path = directory.path().join("absent-parent/player.json");
    assert!(matches!(player.load(), Err(AdvancementDataError::Io(_))));
    assert!(matches!(
        player.snapshot(),
        Err(AdvancementDataError::NotLoaded)
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn unreadable_file_cannot_be_saved_as_an_empty_player() {
    use std::os::unix::fs::PermissionsExt;
    // Root bypasses mode bits; this fixture needs an unprivileged test process.
    // SAFETY: geteuid has no preconditions and does not change process state.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let mut player = player(directory.path(), true);
    fs::write(&player.path, OFFICIAL).unwrap();
    fs::set_permissions(&player.path, fs::Permissions::from_mode(0o000)).unwrap();
    let loaded = player.load();
    let saved = player.save().await;
    fs::set_permissions(&player.path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(
        matches!(loaded, Err(AdvancementDataError::Io(error)) if error.kind() == io::ErrorKind::PermissionDenied)
    );
    assert!(matches!(saved, Err(AdvancementDataError::NotLoaded)));
    assert_eq!(fs::read_to_string(&player.path).unwrap(), OFFICIAL);
}

#[cfg(unix)]
#[tokio::test]
async fn dangling_and_readable_symlinks_never_become_writable_new_players() {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().unwrap();
    let mut player = player(directory.path(), true);
    let target = directory.path().join("target.json");
    symlink(&target, &player.path).unwrap();
    assert!(matches!(player.load(), Err(AdvancementDataError::Io(_))));
    assert!(matches!(
        player.save().await,
        Err(AdvancementDataError::NotLoaded)
    ));
    assert_eq!(fs::read_link(&player.path).unwrap(), target);
    assert!(!target.exists());
    fs::write(&target, OFFICIAL).unwrap();
    assert!(matches!(player.load(), Err(AdvancementDataError::Io(_))));
    assert!(matches!(
        player.save().await,
        Err(AdvancementDataError::NotLoaded)
    ));
    assert_eq!(fs::read_to_string(&target).unwrap(), OFFICIAL);
}

#[cfg(unix)]
#[tokio::test]
async fn symlinked_world_directory_supports_new_and_existing_advancements() {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().unwrap();
    let world = directory.path().join("ssd-world");
    fs::create_dir(&world).unwrap();
    let alias = directory.path().join("world");
    symlink(&world, &alias).unwrap();
    let mut progress = player(&alias.join("players"), true);
    progress.load().unwrap();
    assert!(
        progress
            .award(Advancement::STORY_ROOT, "crafting_table")
            .awarded
    );
    progress.save().await.unwrap();
    let actual = world
        .join("players/advancements")
        .join(progress.path.file_name().unwrap());
    assert!(actual.is_file());
    let bytes = fs::read(&actual).unwrap();
    progress.load().unwrap();
    assert!(progress.progress.map[Advancement::STORY_ROOT].is_done());
    assert_eq!(
        saved(&progress),
        serde_json::from_slice::<Value>(&bytes).unwrap()
    );
    assert_eq!(fs::read_link(alias).unwrap(), world);
}

#[test]
fn timestamp_offsets_pre_epoch_precision_and_range_are_checked() {
    let first = Timestamp::parse("2026-10-16 03:33:08 +0200".into()).unwrap();
    let second = Timestamp::parse("2026-10-15 20:33:08 -0500".into()).unwrap();
    assert_eq!(first.instant, second.instant);
    let before = Timestamp::parse("1969-12-31 23:59:59 +0000".into()).unwrap();
    assert_eq!(before.instant, UNIX_EPOCH - Duration::from_secs(1));
    assert_eq!(
        Timestamp::from_time(before.instant).unwrap().text,
        before.text
    );
    assert_eq!(
        Timestamp::from_time(UNIX_EPOCH - Duration::from_nanos(1))
            .unwrap()
            .text,
        before.text
    );
    assert_eq!(
        Timestamp::from_time(UNIX_EPOCH + Duration::from_millis(999))
            .unwrap()
            .text,
        "1970-01-01 00:00:00 +0000"
    );
    for text in [
        "2026-02-30 00:00:00 +0000",
        "2026-01-01 00:00:00 +1900",
        "2026-01-01T00:00:00Z",
        "0000-01-01 00:00:00 +0000",
    ] {
        assert!(Timestamp::parse(text.into()).is_err());
    }
    let outside = UNIX_EPOCH
        .checked_add(Duration::from_hours(70_389_528))
        .unwrap();
    assert!(matches!(
        Timestamp::from_time(outside),
        Err(AdvancementDataError::Timestamp(_))
    ));
    for version in [
        MAXIMUM_SUPPORTED_WORLD_DATA_VERSION - 1,
        MAXIMUM_SUPPORTED_WORLD_DATA_VERSION + 1,
    ] {
        assert!(
            matches!(decode(json!({"DataVersion": version}).to_string().as_bytes()), Err(AdvancementDataError::UnsupportedVersion(found)) if found == version)
        );
    }
}
