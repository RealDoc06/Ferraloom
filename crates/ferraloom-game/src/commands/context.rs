//! Port of CommandBuildContext's registry/feature view. Reload replaces the owned view.
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default)]
pub struct CommandBuildContext {
    pub registries: BTreeMap<String, Vec<String>>,
    pub tags: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    pub features: crate::gamerules::RuleFeatures,
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

/// Registry references in command argument payloads use the same captured registry view.
impl ferraloom_protocol::codec::RegistryContext for CommandBuildContext {
    fn entry_id(
        &self,
        registry: &ferraloom_protocol::Identifier,
        entry: &ferraloom_protocol::Identifier,
    ) -> Option<i32> {
        self.entries(registry.as_str())
            .iter()
            .position(|key| *key == entry.as_str())
            .map(|id| id as i32)
    }
    fn entry_key(
        &self,
        registry: &ferraloom_protocol::Identifier,
        id: i32,
    ) -> Option<ferraloom_protocol::Identifier> {
        self.entries(registry.as_str())
            .get(usize::try_from(id).ok()?)
            .and_then(|key| ferraloom_protocol::Identifier::parse(key).ok())
    }
    fn has_tag(
        &self,
        registry: &ferraloom_protocol::Identifier,
        tag: &ferraloom_protocol::Identifier,
    ) -> bool {
        self.tag(registry.as_str(), tag.as_str()).is_some()
    }
}
