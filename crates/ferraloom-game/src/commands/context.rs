//! Port of CommandBuildContext's registry/feature view. Reload replaces the owned view.
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default)]
pub struct CommandBuildContext {
    pub registries: BTreeMap<String, Vec<String>>,
    pub tags: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}
impl CommandBuildContext {
    pub fn entries(&self, registry: &str) -> Vec<&str> {
        self.registries.get(registry).map_or_else(
            || {
                ferraloom_data::registry_entries(registry)
                    .unwrap_or_default()
                    .to_vec()
            },
            |entries| entries.iter().map(String::as_str).collect(),
        )
    }
    pub fn contains(&self, registry: &str, id: &str) -> bool {
        self.registries.get(registry).map_or_else(
            || {
                ferraloom_data::registry_entries(registry)
                    .is_some_and(|entries| entries.contains(&id))
            },
            |entries| entries.iter().any(|entry| entry == id),
        )
    }
    pub fn tag(&self, registry: &str, id: &str) -> Option<&[String]> {
        self.tags.get(registry)?.get(id).map(Vec::as_slice)
    }
}
