use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ferraloom-command-storage-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[derive(serde::Deserialize)]
struct Step {
    op: String,
    id: Option<String>,
    input: Option<String>,
    result: Option<String>,
    keys: Vec<String>,
    dirty: std::collections::BTreeMap<String, bool>,
}
fn dirty(storage: &CommandStorage) -> std::collections::BTreeMap<String, bool> {
    let namespaces = storage
        .current
        .touches
        .lock()
        .unwrap()
        .loaded
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    namespaces
        .into_iter()
        .map(|namespace| {
            let data = storage
                .current
                .namespaces
                .get(&namespace)
                .unwrap()
                .as_ref()
                .unwrap();
            (namespace, data.is_dirty())
        })
        .collect()
}
#[test]
fn native_get_set_discovery_dirty_flags_and_saved_container_match() {
    let vectors: Vec<Step> = serde_json::from_str(include_str!(
        "../vanilla/storage/command-storage-vectors.json"
    ))
    .unwrap();
    let directory = Directory::new();
    let disk = SavedDataStorage::new(&directory.0);
    let mut commands = CommandStorage::preload(&directory.0);
    let mut first_save = true;
    for step in vectors {
        let id = step.id.as_deref().map(|id| Identifier::parse(id).unwrap());
        match step.op.as_str() {
            "start" => {}
            "reload" => commands = CommandStorage::preload(&directory.0),
            "get" => {
                let value = commands.get(id.as_ref().unwrap()).unwrap();
                assert_eq!(
                    value,
                    ferraloom_nbt::parse_compound(step.result.as_deref().unwrap()).unwrap()
                );
            }
            "set" => commands
                .set(
                    id.as_ref().unwrap(),
                    ferraloom_nbt::parse_compound(step.input.as_deref().unwrap()).unwrap(),
                )
                .unwrap(),
            "save" => {
                for (id, mut data) in commands.take_dirty() {
                    disk.save(&id, &mut data).unwrap();
                }
                if first_save {
                    let saved = disk
                        .load::<StorageContainer>("test:command_storage")
                        .unwrap()
                        .unwrap()
                        .to_nbt()
                        .unwrap();
                    let expected = ferraloom_nbt::parse_compound(include_str!(
                        "../vanilla/storage/native-command-storage.snbt"
                    ))
                    .unwrap();
                    assert_eq!(saved, expected);
                    first_save = false;
                }
            }
            op => panic!("unknown fixture operation {op}"),
        }
        assert_eq!(
            commands
                .keys()
                .into_iter()
                .map(|key| key.to_string())
                .collect::<Vec<_>>(),
            step.keys,
            "{} {:?}",
            step.op,
            step.id
        );
        assert_eq!(dirty(&commands), step.dirty, "{} {:?}", step.op, step.id);
    }
}
#[test]
fn real_native_saved_file_loads_and_level_reads_share_namespace_touches() {
    let directory = Directory::new();
    std::fs::create_dir_all(directory.0.join("test")).unwrap();
    std::fs::write(
        directory.0.join("test/command_storage.dat"),
        include_bytes!("../vanilla/storage/native-command-storage.dat"),
    )
    .unwrap();
    let mut commands = CommandStorage::preload(&directory.0);
    assert!(commands.keys().is_empty());
    static BEHAVIORS: crate::Behaviors = crate::Behaviors::new();
    let mut level = crate::level::Level::new(
        crate::level::Region::new(&BEHAVIORS),
        crate::level::LevelContext::fixture(0, 63),
    );
    assert!(level.command_storage().is_none());
    level.set_command_storage(commands.snapshot());
    assert_eq!(
        level
            .command_storage()
            .unwrap()
            .get(&Identifier::parse("test:an").unwrap())
            .unwrap(),
        ferraloom_nbt::parse_compound("{collision:1}").unwrap()
    );
    assert_eq!(
        commands
            .keys()
            .into_iter()
            .map(|key| key.to_string())
            .collect::<Vec<_>>(),
        ["test:an", "test:c0"]
    );
    assert!(commands.take_dirty().is_empty());
}
#[test]
fn failed_namespace_load_is_deferred_to_reads_and_preserves_its_error() {
    let directory = Directory::new();
    std::fs::create_dir_all(directory.0.join("broken")).unwrap();
    std::fs::write(directory.0.join("broken/command_storage.dat"), b"not NBT").unwrap();
    let commands = CommandStorage::preload(&directory.0);
    assert!(commands.keys().is_empty());
    assert!(
        commands
            .get(&Identifier::parse("absent:key").unwrap())
            .unwrap()
            .is_empty()
    );
    let snapshot = commands.snapshot();
    let error = snapshot
        .get(&Identifier::parse("broken:key").unwrap())
        .unwrap_err();
    assert_eq!(error.namespace, "broken");
    assert!(std::error::Error::source(&error).is_some());
    assert!(commands.keys().is_empty());
}
