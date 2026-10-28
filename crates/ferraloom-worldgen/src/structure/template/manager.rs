use super::{Error, Identifier, StructureTemplate};
use ferraloom_data::datapack::ResourceManager;
use ferraloom_util::collections::JavaHashSet;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[cfg(test)]
mod tests;

/// Port of StructureTemplateManager's generated-directory/resource sources and
/// structureRepository. Returned immutable Arcs outlive eviction and reload.
#[derive(Debug, Default)]
pub struct StructureTemplateManager {
    /// Predecoded resource assets; construction fixtures may adjust these
    /// before publishing the manager or performing a lookup.
    pub templates: BTreeMap<Identifier, Arc<StructureTemplate>>,
    resources: ResourceManager,
    generated: Option<PathBuf>,
    // This is cold immutable-asset infrastructure, not game/tick state. The
    // guard covers only metadata lookup/insert, never IO, parsing or callbacks.
    repository: Mutex<BTreeMap<Identifier, Option<Arc<StructureTemplate>>>>,
}
impl Clone for StructureTemplateManager {
    fn clone(&self) -> Self {
        Self {
            templates: self.templates.clone(),
            resources: self.resources.clone(),
            generated: self.generated.clone(),
            repository: Mutex::new(
                self.repository
                    .lock()
                    .expect("template cache poisoned")
                    .clone(),
            ),
        }
    }
}
impl StructureTemplateManager {
    pub fn load_all(resources: &ResourceManager) -> Result<Self, Error> {
        let ids = resource_ids(resources);
        Self::load(resources, ids)
    }
    pub fn load(
        resources: &ResourceManager,
        ids: impl IntoIterator<Item = Identifier>,
    ) -> Result<Self, Error> {
        let mut templates = BTreeMap::new();
        for id in ids {
            let file = id.with_path(format!("structure/{}.nbt", id.path()))?;
            if resources.get(&file).is_some() {
                templates.insert(
                    id.clone(),
                    Arc::new(StructureTemplate::load(resources, &id)?),
                );
            }
        }
        Ok(Self {
            templates,
            resources: resources.clone(),
            ..Self::default()
        })
    }
    /// Bind the server's actual world root before sharing this manager.
    pub fn with_world_source(mut self, world: &Path) -> Self {
        self.generated = Some(world.join("generated"));
        self.repository
            .get_mut()
            .expect("template cache poisoned")
            .clear();
        self
    }
    fn find(&self, id: &Identifier) -> Option<Arc<StructureTemplate>> {
        if let Some(cached) = {
            self.repository
                .lock()
                .expect("template cache poisoned")
                .get(id)
                .cloned()
        } {
            return cached;
        }
        let loaded = self
            .load_generated(id)
            .or_else(|| self.templates.get(id).cloned())
            .or_else(|| {
                StructureTemplate::load(&self.resources, id)
                    .ok()
                    .map(Arc::new)
            });
        let mut repository = self.repository.lock().expect("template cache poisoned");
        if let Some(cached) = repository.get(id) {
            cached.clone()
        } else {
            repository.insert(id.clone(), loaded.clone());
            loaded
        }
    }
    fn load_generated(&self, id: &Identifier) -> Option<Arc<StructureTemplate>> {
        // PathPackResources rejects dot/empty path components before opening.
        if id
            .path()
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
        {
            return None;
        }
        let file = self
            .generated
            .as_ref()?
            .join(id.namespace())
            .join("structure")
            .join(format!("{}.nbt", id.path()));
        let bytes = std::fs::read(file).ok()?;
        StructureTemplate::load_bytes(&bytes, id).ok().map(Arc::new)
    }
    /// Native get caches misses as well as hits. Required callers retain the
    /// existing Rust missing-template error rather than inventing a template.
    pub fn get(&self, id: &Identifier) -> Result<Arc<StructureTemplate>, Error> {
        self.find(id)
            .ok_or_else(|| Error(format!("missing structure template {id}")))
    }
    /// Port of getOrCreate: a cached miss becomes a per-name empty template.
    pub fn get_or_create(&self, id: &Identifier) -> Arc<StructureTemplate> {
        if let Some(template) = self.find(id) {
            return template;
        }
        let template = Arc::new(StructureTemplate::default());
        self.repository
            .lock()
            .expect("template cache poisoned")
            .insert(id.clone(), Some(template.clone()));
        template
    }
    pub fn remove(&self, id: &Identifier) {
        self.repository
            .lock()
            .expect("template cache poisoned")
            .remove(id);
    }
    pub fn on_resource_manager_reload(&mut self, resources: ResourceManager) {
        self.resources = resources;
        self.templates.clear();
        self.repository
            .get_mut()
            .expect("template cache poisoned")
            .clear();
    }
    /// Source listing is independent of hits, misses and getOrCreate entries.
    /// Generated files precede resources; distinct keeps each first occurrence.
    pub fn list_templates(&self) -> Vec<Identifier> {
        let mut result = Vec::new();
        if let Some(generated) = &self.generated {
            let mut namespaces = JavaHashSet::<String>::new();
            if let Ok(entries) = std::fs::read_dir(generated) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if entry.path().is_dir() && Identifier::new(&name, "").is_ok() {
                        namespaces.insert(name);
                    }
                }
            }
            for namespace in &namespaces {
                let root = generated.join(namespace).join("structure");
                list_generated(&root, &root, namespace, &mut result);
            }
        }
        result.extend(resource_ids(&self.resources));
        let mut seen = BTreeSet::new();
        result.retain(|id| seen.insert(id.clone()));
        result
    }
}
fn resource_ids(resources: &ResourceManager) -> Vec<Identifier> {
    resources
        .list("structure/", ".nbt")
        .into_iter()
        .filter_map(|id| {
            let path = id.path().strip_prefix("structure/")?.strip_suffix(".nbt")?;
            id.with_path(path).ok()
        })
        .collect()
}
fn list_generated(directory: &Path, root: &Path, namespace: &str, result: &mut Vec<Identifier>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            list_generated(&path, root, namespace, result);
        } else if kind.is_file()
            && let Ok(relative) = path.strip_prefix(root)
        {
            let text = relative
                .iter()
                .map(|part| part.to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            if let Some(name) = text.strip_suffix(".nbt")
                && let Ok(id) = Identifier::new(namespace, name)
            {
                result.push(id);
            }
        }
    }
}
