//! Port of RegistryDataLoader, RegistrySynchronization and TagNetworkSerialization.
use anyhow::{Result, ensure};
use ferraloom_data::datapack::{ResourceManager, load_tags};
use ferraloom_game::loot::{self, RegistryHolder, RegistryKey, RegistryLookup, RegistryReference};
use ferraloom_protocol::{
    Identifier,
    codec::VarInt,
    common::clientbound::{RegistryTags, TagEntry, UpdateTags},
    configuration as cfg,
};
use std::{collections::BTreeMap, sync::Arc};

/// RegistryDataLoader.SYNCHRONIZED_REGISTRIES order is wire-visible.
pub const SYNCHRONIZED: &[&str] = &[
    "worldgen/biome",
    "chat_type",
    "trim_pattern",
    "trim_material",
    "wolf_variant",
    "wolf_sound_variant",
    "pig_variant",
    "pig_sound_variant",
    "frog_variant",
    "cat_variant",
    "cat_sound_variant",
    "cow_sound_variant",
    "cow_variant",
    "chicken_sound_variant",
    "chicken_variant",
    "zombie_nautilus_variant",
    "painting_variant",
    "sulfur_cube_archetype",
    "dimension_type",
    "damage_type",
    "banner_pattern",
    "enchantment",
    "jukebox_song",
    "instrument",
    "test_environment",
    "test_instance",
    "dialog",
    "world_clock",
    "timeline",
    "decorated_pot_pattern",
    "block_transformer",
    "worldgen/block_state_provider",
];
pub fn id(name: &str) -> Identifier {
    Identifier::parse(name).expect("validated built-in identifier")
}
pub fn core_pack() -> cfg::KnownPack {
    cfg::KnownPack {
        namespace: "minecraft".into(),
        id: "core".into(),
        version: "26.3".into(),
    }
}
#[derive(Clone)]
pub struct RegistrySet {
    pub resources: Arc<ResourceManager>,
    pub entries: BTreeMap<String, Vec<String>>,
    pub tags: UpdateTags,
}
impl RegistrySet {
    pub fn load(resources: Arc<ResourceManager>) -> Result<Self> {
        let mut entries = BTreeMap::new();
        for &registry in SYNCHRONIZED {
            let names = resources
                .registry(registry)
                .into_keys()
                .map(|id| id.to_string())
                .collect::<Vec<_>>();
            ensure!(!names.is_empty(), "empty synchronized registry {registry}");
            entries.insert(format!("minecraft:{registry}"), names);
        }
        let mut candidates = entries.clone();
        for registry in ferraloom_data::REGISTRIES {
            if let Some(names) = ferraloom_data::registry_entries(registry.name) {
                candidates.insert(
                    registry.name.into(),
                    names.iter().map(|s| s.to_string()).collect(),
                );
            }
        }
        let mut tags = Vec::new();
        for (registry, names) in &candidates {
            let directory = registry.strip_prefix("minecraft:").unwrap_or(registry);
            if resources
                .list(&format!("tags/{directory}/"), ".json")
                .is_empty()
            {
                continue;
            }
            let lookup: BTreeMap<_, _> = names
                .iter()
                .enumerate()
                .map(|(i, n)| (n.as_str(), i as i32))
                .collect();
            let loaded = load_tags(&resources, directory, |id, _| {
                lookup.get(id.to_string().as_str()).copied()
            });
            ensure!(
                loaded.diagnostics.is_empty(),
                "invalid {registry} tags: {:?}",
                loaded.diagnostics
            );
            tags.push(RegistryTags {
                registry: id(registry),
                tags: loaded
                    .values
                    .into_iter()
                    .map(|(name, ids)| TagEntry {
                        name: id(&name.to_string()),
                        ids: ids.into_iter().map(VarInt).collect(),
                    })
                    .collect(),
            });
        }
        Ok(Self {
            resources,
            entries,
            tags: UpdateTags { tags },
        })
    }
    pub fn numeric(&self, registry: &str, name: &str) -> i32 {
        self.entries[&id(registry).to_string()]
            .iter()
            .position(|n| n == id(name).as_str())
            .expect("required registry entry") as i32
    }
    pub fn synchronize(&self, packs: &[cfg::KnownPack]) -> Result<Vec<cfg::ClientboundPacket>> {
        ensure!(
            packs == [core_pack()],
            "This generation slice requires the vanilla 26.3 core pack; direct registry codecs are not yet ported"
        );
        let mut packets = Vec::new();
        for &registry in SYNCHRONIZED {
            let entries = self.entries[&format!("minecraft:{registry}")]
                .iter()
                .map(|name| cfg::PackedRegistryEntry {
                    id: id(name),
                    data: None,
                })
                .collect();
            packets.push(cfg::ClientboundPacket::RegistryData(cfg::RegistryData {
                registry: id(registry),
                entries,
            }));
        }
        packets.push(cfg::ClientboundPacket::UpdateTags(self.tags.clone()));
        Ok(packets)
    }
}
impl ferraloom_protocol::codec::RegistryContext for RegistrySet {
    fn entry_value(&self, registry: &Identifier, key: &Identifier) -> Option<serde_json::Value> {
        self.entry_id(registry, key)?;
        self.resources
            .get(
                &ferraloom_data::datapack::Identifier::new(
                    key.namespace(),
                    format!("{}/{}.json", registry.path(), key.path()),
                )
                .ok()?,
            )?
            .json()
            .ok()
    }
    fn entry_id(&self, registry: &Identifier, entry: &Identifier) -> Option<i32> {
        if let Some(names) = self.entries.get(registry.as_str()) {
            names
                .iter()
                .position(|name| name == entry.as_str())
                .map(|id| id as i32)
        } else {
            ferraloom_data::registry_entries(registry.as_str())?
                .iter()
                .position(|name| *name == entry.as_str())
                .map(|id| id as i32)
        }
    }
    fn entry_key(&self, registry: &Identifier, number: i32) -> Option<Identifier> {
        let index = usize::try_from(number).ok()?;
        if let Some(names) = self.entries.get(registry.as_str()) {
            Identifier::parse(names.get(index)?).ok()
        } else {
            Identifier::parse(ferraloom_data::registry_entries(registry.as_str())?.get(index)?).ok()
        }
    }
    fn has_tag(&self, registry: &Identifier, tag: &Identifier) -> bool {
        self.tags
            .tags
            .iter()
            .find(|tags| &tags.registry == registry)
            .is_some_and(|tags| tags.tags.iter().any(|entry| &entry.name == tag))
    }
}

