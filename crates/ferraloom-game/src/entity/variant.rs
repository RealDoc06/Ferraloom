//! Immutable spawn-conditioned variant registries, shared by the Level context.
use crate::{
    level::{Level, LevelError},
    loot::{Result, codec},
    predicates::DoubleBounds,
};
use ferraloom_data::datapack::{ResourceManager, load_tags};
use ferraloom_protocol::{Identifier, codec::HolderSet, component::Persistent};
use ferraloom_util::BlockPos;
use ferraloom_worldgen::generator::ChunkGeneratorAccess;
use serde_json::Value;

#[cfg(test)]
mod tests;

/// One load-scoped owner grows by species, without adding more Level services.
#[derive(Debug)]
pub struct MobVariants {
    pub cow: CowVariants,
}
#[derive(Debug)]
pub struct CowVariants {
    pub variants: SpawnVariantRegistry,
    sound_variants: Vec<Identifier>,
}
impl MobVariants {
    pub fn load(resources: &ResourceManager) -> Result<Self> {
        let variants = SpawnVariantRegistry::load(resources, "cow_variant")?;
        for resource in resources.registry("cow_variant").values() {
            let value = resource.json()?;
            codec::id(codec::field(&value, "asset_id")?)?;
            codec::id(codec::field(&value, "baby_asset_id")?)?;
            let model = codec::optional(&value, "model", codec::string)?.unwrap_or("normal");
            if !matches!(model, "normal" | "cold" | "warm") {
                return Err(codec::error(format!("invalid cow model {model}")));
            }
        }
        let mut sound_variants = Vec::new();
        for (key, resource) in resources.registry("cow_sound_variant") {
            let value = resource.json()?;
            for field in ["ambient_sound", "hurt_sound", "death_sound", "step_sound"] {
                let sound = ferraloom_protocol::play::SoundEventHolder::from_json(codec::field(
                    &value, field,
                )?)?;
                if let ferraloom_protocol::codec::Holder::Reference(key) = &sound.0
                    && ferraloom_data::registries::sound_event::Id::from_name(key.as_str())
                        .is_none()
                {
                    return Err(codec::error(format!("unbound sound event {key}")));
                }
            }
            sound_variants.push(key.to_string().parse()?);
        }
        Ok(Self {
            cow: CowVariants {
                variants,
                sound_variants,
            },
        })
    }
}
impl CowVariants {
    pub fn sound_variants(&self) -> &[Identifier] {
        &self.sound_variants
    }
}

