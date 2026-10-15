//! Java 26.3 command storage: one gzip saved-data file per namespace.

use super::Server;
use pumpkin_nbt::{NbtCompound, nbt_compress::read_gzip_compound_tag, tag::NbtTag};
use pumpkin_util::identifier::Identifier;
use pumpkin_world::world_info::MAXIMUM_SUPPORTED_WORLD_DATA_VERSION;
use std::{
    collections::{BTreeMap, HashMap},
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

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn validate_namespace(namespace: &str) -> io::Result<()> {
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
        let namespaces = match fs::read_dir(&directory) {
            Ok(namespaces) => namespaces,
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
        for namespace in namespaces {
            let namespace = namespace?;
            let kind = namespace.file_type()?;
            if !kind.is_dir() && (!kind.is_symlink() || !fs::metadata(namespace.path())?.is_dir()) {
                continue;
            }
            let path = namespace.path().join("command_storage.dat");
            let Some(file) = open_existing(&path)? else {
                continue;
            };
            let name = namespace
                .file_name()
                .into_string()
                .map_err(|_| invalid(format!("Invalid command-storage path {}", path.display())))?;
            validate_namespace(&name)?;
            let root = read_gzip_compound_tag(file)
                .map_err(|error| invalid(format!("{}: {error}", path.display())))?;
            let root = Self::decode(&name, root, &mut entries)
                .map_err(|error| invalid(format!("{}: {error}", path.display())))?;
            files.roots.insert(name, root);
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

    fn snapshot(&mut self, entries: &Entries) -> io::Result<BTreeMap<String, NbtCompound>> {
        let mut contents = BTreeMap::<String, NbtCompound>::new();
        for (id, value) in entries {
            let id = Identifier::parse(id).map_err(|error| invalid(error.to_string()))?;
            validate_namespace(id.namespace())?;
            let contents = contents.entry(id.namespace().to_owned()).or_default();
            if !value.is_empty() {
                contents.put_compound(id.path(), value.clone());
            }
        }
        for namespace in contents.keys() {
            self.roots.entry(namespace.clone()).or_default();
        }
        self.roots
            .iter()
            .map(|(namespace, preserved)| {
                validate_namespace(namespace)?;
                let mut root = preserved.clone();
                root.put_int("DataVersion", MAXIMUM_SUPPORTED_WORLD_DATA_VERSION);
                let mut data = root.get_compound("data").cloned().unwrap_or_default();
                data.put_compound("contents", contents.remove(namespace).unwrap_or_default());
                root.put_compound("data", data);
                Ok((namespace.clone(), root))
            })
            .collect()
    }

    fn write(world_path: &Path, snapshots: BTreeMap<String, NbtCompound>) -> io::Result<()> {
        let mut first_error = None;
        for (namespace, root) in snapshots {
            let directory = world_path.join("data").join(namespace);
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

impl Server {
    /// Remember even an empty first write: Java retains that namespace's file.
    /// Call before taking the command_storage mutex.
    pub(crate) fn remember_command_storage_namespace(&self, id: &str) {
        let namespace = id
            .split_once(':')
            .map_or("minecraft", |(namespace, _)| namespace);
        let namespace = if namespace.is_empty() {
            "minecraft"
        } else {
            namespace
        };
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
            .snapshot(&entries)?;
        NamespaceFiles::write(&self.basic_config.get_world_path(), snapshots)
    }
}

#[cfg(test)]
mod tests;
