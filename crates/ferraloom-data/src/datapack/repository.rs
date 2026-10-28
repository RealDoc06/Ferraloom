//! Ports of PackRepository, ServerPacksSource and FolderRepositorySource.
use super::{
    DataPackConfig, Error, FeatureFlags, Pack, ResourceManager, WorldDataConfiguration,
    metadata::{MetadataCodecs, PackMetadata},
    strict_json, vanilla_archive,
};
use ferraloom_util::collections::LinkedHashSet;
use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackSource {
    BuiltIn,
    Feature,
    World,
}
impl PackSource {
    fn automatic(self) -> bool {
        self != Self::Feature
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    Bottom,
    Top,
}
#[derive(Clone, Debug)]
enum Location {
    Directory(PathBuf),
    Zip(PathBuf),
    BuiltIn { bytes: Arc<[u8]>, prefix: String },
}

pub struct RepositoryPack<D> {
    pub id: String,
    pub source: PackSource,
    pub metadata: PackMetadata<D>,
    pub required: bool,
    pub fixed_position: bool,
    pub default_position: Position,
    location: Location,
    filter: Option<serde_json::Value>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Info,
    Warn,
}
#[derive(Clone, Debug)]
pub struct PackDiagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
}
/// The available collection follows TreeMap<String,Pack>'s UTF-16 key order.
/// Selected indices refer to these same owned definitions, never another list.
pub struct PackRepository<D> {
    available: Vec<RepositoryPack<D>>,
    selected: Vec<usize>,
    diagnostics: Vec<PackDiagnostic>,
}
impl<D> PackRepository<D> {
    pub fn discover(
        jar: &Path,
        folder: &Path,
        custom_allowlist: Option<&Path>,
        codecs: &impl MetadataCodecs<Description = D>,
    ) -> Result<Self, Error> {
        let bytes = vanilla_archive(jar)?;
        let mut repository = Self {
            available: Vec::new(),
            selected: Vec::new(),
            diagnostics: Vec::new(),
        };
        let vanilla = serde_json::json!({"pack":{"description":{"translate":"dataPack.vanilla.description"},"min_format":121,"max_format":[121,2147483647]},"features":{"enabled":["minecraft:vanilla"]}});
        repository.insert(
            "vanilla".into(),
            PackSource::BuiltIn,
            Location::BuiltIn {
                bytes: bytes.clone(),
                prefix: String::new(),
            },
            vanilla,
            codecs,
        );
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes.clone()))?;
        let mut builtins = archive
            .file_names()
            .filter_map(|name| {
                name.strip_prefix("data/minecraft/datapacks/")?
                    .strip_suffix("/pack.mcmeta")
                    .map(str::to_owned)
            })
            .collect::<Vec<_>>();
        builtins.sort();
        builtins.dedup();
        for id in builtins {
            let prefix = format!("data/minecraft/datapacks/{id}/");
            let mut raw = Vec::new();
            archive
                .by_name(&format!("{prefix}pack.mcmeta"))?
                .read_to_end(&mut raw)?;
            match strict_json(&raw) {
                Ok(metadata) => repository.insert(
                    id,
                    PackSource::Feature,
                    Location::BuiltIn {
                        bytes: bytes.clone(),
                        prefix,
                    },
                    metadata,
                    codecs,
                ),
                Err(error) => {
                    repository.warn(format!("Failed to read pack {id} metadata: {error}"))
                }
            }
        }
        match fs::create_dir_all(folder).and_then(|()| fs::read_dir(folder)) {
            Ok(entries) => {
                for entry in entries {
                    let entry = match entry {
                        Ok(entry) => entry,
                        Err(error) => {
                            repository.warn(format!(
                                "Failed to list packs in {}: {error}",
                                folder.display()
                            ));
                            continue;
                        }
                    };
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let id = format!("file/{name}");
                    match discover_world_pack(&path, &id, custom_allowlist) {
                        Ok(Some(location)) => match read_metadata(&location) {
                            Ok(metadata) => {
                                repository.insert(id, PackSource::World, location, metadata, codecs)
                            }
                            Err(error) => repository
                                .warn(format!("Failed to read pack {id} metadata: {error}")),
                        },
                        Ok(None) => repository.info(format!(
                            "Found non-pack entry '{}', ignoring",
                            path.display()
                        )),
                        Err(error @ Error::UnsupportedPack { .. }) => return Err(error),
                        Err(error) => repository.warn(format!(
                            "Failed to read properties of '{}', ignoring: {error}",
                            path.display()
                        )),
                    }
                }
            }
            Err(error) => repository.warn(format!(
                "Failed to list packs in {}: {error}",
                folder.display()
            )),
        }
        repository
            .available
            .sort_by(|a, b| a.id.encode_utf16().cmp(b.id.encode_utf16()));
        Ok(repository)
    }
    fn insert(
        &mut self,
        id: String,
        source: PackSource,
        location: Location,
        value: serde_json::Value,
        codecs: &impl MetadataCodecs<Description = D>,
    ) {
        match PackMetadata::decode(&value, codecs) {
            Ok(metadata) => {
                if let Some(error) = &metadata.format_fallback {
                    self.warn(format!(
                        "Error reading pack metadata for {id}, attempting fallback type: {error}"
                    ));
                }
                let pack = RepositoryPack {
                    id: id.clone(),
                    source,
                    metadata,
                    required: false,
                    fixed_position: false,
                    default_position: if source == PackSource::BuiltIn {
                        Position::Bottom
                    } else {
                        Position::Top
                    },
                    location,
                    filter: value.get("filter").cloned(),
                };
                if let Some(index) = self.available.iter().position(|pack| pack.id == id) {
                    self.available[index] = pack;
                } else {
                    self.available.push(pack);
                }
            }
            Err(error) => self.warn(format!("Failed to read pack {id} metadata: {error}")),
        }
    }
    fn info(&mut self, message: String) {
        self.diagnostics.push(PackDiagnostic {
            level: DiagnosticLevel::Info,
            message,
        });
    }
    fn warn(&mut self, message: String) {
        self.diagnostics.push(PackDiagnostic {
            level: DiagnosticLevel::Warn,
            message,
        });
    }
    pub fn diagnostics(&self) -> &[PackDiagnostic] {
        &self.diagnostics
    }
    pub fn available(&self) -> impl Iterator<Item = &RepositoryPack<D>> {
        self.available.iter()
    }
    pub fn selected(&self) -> impl Iterator<Item = &RepositoryPack<D>> {
        self.selected.iter().map(|&index| &self.available[index])
    }
    pub fn get(&self, id: &str) -> Option<&RepositoryPack<D>> {
        self.available.iter().find(|pack| pack.id == id)
    }
    fn index(&self, id: &str) -> Option<usize> {
        self.available.iter().position(|pack| pack.id == id)
    }
    /// Port of PackRepository#rebuildSelected, including required pack insertion.
    pub fn set_selected<'a>(&mut self, ids: impl IntoIterator<Item = &'a str>) {
        let mut selected = ids
            .into_iter()
            .filter_map(|id| self.index(id))
            .collect::<Vec<_>>();
        for (index, pack) in self.available.iter().enumerate() {
            if !pack.required || selected.contains(&index) {
                continue;
            }
            let position = match pack.default_position {
                Position::Bottom => selected
                    .iter()
                    .position(|&i| {
                        !self.available[i].fixed_position
                            || self.available[i].default_position != Position::Bottom
                    })
                    .unwrap_or(selected.len()),
                Position::Top => selected
                    .iter()
                    .rposition(|&i| {
                        !self.available[i].fixed_position
                            || self.available[i].default_position != Position::Top
                    })
                    .map_or(0, |index| index + 1),
            };
            selected.insert(position, index);
        }
        self.selected = selected;
    }
    pub fn selected_ids(&self) -> Vec<String> {
        let mut ids = LinkedHashSet::new();
        for pack in self.selected() {
            ids.insert(pack.id.clone());
        }
        ids.iter().cloned().collect()
    }
    fn requested_features(&self) -> FeatureFlags {
        self.selected().fold(FeatureFlags::EMPTY, |flags, pack| {
            flags.join(pack.metadata.features)
        })
    }
    /// Port of MinecraftServer#configurePackRepository and forced-feature packs.
    pub fn configure(
        &mut self,
        initial: &WorldDataConfiguration,
        init_mode: bool,
        safe_mode: bool,
    ) -> Result<WorldDataConfiguration, Error> {
        let forced = if init_mode {
            FeatureFlags::EMPTY
        } else {
            initial.enabled_features
        };
        let allowed = if init_mode {
            FeatureFlags::ALL
        } else {
            initial.enabled_features
        };
        let mut selected = LinkedHashSet::new();
        if safe_mode {
            selected.insert("vanilla".into());
        } else {
            for id in &initial.data_packs.enabled {
                if self.index(id).is_some() {
                    selected.insert(id.clone());
                } else {
                    self.warn(format!("Missing data pack {id}"));
                }
            }
            for index in 0..self.available.len() {
                let pack = &self.available[index];
                let id = pack.id.clone();
                if initial.data_packs.disabled.contains(&id) {
                    continue;
                }
                let features = pack.metadata.features;
                let was_selected = selected.contains(&id);
                if !was_selected && pack.source.automatic() {
                    if features.is_subset_of(allowed) {
                        self.info(format!(
                            "Found new data pack {id}, loading it automatically"
                        ));
                        selected.insert(id.clone());
                    } else {
                        self.info(format!("Found new data pack {id}, but can't load it due to missing features {}",features.subtract(allowed).names().join(", ")));
                    }
                }
                if was_selected && !features.is_subset_of(allowed) {
                    self.warn(format!("Pack {id} requires features {} that are not enabled for this world, disabling pack.",features.subtract(allowed).names().join(", ")));
                    selected.remove(&id);
                }
            }
            if selected.is_empty() {
                self.info("No datapacks selected, forcing vanilla".into());
                selected.insert("vanilla".into());
            }
        }
        self.set_selected(selected.iter().map(String::as_str));
        let mut missing = forced.subtract(self.requested_features());
        if !missing.is_empty() {
            let mut ids = LinkedHashSet::new();
            for id in self.selected_ids() {
                ids.insert(id);
            }
            for index in 0..self.available.len() {
                let pack = &self.available[index];
                if missing.is_empty() {
                    break;
                }
                let features = pack.metadata.features;
                if pack.source == PackSource::Feature
                    && !features.is_empty()
                    && features.intersects(missing)
                    && features.is_subset_of(forced)
                {
                    let id = pack.id.clone();
                    if !ids.insert(id.clone()) {
                        return Err(Error::InvalidData(format!(
                            "Tried to force '{id}', but it was already enabled"
                        )));
                    }
                    self.info(format!(
                        "Found feature pack ('{id}') for requested feature, forcing to enabled"
                    ));
                    missing = missing.subtract(features);
                }
            }
            self.set_selected(ids.iter().map(String::as_str));
        }
        let enabled = self.selected_ids();
        let disabled = if safe_mode {
            Vec::new()
        } else {
            self.available
                .iter()
                .filter(|pack| !enabled.contains(&pack.id))
                .map(|pack| pack.id.clone())
                .collect()
        };
        Ok(WorldDataConfiguration {
            data_packs: Arc::new(DataPackConfig { enabled, disabled }),
            enabled_features: self.requested_features().join(forced),
        })
    }
    pub fn open_selected(&self) -> Result<ResourceManager, Error> {
        let mut packs = Vec::new();
        for pack in self.selected() {
            if let Some(filter) = &pack.filter {
                return Err(Error::UnsupportedPack {
                    pack: pack.id.clone(),
                    operation: format!("resource filter {filter} requires Java regex patterns"),
                });
            }
            let mut resources = open_location(&pack.id, &pack.location, "")?;
            for overlay in &pack.metadata.overlays {
                resources.overlay(open_location(
                    &pack.id,
                    &pack.location,
                    &format!("{overlay}/"),
                )?);
            }
            packs.push(resources);
        }
        Ok(ResourceManager::new(packs))
    }
}

