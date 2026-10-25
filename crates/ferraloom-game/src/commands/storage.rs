//! Port of CommandStorage and its per-namespace SavedData containers.
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::Identifier;
use ferraloom_storage::{SavedData, SavedDataCodec, SavedDataStorage};
use ferraloom_util::collections::JavaHashMap;
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug)]
pub struct StorageReadError {
    pub namespace: String,
    pub source: Arc<ferraloom_storage::Error>,
}
impl std::fmt::Display for StorageReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Could not load data {}:command_storage: {}",
            self.namespace, self.source
        )
    }
}
impl std::error::Error for StorageReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}
/// Port of CommandStorage.Container.CODEC. Keys are resource paths, not full IDs.
#[derive(Clone, Debug, Default)]
pub struct StorageContainer {
    contents: JavaHashMap<String, CompoundTag>,
}
impl SavedDataCodec for StorageContainer {
    fn decode(tag: &Tag) -> ferraloom_storage::Result<Self> {
        let map = tag
            .as_compound()
            .and_then(|map| map.get_compound("contents"))
            .ok_or_else(|| {
                ferraloom_storage::Error::Invalid("command storage has no contents map".into())
            })?;
        // HashMap(Map) sizes its table from the source size before insertion.
        let mut contents = if map.is_empty() {
            JavaHashMap::new()
        } else {
            JavaHashMap::with_capacity((map.len() as f32 / 0.75 + 1.) as usize)
        };
        for (key, value) in map.iter_java_order() {
            let key = key.to_string_lossy();
            if !key.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"/._-".contains(&byte)
            }) {
                return Err(ferraloom_storage::Error::Invalid(format!(
                    "Non [a-z0-9/._-] character in path: {key}"
                )));
            }
            let value = value.as_compound().ok_or_else(|| {
                ferraloom_storage::Error::Invalid(format!(
                    "command storage value {key} is not a compound"
                ))
            })?;
            contents.insert(key, value.clone());
        }
        Ok(Self { contents })
    }
    fn encode(&self) -> ferraloom_storage::Result<Tag> {
        let mut contents = CompoundTag::new();
        for (key, value) in self.contents.iter() {
            contents.insert(key.as_str(), Tag::Compound(value.clone()));
        }
        let mut result = CompoundTag::new();
        result.insert("contents", Tag::Compound(contents));
        Ok(Tag::Compound(result))
    }
}
#[derive(Debug, Default)]
struct Touches {
    loaded: JavaHashMap<String, ()>,
}
type LoadedContainer = Result<SavedData<StorageContainer>, Arc<ferraloom_storage::Error>>;
/// Read-only data publication. The shared cache touch changes discovery/order,
/// never stored NBT; the coordinator-approved guard is released before returning.
#[derive(Clone, Debug, Default)]
pub struct CommandStorageSnapshot {
    namespaces: JavaHashMap<String, LoadedContainer>,
    discovery_error: Option<Arc<ferraloom_storage::Error>>,
    /// Approved exception: cold, uncontended, metadata-only lock, never held across calls; the Arc snapshot keeps Level Send and stays correct under phase-5 parallel regions.
    touches: Arc<Mutex<Touches>>,
}
impl CommandStorageSnapshot {
    pub fn get(&self, id: &Identifier) -> Result<CompoundTag, StorageReadError> {
        let namespace = id.namespace();
        let loaded = self.namespaces.peek(&namespace.to_owned());
        {
            let mut touches = self.touches.lock().expect("command storage cache poisoned");
            if matches!(loaded, Some(Ok(_))) {
                touches.loaded.insert(namespace.to_owned(), ());
            }
        }
        let data = match loaded {
            Some(Ok(data)) => Some(data),
            Some(Err(error)) => {
                return Err(StorageReadError {
                    namespace: namespace.into(),
                    source: error.clone(),
                });
            }
            None => {
                if let Some(error) = &self.discovery_error {
                    return Err(StorageReadError {
                        namespace: namespace.into(),
                        source: error.clone(),
                    });
                }
                None
            }
        };
        Ok(data
            .and_then(|data| data.value().contents.peek(&id.path().to_owned()))
            .cloned()
            .unwrap_or_default())
    }
    /// Native keys() exposes only namespace containers touched by get/set.
    pub fn keys(&self) -> Vec<Identifier> {
        let namespaces = self
            .touches
            .lock()
            .expect("command storage cache poisoned")
            .loaded
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let mut keys = Vec::new();
        for namespace in namespaces {
            if let Some(Ok(data)) = self.namespaces.peek(&namespace) {
                for path in data.value().contents.keys() {
                    keys.push(
                        Identifier::parse(&format!("{namespace}:{path}"))
                            .expect("validated storage key"),
                    );
                }
            }
        }
        keys
    }
}
/// The server owns the sole mutable store. Level receives only snapshot().
#[derive(Default)]
pub struct CommandStorage {
    current: Arc<CommandStorageSnapshot>,
}
impl CommandStorage {
    /// Preload off tick but retain failed loads until the native first-access slot.
    pub fn preload(folder: &Path) -> Self {
        let mut snapshot = CommandStorageSnapshot::default();
        let storage = SavedDataStorage::new(folder);
        let entries = match std::fs::read_dir(folder) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(error) => {
                snapshot.discovery_error = Some(Arc::new(error.into()));
                return Self {
                    current: Arc::new(snapshot),
                };
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    snapshot.discovery_error = Some(Arc::new(error.into()));
                    continue;
                }
            };
            let Some(namespace) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Ok(id) = Identifier::parse(&format!("{namespace}:command_storage")) else {
                continue;
            };
            match storage.load::<StorageContainer>(id.as_str()) {
                Ok(Some(data)) => {
                    snapshot.namespaces.insert(namespace, Ok(data));
                }
                Ok(None) => {}
                Err(error) => {
                    snapshot.namespaces.insert(namespace, Err(Arc::new(error)));
                }
            }
        }
        Self {
            current: Arc::new(snapshot),
        }
    }
    pub fn snapshot(&self) -> Arc<CommandStorageSnapshot> {
        self.current.clone()
    }
    pub fn get(&self, id: &Identifier) -> Result<CompoundTag, StorageReadError> {
        self.current.get(id)
    }
    pub fn keys(&self) -> Vec<Identifier> {
        self.current.keys()
    }
    /// Port of mutations through Container#get's aliased compound. A failed
    /// /data operation can change an existing value without Container#put, so
    /// neither the key's existence nor the current dirty flag changes here.
    pub fn publish_read_mutation(
        &mut self,
        id: &Identifier,
        value: &CompoundTag,
    ) -> Result<(), StorageReadError> {
        self.get(id)?;
        let snapshot = Arc::make_mut(&mut self.current);
        if let Some(Ok(data)) = snapshot.namespaces.get_mut(&id.namespace().to_owned())
            && data.value().contents.contains_key(&id.path().to_owned())
        {
            let dirty = data.is_dirty();
            data.value_mut()
                .contents
                .insert(id.path().to_owned(), value.clone());
            data.set_dirty(dirty);
        }
        Ok(())
    }
    /// Port of Container#put: empty compounds remove entries; every set dirties.
    /// Canonical callers are server-exclusive /data handlers, never Level readers.
    pub fn set(&mut self, id: &Identifier, contents: CompoundTag) -> Result<(), StorageReadError> {
        self.get(id)?;
        let namespace = id.namespace().to_owned();
        let snapshot = Arc::make_mut(&mut self.current);
        let entry = snapshot.namespaces.get_mut(&namespace);
        if entry.is_none() {
            snapshot.namespaces.insert(
                namespace.clone(),
                Ok(SavedData::new(StorageContainer::default())),
            );
        }
        let data = snapshot
            .namespaces
            .get_mut(&namespace)
            .expect("container created")
            .as_mut()
            .expect("failed load returned above");
        if contents.is_empty() {
            data.value_mut().contents.remove(&id.path().to_owned());
        } else {
            data.value_mut().contents.insert(id.path().into(), contents);
        }
        snapshot
            .touches
            .lock()
            .expect("command storage cache poisoned")
            .loaded
            .insert(namespace, ());
        Ok(())
    }
    /// Native scheduleSave captures dirty payloads and clears their flags before IO.
    pub fn take_dirty(&mut self) -> Vec<(String, SavedData<StorageContainer>)> {
        let snapshot = Arc::make_mut(&mut self.current);
        let namespaces = snapshot.namespaces.keys().cloned().collect::<Vec<_>>();
        let mut dirty = Vec::new();
        for namespace in namespaces {
            if let Some(Ok(data)) = snapshot.namespaces.get_mut(&namespace)
                && data.is_dirty()
            {
                dirty.push((format!("{namespace}:command_storage"), data.clone()));
                data.set_dirty(false);
            }
        }
        dirty
    }
}

#[cfg(test)]
mod tests;
