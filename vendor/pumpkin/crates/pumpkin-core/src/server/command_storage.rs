//! Java 26.3 command storage: one gzip saved-data file per namespace.

use super::Server;
use pumpkin_nbt::{NbtCompound, nbt_compress::read_gzip_compound_tag, tag::NbtTag};
use pumpkin_util::identifier::Identifier;
use pumpkin_world::world_info::MAXIMUM_SUPPORTED_WORLD_DATA_VERSION;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::{self, File},
    io,
    path::{Component, Path},
};

type Entries = HashMap<String, NbtCompound>;

/// Only unmodeled root/data fields and namespace existence live here. Entries
/// remain in Server::command_storage so commands keep their existing API.
#[derive(Default)]
pub(super) struct NamespaceFiles {
    roots: BTreeMap<String, NbtCompound>,
}

#[derive(Default)]
struct Snapshot {
    namespaces: BTreeMap<String, NbtCompound>,
    first_error: Option<io::Error>,
}

impl Snapshot {
    fn record_error(&mut self, error: io::Error) {
        tracing::error!("Failed to snapshot command storage: {error}");
        self.first_error.get_or_insert(error);
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn validate_namespace(namespace: &str) -> io::Result<()> {
    // Java normalizes namespace "." to the data directory itself.
    if namespace == "." {
        return Ok(());
    }
    let mut components = Path::new(namespace).components();
    if !Identifier::is_valid_namespace(namespace)
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        return Err(invalid(format!(
            "Invalid command-storage namespace {namespace:?}"
        )));
    }
    Ok(())
}

fn namespace_directory(world_path: &Path, namespace: &str) -> std::path::PathBuf {
    let data = world_path.join("data");
    if namespace == "." {
        data
    } else {
        data.join(namespace)
    }
}

fn open_existing(path: &Path) -> io::Result<Option<File>> {
    match File::open(path) {
        Ok(file) => Ok(Some(file)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // A dangling symlink is an unreadable save, not an absent save.
            match fs::symlink_metadata(path) {
                Err(missing) if missing.kind() == io::ErrorKind::NotFound => Ok(None),
                _ => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

impl NamespaceFiles {
    pub(super) fn load(world_path: &Path) -> io::Result<(Entries, Self)> {
        let mut entries = Entries::new();
        let mut files = Self::default();
        let directory = world_path.join("data");
        let directory_entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if fs::symlink_metadata(&directory)
                    .is_err_and(|missing| missing.kind() == io::ErrorKind::NotFound)
                {
                    return Ok((entries, files));
                }
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        let mut namespaces = BTreeSet::new();
        let mut legacy_namespaces = BTreeSet::new();
        for entry in directory_entries {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let legacy_namespace = name
                .strip_prefix("command_storage_")
                .and_then(|name| name.strip_suffix(".dat"))
                .filter(|namespace| validate_namespace(namespace).is_ok());
            // Unrelated directories and dangling links are not saved namespaces.
            if validate_namespace(name).is_err() && legacy_namespace.is_none() {
                continue;
            }
            let kind = entry.file_type()?;
            let is_directory = if kind.is_symlink() {
                match fs::metadata(entry.path()) {
                    Ok(metadata) => metadata.is_dir(),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => false,
                    Err(error) => return Err(error),
                }
            } else {
                kind.is_dir()
            };
            if is_directory {
                namespaces.insert(name.to_owned());
            } else if name == "command_storage.dat" {
                namespaces.insert(".".to_owned());
            } else if let Some(namespace) = legacy_namespace {
                // DimensionStorageFileFix (4772) moved these names into namespace
                // directories. Keep the old file and prefer any modern file.
                namespaces.insert(namespace.to_owned());
                legacy_namespaces.insert(namespace.to_owned());
            }
        }
        for namespace in namespaces {
            let modern = namespace_directory(world_path, &namespace).join("command_storage.dat");
            let (path, file) = match open_existing(&modern)? {
                Some(file) => (modern, file),
                None => {
                    // A legal modern namespace directory can itself have the
                    // name command_storage_<namespace>.dat. It is not a legacy file.
                    if !legacy_namespaces.contains(&namespace) {
                        continue;
                    }
                    let legacy = directory.join(format!("command_storage_{namespace}.dat"));
                    let Some(file) = open_existing(&legacy)? else {
                        continue;
                    };
                    (legacy, file)
                }
            };
            let root = read_gzip_compound_tag(file)
                .map_err(|error| invalid(format!("{}: {error}", path.display())))?;
            let root = Self::decode(&namespace, root, &mut entries)
                .map_err(|error| invalid(format!("{}: {error}", path.display())))?;
            files.roots.insert(namespace, root);
        }
        Ok((entries, files))
    }

    fn decode(
        namespace: &str,
        mut root: NbtCompound,
        entries: &mut Entries,
    ) -> io::Result<NbtCompound> {
        if let Some(version) = root.get("DataVersion") {
            let version = version
                .extract_int()
                .ok_or_else(|| invalid("Invalid command-storage DataVersion"))?;
            if version > MAXIMUM_SUPPORTED_WORLD_DATA_VERSION {
                return Err(invalid(format!(
                    "Unsupported command-storage DataVersion {version}"
                )));
            }
        }
        let mut data = root
            .get_compound("data")
            .cloned()
            .ok_or_else(|| invalid("Missing command-storage data compound"))?;
        let contents = data
            .get_compound("contents")
            .ok_or_else(|| invalid("Missing command-storage contents compound"))?;
        for (path, tag) in &contents.child_tags {
            let id = Identifier::new(namespace.to_owned(), path.to_string())
                .map_err(|error| invalid(error.to_string()))?;
            let NbtTag::Compound(value) = tag else {
                return Err(invalid(format!("Invalid command-storage entry {id}")));
            };
            entries.insert(id.to_string(), value.clone());
        }
        data.child_tags.remove("contents");
        root.put_compound("data", data);
        Ok(root)
    }

    fn snapshot(&mut self, entries: &Entries) -> Snapshot {
        let mut snapshot = Snapshot::default();
        let mut failed_namespaces = BTreeSet::new();
        let mut contents = BTreeMap::<String, NbtCompound>::new();
        for (raw_id, value) in entries {
            let id = match Identifier::parse(raw_id) {
                Ok(id) => id,
                Err(error) => {
                    failed_namespaces.insert(namespace_from_id(raw_id).to_owned());
                    snapshot.record_error(invalid(error.to_string()));
                    continue;
                }
            };
            if let Err(error) = validate_namespace(id.namespace()) {
                failed_namespaces.insert(id.namespace().to_owned());
                snapshot.record_error(error);
                continue;
            }
            let contents = contents.entry(id.namespace().to_owned()).or_default();
            if !value.is_empty() {
                contents.put_compound(id.path(), value.clone());
            }
        }
        for namespace in contents.keys() {
            self.roots.entry(namespace.clone()).or_default();
        }
        for (namespace, preserved) in &self.roots {
            if let Err(error) = validate_namespace(namespace) {
                snapshot.record_error(error);
                continue;
            }
            // A malformed externally inserted ID must not erase other entries
            // from its namespace, or prevent healthy namespaces being saved.
            if failed_namespaces.contains(namespace) {
                continue;
            }
            let mut root = preserved.clone();
            root.put_int("DataVersion", MAXIMUM_SUPPORTED_WORLD_DATA_VERSION);
            let mut data = root.get_compound("data").cloned().unwrap_or_default();
            data.put_compound("contents", contents.remove(namespace).unwrap_or_default());
            root.put_compound("data", data);
            snapshot.namespaces.insert(namespace.clone(), root);
        }
        snapshot
    }

    fn write(world_path: &Path, snapshot: Snapshot) -> io::Result<()> {
        let mut first_error = snapshot.first_error;
        for (namespace, root) in snapshot.namespaces {
            let directory = namespace_directory(world_path, &namespace);
            let path = directory.join("command_storage.dat");
            let result = fs::create_dir_all(&directory).and_then(|()| {
                pumpkin_world::nbt_io::write_gzip_compound_atomic(&path, root).map_err(Into::into)
            });
            if let Err(error) = result {
                let error = io::Error::new(error.kind(), format!("{}: {error}", path.display()));
                tracing::error!("Failed to save command storage: {error}");
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

fn namespace_from_id(id: &str) -> &str {
    match id.split_once(':') {
        Some((namespace, _)) if !namespace.is_empty() => namespace,
        _ => "minecraft",
    }
}

impl Server {
    /// Check Java's saved-data namespace boundary before command access/mutation.
    pub(crate) fn validate_command_storage_id(id: &str) -> io::Result<()> {
        let id = Identifier::parse(id).map_err(|error| invalid(error.to_string()))?;
        validate_namespace(id.namespace())
    }

    /// Remember even an empty first write: Java retains that namespace's file.
    /// Call before taking the command_storage mutex.
    pub(crate) fn remember_command_storage_namespace(&self, id: &str) {
        let namespace = namespace_from_id(id);
        self.command_storage_files
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .roots
            .entry(namespace.to_owned())
            .or_default();
    }

    /// The caller holds SaveCoordinator through snapshot and durable replacement.
    pub(super) fn save_command_storage(&self) -> io::Result<()> {
        let entries = self
            .command_storage
            .lock()
            .map_err(|_| io::Error::other("Command storage lock poisoned"))?
            .clone();
        let snapshots = self
            .command_storage_files
            .lock()
            .map_err(|_| io::Error::other("Command-storage namespace lock poisoned"))?
            .snapshot(&entries);
        NamespaceFiles::write(&self.basic_config.get_world_path(), snapshots)
    }
}

#[cfg(test)]
mod tests;