#[derive(Debug)]
pub struct SpawnVariantRegistry {
    entries: Vec<SpawnVariant>,
}
#[derive(Debug)]
pub struct SpawnVariant {
    pub key: Identifier,
    pub selectors: Vec<SpawnSelector>,
}
#[derive(Debug)]
pub struct SpawnSelector {
    pub condition: Option<SpawnCondition>,
    pub priority: i32,
}
#[derive(Debug)]
pub enum SpawnCondition {
    Biome(HolderSet),
    Structure(HolderSet),
    MoonBrightness(DoubleBounds),
}
impl SpawnVariantRegistry {
    /// Generic registry loader for PriorityProvider variants. ResourceManager's
    /// merged key order is the same order assigned by the protocol registries.
    pub fn load(resources: &ResourceManager, registry: &str) -> Result<Self> {
        let entries = resources
            .registry(registry)
            .into_iter()
            .map(|(key, resource)| {
                let value = resource.json()?;
                let selectors = codec::list(codec::field(&value, "spawn_conditions")?, |value| {
                    Ok(SpawnSelector {
                        condition: codec::optional(value, "condition", |value| {
                            SpawnCondition::decode(value, resources)
                        })?,
                        priority: codec::int(codec::field(value, "priority")?)?,
                    })
                })?;
                Ok(SpawnVariant {
                    key: key.to_string().parse()?,
                    selectors,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self { entries })
    }
    pub fn entries(&self) -> &[SpawnVariant] {
        &self.entries
    }

    /// Port of SpawnContext#create and PriorityProvider#select. Biome capture
    /// occurs even when no selector needs it, before any selection RNG draw.
    pub fn candidates<'a>(
        &'a self,
        pos: BlockPos,
        level: &Level,
    ) -> std::result::Result<Vec<&'a Identifier>, LevelError> {
        let biome = level.biome_at(pos)?;
        self.select(|condition| condition.test(pos, biome, level))
    }
    /// Stable descending priority retains every matching selector, including
    /// repeated entries. Conditions below the winning priority are not read.
    fn select<E>(
        &self,
        mut test: impl FnMut(&SpawnCondition) -> std::result::Result<bool, E>,
    ) -> std::result::Result<Vec<&Identifier>, E> {
        let mut entries = self
            .entries
            .iter()
            .flat_map(|entry| {
                entry
                    .selectors
                    .iter()
                    .map(move |selector| (entry, selector))
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|(_, selector)| std::cmp::Reverse(selector.priority));
        let mut highest = i32::MIN;
        let mut selected = Vec::new();
        for (entry, selector) in entries {
            if selector.priority >= highest
                && match &selector.condition {
                    Some(condition) => test(condition)?,
                    None => true,
                }
            {
                highest = selector.priority;
                selected.push(&entry.key);
            }
        }
        Ok(selected)
    }
}
impl SpawnCondition {
    fn decode(value: &Value, resources: &ResourceManager) -> Result<Self> {
        let kind = codec::kind(value)?;
        Ok(match kind.as_str() {
            "minecraft:biome" => Self::Biome(decode_set(
                codec::field(value, "biomes")?,
                "worldgen/biome",
                resources,
            )?),
            "minecraft:structure" => Self::Structure(decode_set(
                codec::field(value, "structures")?,
                "worldgen/structure",
                resources,
            )?),
            "minecraft:moon_brightness" => {
                Self::MoonBrightness(DoubleBounds::decode(codec::field(value, "range")?)?)
            }
            _ => return Err(codec::error(format!("unknown spawn condition {kind}"))),
        })
    }
    fn test(
        &self,
        pos: BlockPos,
        biome: &ferraloom_worldgen::Identifier,
        level: &Level,
    ) -> std::result::Result<bool, LevelError> {
        match self {
            Self::Biome(HolderSet::Direct(entries)) => Ok(entries.iter().any(|entry| {
                entry.namespace() == biome.namespace() && entry.path() == biome.path()
            })),
            Self::Biome(HolderSet::Named(tag)) => {
                let tag: ferraloom_worldgen::Identifier =
                    tag.as_str().parse().expect("validated biome tag");
                let values = level
                    .context()
                    .generator
                    .data()
                    .biome_tags
                    .get(&tag)
                    .ok_or(LevelError::MissingEnvironmentContext)?;
                Ok(values.contains(biome))
            }
            Self::Structure(set) => Ok(level
                .structure_manager()
                .get_structure_with_piece_at_in(pos, set)?
                .is_some()),
            Self::MoonBrightness(_) => Err(LevelError::UnsupportedSpawnCondition(
                "minecraft:moon_brightness"
                    .parse()
                    .expect("native condition id"),
            )),
        }
    }
}
/// Registry-aware binding rejects missing names/tags at load, as RegistryCodecs
/// does. Query-time named biome membership uses the actual GenerationData tags.
fn decode_set(value: &Value, registry: &str, resources: &ResourceManager) -> Result<HolderSet> {
    let set = HolderSet::from_json(value)?;
    let entries = resources.registry(registry);
    let valid = match &set {
        HolderSet::Direct(values) => values.iter().all(|key| {
            entries
                .keys()
                .any(|entry| entry.to_string() == key.as_str())
        }),
        HolderSet::Named(key) => {
            let tags = load_tags(resources, registry, |id, _| {
                entries.contains_key(id).then(|| id.clone())
            });
            tags.values
                .keys()
                .any(|tag| tag.to_string() == key.as_str())
        }
    };
    if !valid {
        return Err(codec::error(format!(
            "unbound {registry} holder set {set:?}"
        )));
    }
    Ok(set)
}