/// Cold RegistryOps view over the published registry and datapack snapshot.
/// Loot retains only qualified identities; registry values stay with their owner.
pub(crate) struct LootRegistryLookup<'a> {
    registries: &'a RegistrySet,
    resources: &'a ResourceManager,
}
impl<'a> LootRegistryLookup<'a> {
    pub(crate) fn new(registries: &'a RegistrySet, resources: &'a ResourceManager) -> Self {
        Self {
            registries,
            resources,
        }
    }
    fn value_exists(&self, registry: RegistryKey, value: &Identifier) -> bool {
        let key = format!("minecraft:{}", registry.name());
        if let Some(entries) = self.registries.entries.get(&key) {
            return entries.iter().any(|entry| entry == value.as_str());
        }
        // Structures are datapack registries outside the synchronized wire set.
        if registry == RegistryKey::Structure {
            let Ok(resource) = ferraloom_data::datapack::Identifier::new(
                value.namespace(),
                format!("{}/{}.json", registry.name(), value.path()),
            ) else {
                return false;
            };
            return self.resources.get(&resource).is_some();
        }
        ferraloom_data::registry_entries(&key)
            .is_some_and(|entries| entries.contains(&value.as_str()))
    }
}
impl RegistryLookup for LootRegistryLookup<'_> {
    fn lookup(
        &self,
        registry: RegistryKey,
        reference: &RegistryReference,
    ) -> loot::Result<Option<RegistryHolder>> {
        let exists = match reference {
            RegistryReference::Value(value) => self.value_exists(registry, value),
            RegistryReference::Tag(tag) => {
                let loaded = load_tags(self.resources, registry.name(), |entry, _| {
                    let entry = Identifier::parse(&entry.to_string()).ok()?;
                    self.value_exists(registry, &entry).then_some(entry)
                });
                if !loaded.diagnostics.is_empty() {
                    return Err(loot::LootError::Decode(format!(
                        "invalid {} tags: {:?}",
                        registry.name(),
                        loaded.diagnostics
                    )));
                }
                loaded
                    .values
                    .keys()
                    .any(|entry| entry.namespace() == tag.namespace() && entry.path() == tag.path())
            }
        };
        Ok(exists.then(|| RegistryHolder {
            registry,
            reference: reference.clone(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_data::datapack::Pack;

    fn resources(files: &[(&str, &str)]) -> ResourceManager {
        let mut pack = Pack::new("lookup-fixture");
        for (name, contents) in files {
            pack.insert(name.parse().unwrap(), contents.as_bytes());
        }
        ResourceManager::new(vec![pack])
    }
    fn registries() -> RegistrySet {
        RegistrySet {
            resources: Arc::new(ResourceManager::default()),
            entries: [(
                "minecraft:enchantment".into(),
                vec!["example:published".into()],
            )]
            .into(),
            tags: UpdateTags { tags: Vec::new() },
        }
    }

    #[test]
    fn cold_registry_values_require_published_keys_and_share_the_resource_owner() {
        use ferraloom_protocol::codec::RegistryContext;
        let resources = Arc::new(resources(&[
            (
                "example:jukebox_song/published.json",
                r#"{"length_in_seconds":2.5}"#,
            ),
            (
                "example:jukebox_song/unpublished.json",
                r#"{"length_in_seconds":99}"#,
            ),
        ]));
        let mut registry = registries();
        registry.resources = resources.clone();
        registry.entries.insert(
            "minecraft:jukebox_song".into(),
            vec!["example:published".into()],
        );
        assert!(Arc::ptr_eq(&registry.resources, &resources));
        let kind = id("minecraft:jukebox_song");
        assert_eq!(
            registry.entry_value(&kind, &id("example:published")),
            Some(serde_json::json!({"length_in_seconds":2.5}))
        );
        assert_eq!(
            registry.entry_value(&kind, &id("example:unpublished")),
            None
        );
        assert_eq!(registry.entry_value(&kind, &id("example:missing")), None);
    }

    #[test]
    fn loot_values_use_published_registries_and_actual_structure_resources() {
        let resources = resources(&[
            ("example:worldgen/structure/room.json", "{}"),
            ("example:enchantment/unpublished.json", "{}"),
        ]);
        let registries = registries();
        let lookup = LootRegistryLookup::new(&registries, &resources);
        for (registry, name, exists) in [
            (RegistryKey::Item, "minecraft:stone", true),
            (RegistryKey::Item, "example:stone", false),
            (RegistryKey::Enchantment, "example:published", true),
            (RegistryKey::Enchantment, "example:unpublished", false),
            (RegistryKey::Structure, "example:room", true),
            (RegistryKey::Structure, "minecraft:room", false),
            (RegistryKey::Structure, "minecraft:village_plains", false),
        ] {
            let reference = RegistryReference::Value(id(name));
            assert_eq!(
                lookup.lookup(registry, &reference).unwrap(),
                exists.then_some(RegistryHolder {
                    registry,
                    reference,
                })
            );
        }
    }

    #[test]
    fn loot_tags_resolve_nested_optional_and_empty_sets() {
        let resources = resources(&[
            ("example:worldgen/structure/room.json", "{}"),
            (
                "example:tags/worldgen/structure/rooms.json",
                r#"{"values":["example:room"]}"#,
            ),
            (
                "example:tags/item/base.json",
                r#"{"values":["minecraft:stone"]}"#,
            ),
            (
                "example:tags/item/nested.json",
                r##"{"values":["#example:base",{"id":"example:missing","required":false}]}"##,
            ),
            ("example:tags/item/empty.json", r#"{"values":[]}"#),
        ]);
        let registries = registries();
        let lookup = LootRegistryLookup::new(&registries, &resources);
        for (registry, name, exists) in [
            (RegistryKey::Item, "example:nested", true),
            (RegistryKey::Item, "example:empty", true),
            (RegistryKey::Item, "example:missing", false),
            (RegistryKey::Structure, "example:rooms", true),
            (RegistryKey::Structure, "minecraft:rooms", false),
        ] {
            let reference = RegistryReference::Tag(id(name));
            assert_eq!(
                lookup.lookup(registry, &reference).unwrap(),
                exists.then_some(RegistryHolder {
                    registry,
                    reference,
                })
            );
        }
    }

    #[test]
    fn loot_tags_reject_unresolved_required_entries() {
        let resources = resources(&[(
            "example:tags/item/invalid.json",
            r#"{"values":["example:missing"]}"#,
        )]);
        let registries = registries();
        assert!(matches!(
            LootRegistryLookup::new(&registries, &resources).lookup(
                RegistryKey::Item,
                &RegistryReference::Tag(id("example:invalid"))
            ),
            Err(loot::LootError::Decode(message)) if message.contains("invalid item tags")
        ));
    }
}
