//! Port of world.damagesource: registry-backed types, holders and sources.
use ferraloom_data::datapack::{self, Identifier, ResourceManager, TagElement, TagSet, load_tags};
use std::{collections::BTreeMap, sync::Arc};

pub mod combat_rules;
mod source;
mod sources;
mod types;
pub use source::DamageSource;
pub use sources::DamageSources;
pub use types::{
    DamageEffects, DamageScaling, DamageType, DamageTypeKey, DamageTypeTag, DeathMessageType,
};

/// Dense local registry identity, used only with this registry's resolved tags.
/// The server registry codec owns the independent network ID assignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DamageTypeId(usize);
impl TagElement for DamageTypeId {
    fn tag_index(self) -> usize {
        self.0
    }
}

#[derive(Debug)]
struct DamageTypeEntry {
    id: DamageTypeId,
    name: Identifier,
    value: DamageType,
    native_tags: u64,
    native_key: Option<DamageTypeKey>,
}

/// Port of Holder.Reference<DamageType>. Held sources retain this immutable
/// registry entry rather than copying flags or re-resolving a name on a hot path.
#[derive(Clone, Debug)]
pub struct DamageTypeHolder(Arc<DamageTypeEntry>);
impl DamageTypeHolder {
    pub fn id(&self) -> DamageTypeId {
        self.0.id
    }
    pub fn name(&self) -> &Identifier {
        &self.0.name
    }
    pub fn value(&self) -> &DamageType {
        &self.0.value
    }
    pub fn is_tag(&self, tag: DamageTypeTag) -> bool {
        self.0.native_tags & (1 << tag as usize) != 0
    }
    pub fn is_type(&self, key: DamageTypeKey) -> bool {
        self.0.native_key == Some(key)
    }
}

/// Port of the damage_type registry loaded by RegistryDataLoader, including
/// TagLoader membership. Required built-in keys are resolved once at load time.
#[derive(Debug)]
pub struct DamageTypeRegistry {
    by_name: BTreeMap<Identifier, DamageTypeHolder>,
    tags: TagSet<DamageTypeId>,
    sources: DamageSources,
}
impl DamageTypeRegistry {
    pub fn load(resources: &ResourceManager) -> Result<Self, datapack::Error> {
        let mut entries = Vec::new();
        let mut ids = BTreeMap::new();
        for (name, resource) in resources.registry("damage_type") {
            let value = DamageType::decode(&resource.json()?).map_err(|error| {
                datapack::Error::InvalidData(format!(
                    "damage type {name} from {}: {error}",
                    resource.source
                ))
            })?;
            let id = DamageTypeId(entries.len());
            ids.insert(name.clone(), id);
            entries.push((id, name, value));
        }
        let tags = TagSet::from_tags(
            load_tags(resources, "damage_type", |name, _| ids.get(name).copied()),
            entries.len(),
        )?;
        let native_tags: Vec<_> = DamageTypeTag::ALL
            .iter()
            .map(|tag| {
                tags.resolve_name(&tag.name().parse().expect("native damage tag identifier"))
            })
            .collect();
        let by_name: BTreeMap<_, _> = entries
            .into_iter()
            .map(|(id, name, value)| {
                let membership = native_tags
                    .iter()
                    .enumerate()
                    .fold(0, |bits, (index, tag)| {
                        bits | if tag.is_some_and(|tag| tags.contains(tag, id)) {
                            1 << index
                        } else {
                            0
                        }
                    });
                let native_key = DamageTypeKey::ALL.iter().copied().find(|key| {
                    name.namespace() == "minecraft"
                        && name.path() == key.name().strip_prefix("minecraft:").unwrap()
                });
                let holder = DamageTypeHolder(Arc::new(DamageTypeEntry {
                    id,
                    name: name.clone(),
                    value,
                    native_tags: membership,
                    native_key,
                }));
                (name, holder)
            })
            .collect();
        let builtin = DamageTypeKey::ALL
            .iter()
            .map(|key| {
                let name = key.name().parse().expect("native damage type identifier");
                by_name
                    .get(&name)
                    .cloned()
                    .ok_or(datapack::Error::MissingResource(name))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            by_name,
            tags,
            sources: DamageSources::new(builtin.into_boxed_slice()),
        })
    }
    pub fn get(&self, name: &Identifier) -> Option<&DamageTypeHolder> {
        self.by_name.get(name)
    }
    pub fn holder(&self, key: DamageTypeKey) -> &DamageTypeHolder {
        self.sources.holder(key)
    }
    pub fn tags(&self) -> &TagSet<DamageTypeId> {
        &self.tags
    }
    pub fn sources(&self) -> &DamageSources {
        &self.sources
    }
    pub fn len(&self) -> usize {
        self.by_name.len()
    }
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

impl crate::level::Level {
    pub fn damage_types(&self) -> &DamageTypeRegistry {
        self.context().damage_types()
    }
    /// Port of Level#damageSources, using the immutable registry from this load.
    pub fn damage_sources(&self) -> &DamageSources {
        self.context().damage_sources()
    }
}

#[cfg(test)]
mod tests;