fn discover_world_pack(
    path: &Path,
    id: &str,
    custom_allowlist: Option<&Path>,
) -> Result<Option<Location>, Error> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        unsupported_allowlist(id, custom_allowlist)?;
        return Err(Error::InvalidData(format!(
            "Forbidden symbolic link {}",
            path.display()
        )));
    }
    if metadata.is_dir() {
        reject_symlinks(path, id, custom_allowlist)?;
        Ok(path
            .join("pack.mcmeta")
            .is_file()
            .then(|| Location::Directory(path.to_owned())))
    } else {
        Ok((metadata.is_file()
            && path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(".zip")))
        .then(|| Location::Zip(path.to_owned())))
    }
}
fn reject_symlinks(
    directory: &Path,
    id: &str,
    custom_allowlist: Option<&Path>,
) -> Result<(), Error> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            unsupported_allowlist(id, custom_allowlist)?;
            return Err(Error::InvalidData(format!(
                "Forbidden symbolic link {}",
                entry.path().display()
            )));
        }
        if kind.is_dir() {
            reject_symlinks(&entry.path(), id, custom_allowlist)?;
        }
    }
    Ok(())
}
fn unsupported_allowlist(id: &str, custom_allowlist: Option<&Path>) -> Result<(), Error> {
    if let Some(path) = custom_allowlist {
        return Err(Error::UnsupportedPack {
            pack: id.into(),
            operation: format!("custom symbolic-link allowlist {}", path.display()),
        });
    }
    Ok(())
}
fn read_metadata(location: &Location) -> Result<serde_json::Value, Error> {
    let bytes = match location {
        Location::Directory(path) => fs::read(path.join("pack.mcmeta"))?,
        Location::Zip(path) => {
            let mut archive = zip::ZipArchive::new(fs::File::open(path)?)?;
            let mut bytes = Vec::new();
            archive.by_name("pack.mcmeta")?.read_to_end(&mut bytes)?;
            bytes
        }
        Location::BuiltIn { bytes, prefix } => {
            let mut archive = zip::ZipArchive::new(Cursor::new(bytes.clone()))?;
            let mut bytes = Vec::new();
            archive
                .by_name(&format!("{prefix}pack.mcmeta"))?
                .read_to_end(&mut bytes)?;
            bytes
        }
    };
    strict_json(&bytes)
}
fn open_location(id: &str, location: &Location, overlay: &str) -> Result<Pack, Error> {
    match location {
        Location::Directory(path) => Pack::from_directory(id, path.join(overlay).as_path()),
        Location::Zip(path) => Pack::from_zip_root(id, fs::File::open(path)?, overlay),
        Location::BuiltIn { bytes, prefix } => Pack::from_zip_root(
            id,
            Cursor::new(bytes.clone()),
            &format!("{prefix}{overlay}"),
        ),
    }
}

#[cfg(all(test, unix))]
mod tests;
