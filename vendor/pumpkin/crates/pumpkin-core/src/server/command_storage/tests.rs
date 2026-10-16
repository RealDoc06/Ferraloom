use super::*;
use crate::{command::CommandSource, data::VanillaData};
use pumpkin_config::{AdvancedConfiguration, BasicConfiguration, TelemetryConfig};
use pumpkin_util::world_seed::Seed;
use pumpkin_world::world_info::{LevelData, WorldInfoWriter, anvil::AnvilLevelInfo};
use std::{
    io::Cursor,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

// Actual Java 26.3 /data storage output, DataVersion 5023. The fixture includes
// lists, integer and float tags, and several independently named entries.
const VANILLA_STORAGE: &[u8] = include_bytes!("vanilla_26_3.dat");

async fn server_at(path: &Path) -> Result<Arc<Server>, pumpkin_world::world_info::WorldInfoError> {
    let basic = BasicConfiguration {
        default_level_name: path.to_string_lossy().into_owned(),
        seed: Seed(0),
        allow_nether: false,
        allow_end: false,
        allow_chat_reports: false,
        use_favicon: false,
        ..BasicConfiguration::default()
    };
    let mut advanced = AdvancedConfiguration::default();
    advanced.world.autosave_ticks = 1;
    advanced.networking.java.online_mode = false;
    advanced.networking.bedrock.online_mode = false;
    let data = VanillaData {
        banned_ip_list: std::sync::RwLock::default(),
        banned_player_list: std::sync::RwLock::default(),
        operator_config: std::sync::RwLock::default(),
        user_cache: std::sync::RwLock::default(),
        whitelist_config: std::sync::RwLock::default(),
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

fn metadata(path: &Path) {
    AnvilLevelInfo
        .write_world_info(&LevelData::default(Seed(0)), path)
        .unwrap();
}

fn storage_path(path: &Path, namespace: &str) -> std::path::PathBuf {
    path.join("data")
        .join(namespace)
        .join("command_storage.dat")
}

fn saved(path: &Path, namespace: &str) -> NbtCompound {
    read_gzip_compound_tag(File::open(storage_path(path, namespace)).unwrap()).unwrap()
}

fn entry(root: &NbtCompound, name: &str) -> NbtCompound {
    root.get_compound("data")
        .unwrap()
        .get_compound("contents")
        .unwrap()
        .get_compound(name)
        .unwrap()
        .clone()
}

fn value(answer: i32) -> NbtCompound {
    let mut value = NbtCompound::new();
    value.put_int("answer", answer);
    value
}

fn set(server: &Arc<Server>, id: &str, value: &NbtCompound) {
    let source = CommandSource {
        server: Some(server.clone()),
        ..CommandSource::dummy().with_world(server.worlds.load()[0].clone())
    };
    let dispatcher = server.command_dispatcher.load();
    let previous = server
        .command_storage
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .unwrap_or_default();
    for key in previous.child_tags.keys() {
        dispatcher
            .execute_input(
                &format!(
                    "data remove storage {id} {}",
                    serde_json::to_string(key).unwrap()
                ),
                &source,
            )
            .unwrap();
    }
    if value.is_empty() {
        // Exercise a namespace touched and emptied before its first save.
        dispatcher
            .execute_input(
                &format!("data modify storage {id} temporary set value 1"),
                &source,
            )
            .unwrap();
        dispatcher
            .execute_input(&format!("data remove storage {id} temporary"), &source)
            .unwrap();
    } else {
        dispatcher
            .execute_input(
                &format!(
                    "data merge storage {id} {}",
                    NbtTag::Compound(value.clone())
                ),
                &source,
            )
            .unwrap();
    }
}

async fn autosave(server: &Arc<Server>) {
    server.schedule_autosave();
    server.tasks.close();
    tokio::time::timeout(Duration::from_secs(15), server.tasks.wait())
        .await
        .expect("command-storage fixture tasks must finish");
    server.tasks.reopen();
}

#[tokio::test]
async fn vanilla_storage_roundtrips_namespaces_deleted_entries_and_unknown_fields() {
    let folder = tempfile::tempdir().unwrap();
    metadata(folder.path());
    let mut root = read_gzip_compound_tag(Cursor::new(VANILLA_STORAGE)).unwrap();
    assert_eq!(root.get_int("DataVersion"), Some(5023));
    let expected = entry(&root, "modify_destination");
    assert_eq!(expected.get_int("answer"), Some(42));
    assert_eq!(expected.get_float("entity_health"), Some(7.0));
    let path = storage_path(folder.path(), "ferraloom");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, VANILLA_STORAGE).unwrap();
    let (vanilla_entries, _) = NamespaceFiles::load(folder.path()).unwrap();
    assert_eq!(vanilla_entries["ferraloom:modify_destination"], expected);
    root.put_string("future_root", "keep".into());
    let mut data = root.get_compound("data").unwrap().clone();
    data.put_long("future_data", 123456789);
    root.put_compound("data", data);
    pumpkin_world::nbt_io::write_gzip_compound_atomic(&path, root).unwrap();

    let server = server_at(folder.path()).await.unwrap();
    assert_eq!(
        server.command_storage.lock().unwrap()["ferraloom:modify_destination"],
        expected
    );
    set(&server, "ferraloom:probe", &NbtCompound::new());
    set(&server, "ferraloom_other:folder/probe", &value(73));
    // Empty writes still create a namespace even when nothing ever populated it.
    set(&server, "empty:removed", &NbtCompound::new());
    server.save_all(true).await.unwrap();
    let written = saved(folder.path(), "ferraloom");
    assert_eq!(written.get_int("DataVersion"), Some(5023));
    assert_eq!(written.get_string("future_root"), Some("keep"));
    assert_eq!(
        written
            .get_compound("data")
            .unwrap()
            .get_long("future_data"),
        Some(123456789)
    );
    assert_eq!(entry(&written, "modify_destination"), expected);
    assert!(
        written
            .get_compound("data")
            .unwrap()
            .get_compound("contents")
            .unwrap()
            .get("probe")
            .is_none()
    );
    assert!(
        saved(folder.path(), "empty")
            .get_compound("data")
            .unwrap()
            .get_compound("contents")
            .unwrap()
            .is_empty()
    );
    server.shutdown().await;
    drop(server);

    let restarted = server_at(folder.path()).await.unwrap();
    assert_eq!(
        restarted.command_storage.lock().unwrap()["ferraloom_other:folder/probe"],
        value(73)
    );
    assert!(
        !restarted
            .command_storage
            .lock()
            .unwrap()
            .contains_key("ferraloom:probe")
    );
    // Removing the final entry must commit an empty namespace, not resurrect its
    // old contents when the next server loads it.
    set(
        &restarted,
        "ferraloom_other:folder/probe",
        &NbtCompound::new(),
    );
    restarted.save_all(false).await.unwrap();
    restarted.shutdown().await;
    drop(restarted);
    let (entries, _) = NamespaceFiles::load(folder.path()).unwrap();
    assert!(!entries.contains_key("ferraloom_other:folder/probe"));
    assert!(
        saved(folder.path(), "ferraloom_other")
            .get_compound("data")
            .unwrap()
            .get_compound("contents")
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn save_off_keeps_server_storage_autosave_manual_save_and_shutdown_enabled() {
    let folder = tempfile::tempdir().unwrap();
    metadata(folder.path());
    let server = server_at(folder.path()).await.unwrap();
    let overworld = server.worlds.load()[0].clone();
    overworld.level.save_enabled.store(false, Ordering::Relaxed);
    set(&server, "example:probe", &value(2));
    autosave(&server).await;
    assert_eq!(entry(&saved(folder.path(), "example"), "probe"), value(2));
    set(&server, "example:probe", &value(3));
    server.save_all(true).await.unwrap();
    assert_eq!(entry(&saved(folder.path(), "example"), "probe"), value(3));
    set(&server, "example:probe", &value(4));
    autosave(&server).await;
    assert_eq!(entry(&saved(folder.path(), "example"), "probe"), value(4));
    assert!(!overworld.level.save_enabled.load(Ordering::Relaxed));
    set(&server, "example:probe", &value(5));
    server.shutdown().await;
    assert_eq!(entry(&saved(folder.path(), "example"), "probe"), value(5));
    drop(overworld);
    drop(server);
    let restarted = server_at(folder.path()).await.unwrap();
    assert_eq!(
        restarted.command_storage.lock().unwrap()["example:probe"],
        value(5)
    );
    restarted.shutdown().await;
}

#[tokio::test]
async fn corrupt_storage_aborts_startup_without_replacing_it_or_creating_metadata() {
    let mut wrong_schema = NbtCompound::new();
    wrong_schema.put_int("DataVersion", 5023);
    wrong_schema.put_string("data", "wrong type".into());
    let mut future = read_gzip_compound_tag(Cursor::new(VANILLA_STORAGE)).unwrap();
    future.put_int("DataVersion", 5024);
    let mut wrong_entry = read_gzip_compound_tag(Cursor::new(VANILLA_STORAGE)).unwrap();
    let mut contents = NbtCompound::new();
    contents.put_int("probe", 42);
    let mut data = NbtCompound::new();
    data.put_compound("contents", contents);
    wrong_entry.put_compound("data", data);
    let cases = [
        b"broken gzip".to_vec(),
        pumpkin_nbt::nbt_compress::write_gzip_compound_tag_to_bytes(wrong_schema).unwrap(),
        pumpkin_nbt::nbt_compress::write_gzip_compound_tag_to_bytes(future).unwrap(),
        pumpkin_nbt::nbt_compress::write_gzip_compound_tag_to_bytes(wrong_entry).unwrap(),
    ];
    for bytes in cases {
        let folder = tempfile::tempdir().unwrap();
        let path = storage_path(folder.path(), "ferraloom");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &bytes).unwrap();
        let Err(error) = server_at(folder.path()).await else {
            panic!("corrupt command storage must fail startup");
        };
        assert!(error.to_string().contains("command_storage.dat"), "{error}");
        assert_eq!(fs::read(path).unwrap(), bytes);
        assert!(!folder.path().join("level.dat").exists());
    }
}

#[tokio::test]
async fn storage_save_failures_preserve_previous_file_and_attempt_other_namespaces_and_worlds() {
    let folder = tempfile::tempdir().unwrap();
    metadata(folder.path());
    let server = server_at(folder.path()).await.unwrap();
    set(&server, "alpha:probe", &value(2));
    set(&server, "zeta:probe", &value(3));
    server.save_all(false).await.unwrap();
    let path = storage_path(folder.path(), "alpha");
    let previous = fs::read(&path).unwrap();
    let mut invalid = value(9);
    invalid.put_string("oversized", "x".repeat(65_536));
    set(&server, "alpha:probe", &invalid);
    set(&server, "zeta:probe", &value(4));
    let world = server.worlds.load()[0].clone();
    world
        .custom_data
        .lock()
        .unwrap()
        .put_int("storage_failure_marker", 29);
    let error = server.save_all(true).await.unwrap_err();
    assert!(error.contains("command storage"), "{error}");
    assert_eq!(fs::read(&path).unwrap(), previous);
    assert_eq!(entry(&saved(folder.path(), "zeta"), "probe"), value(4));
    let bytes = fs::read(
        world
            .level
            .level_folder
            .dim_folder
            .join("pumpkin_custom_data.nbt"),
    )
    .unwrap();
    let custom = pumpkin_nbt::Nbt::read(&mut pumpkin_nbt::deserializer::NbtReadHelperJava::new(
        Cursor::new(bytes),
    ))
    .unwrap();
    assert_eq!(custom.root_tag.get_int("storage_failure_marker"), Some(29));

    // Exercise an actual rename failure too; failure in alpha must not prevent
    // zeta from committing its next independent namespace file.
    set(&server, "alpha:probe", &value(5));
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    fs::write(path.join("sentinel"), b"preserve").unwrap();
    set(&server, "zeta:probe", &value(6));
    let error = server.save_all(false).await.unwrap_err();
    assert!(error.contains("command storage"), "{error}");
    assert_eq!(fs::read(path.join("sentinel")).unwrap(), b"preserve");
    assert_eq!(entry(&saved(folder.path(), "zeta"), "probe"), value(6));
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    fs::remove_file(path.join("sentinel")).unwrap();
    fs::remove_dir(&path).unwrap();
    server.shutdown().await;
}

#[test]
fn namespace_cannot_escape_data_directory_but_entry_paths_can_contain_slashes() {
    for namespace in ["", "..", "../outside", "/absolute", "bad:name"] {
        assert!(validate_namespace(namespace).is_err(), "{namespace:?}");
    }
    assert!(Server::validate_command_storage_id(".:folder/probe").is_ok());
    assert!(Server::validate_command_storage_id("..:probe").is_err());
    let mut files = NamespaceFiles::default();
    let mut entries = Entries::new();
    entries.insert("valid:folder/probe".into(), value(1));
    let snapshot = files.snapshot(&entries);
    assert!(snapshot.first_error.is_none());
    assert_eq!(
        entry(&snapshot.namespaces["valid"], "folder/probe"),
        value(1)
    );
}

#[tokio::test]
async fn dot_namespace_persists_in_data_root_and_parent_namespace_fails_before_mutation() {
    let folder = tempfile::tempdir().unwrap();
    metadata(folder.path());
    let server = server_at(folder.path()).await.unwrap();
    let source = CommandSource {
        server: Some(server.clone()),
        ..CommandSource::dummy().with_world(server.worlds.load()[0].clone())
    };
    {
        let dispatcher = server.command_dispatcher.load();
        for command in [
            "data modify storage ..:probe answer set value 1",
            "data get storage ..:probe",
        ] {
            assert!(
                dispatcher.execute_input(command, &source).is_err(),
                "{command}"
            );
        }
    }
    assert!(server.command_storage.lock().unwrap().is_empty());
    assert!(
        server
            .command_storage_files
            .lock()
            .unwrap()
            .roots
            .is_empty()
    );
    set(&server, ".:folder/probe", &value(17));
    set(&server, "healthy:probe", &value(23));
    server.save_all(true).await.unwrap();
    assert!(folder.path().join("data/command_storage.dat").is_file());
    assert!(!folder.path().join("command_storage.dat").exists());
    assert_eq!(entry(&saved(folder.path(), "."), "folder/probe"), value(17));
    server.shutdown().await;
    drop(source);
    drop(server);

    let restarted = server_at(folder.path()).await.unwrap();
    let entries = restarted.command_storage.lock().unwrap().clone();
    assert_eq!(entries[".:folder/probe"], value(17));
    assert_eq!(entries["healthy:probe"], value(23));
    assert!(!entries.contains_key("..:probe"));
    restarted.shutdown().await;
}

#[tokio::test]
async fn legacy_java_storage_is_loaded_without_deleting_or_overriding_its_modern_replacement() {
    let folder = tempfile::tempdir().unwrap();
    metadata(folder.path());
    let data = folder.path().join("data");
    fs::create_dir_all(data.join("Foo")).unwrap();
    fs::write(data.join("Foo/command_storage.dat"), b"unrelated").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("missing", data.join("unrelated_link")).unwrap();
    let legacy = data.join("command_storage_ferraloom.dat");
    // Reuse actual Java entry NBT to exercise the pre-4772 filename layout.
    // This tests the filename migration, without claiming semantic data fixes.
    fs::write(&legacy, VANILLA_STORAGE).unwrap();
    let modern = storage_path(folder.path(), "ferraloom");
    let server = server_at(folder.path()).await.unwrap();
    assert_eq!(
        server.command_storage.lock().unwrap()["ferraloom:probe"],
        value(42)
    );
    assert!(!modern.exists());
    set(&server, "ferraloom:probe", &value(91));
    server.save_all(true).await.unwrap();
    assert_eq!(
        entry(&saved(folder.path(), "ferraloom"), "probe"),
        value(91)
    );
    assert_eq!(fs::read(&legacy).unwrap(), VANILLA_STORAGE);
    server.shutdown().await;
    drop(server);

    let restarted = server_at(folder.path()).await.unwrap();
    assert_eq!(
        restarted.command_storage.lock().unwrap()["ferraloom:probe"],
        value(91)
    );
    restarted.shutdown().await;
    drop(restarted);

    // A present corrupt modern file is authoritative; valid legacy data must
    // not mask it and allow a later save to replace the user's damaged file.
    fs::write(&modern, b"broken modern").unwrap();
    assert!(server_at(folder.path()).await.is_err());
    assert_eq!(fs::read(&modern).unwrap(), b"broken modern");
    assert_eq!(fs::read(&legacy).unwrap(), VANILLA_STORAGE);
    fs::remove_file(&modern).unwrap();
    fs::write(&legacy, b"broken legacy").unwrap();
    assert!(server_at(folder.path()).await.is_err());
    assert_eq!(fs::read(&legacy).unwrap(), b"broken legacy");
    assert!(!modern.exists());
}

#[tokio::test]
async fn namespace_named_like_a_legacy_file_survives_save_and_restart() {
    let folder = tempfile::tempdir().unwrap();
    metadata(folder.path());
    let data = folder.path().join("data");
    fs::create_dir_all(data.join("minecraft")).unwrap();
    let namespace = "command_storage_minecraft.dat";
    let id = format!("{namespace}:probe");
    let server = server_at(folder.path()).await.unwrap();
    set(&server, &id, &value(37));
    server.save_all(true).await.unwrap();
    assert!(data.join(namespace).is_dir());
    assert!(storage_path(folder.path(), namespace).is_file());
    assert!(!storage_path(folder.path(), "minecraft").exists());
    server.shutdown().await;
    drop(server);

    let restarted = server_at(folder.path()).await.unwrap();
    assert_eq!(restarted.command_storage.lock().unwrap()[&id], value(37));
    assert!(
        !restarted
            .command_storage
            .lock()
            .unwrap()
            .contains_key("minecraft:probe")
    );
    set(&restarted, &id, &value(41));
    restarted.save_all(true).await.unwrap();
    assert_eq!(entry(&saved(folder.path(), namespace), "probe"), value(41));
    assert!(!storage_path(folder.path(), "minecraft").exists());
    restarted.shutdown().await;
}

#[tokio::test]
async fn malformed_public_map_entries_do_not_erase_their_namespace_or_block_healthy_saves() {
    let folder = tempfile::tempdir().unwrap();
    metadata(folder.path());
    let server = server_at(folder.path()).await.unwrap();
    set(&server, "alpha:probe", &value(2));
    set(&server, "zeta:probe", &value(3));
    server.save_all(false).await.unwrap();
    let alpha = storage_path(folder.path(), "alpha");
    let previous = fs::read(&alpha).unwrap();
    set(&server, "alpha:probe", &value(4));
    set(&server, "zeta:probe", &value(5));
    {
        // External callers can mutate the retained public map directly. One
        // malformed key must neither clobber alpha nor starve valid zeta data.
        let mut entries = server.command_storage.lock().unwrap();
        entries.insert("alpha:bad path".into(), value(99));
        entries.insert("..:escape".into(), value(99));
    };
    let error = server.save_all(true).await.unwrap_err();
    assert!(error.contains("command storage"), "{error}");
    assert_eq!(fs::read(&alpha).unwrap(), previous);
    assert_eq!(entry(&saved(folder.path(), "zeta"), "probe"), value(5));
    assert!(!folder.path().join("command_storage.dat").exists());
    {
        let mut entries = server.command_storage.lock().unwrap();
        entries.remove("alpha:bad path");
        entries.remove("..:escape");
    };
    server.shutdown().await;
    assert_eq!(entry(&saved(folder.path(), "alpha"), "probe"), value(4));
}
