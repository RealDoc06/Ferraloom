//! Port of RegistryDataLoader, RegistrySynchronization and TagNetworkSerialization.
use anyhow::{Result, ensure};
use ferraloom_data::datapack::{ResourceManager, load_tags};
use ferraloom_protocol::{
    Identifier,
    codec::VarInt,
    common::clientbound::{RegistryTags, TagEntry, UpdateTags},
    configuration as cfg,
};
use std::collections::BTreeMap;

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
    pub entries: BTreeMap<String, Vec<String>>,
    pub tags: UpdateTags,
}
impl RegistrySet {
    pub fn load(resources: &ResourceManager) -> Result<Self> {
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
            let loaded = load_tags(resources, directory, |id, _| {
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
