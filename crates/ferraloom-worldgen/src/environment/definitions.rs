//! Immutable datapack inputs shared by concrete live and generation samplers.
use super::{AttributeMap, ScalarValue, Timeline, UnsupportedAttribute};
use crate::{Error, Identifier};
use ferraloom_data::{
    datapack::{ResourceManager, load_tags},
    registries::environment_attribute::{self, Id},
};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug)]
pub struct DimensionEnvironment {
    pub attributes: Arc<AttributeMap>,
    pub timelines: Vec<Arc<Timeline>>,
}

#[derive(Debug)]
pub struct EnvironmentDefinitions {
    pub(super) defaults: Vec<Result<ScalarValue, UnsupportedAttribute>>,
    pub(super) biome_attributes: Vec<bool>,
    biomes: Vec<Arc<AttributeMap>>,
    biome_ids: BTreeMap<Identifier, usize>,
    timelines: BTreeMap<Identifier, Arc<Timeline>>,
    timeline_tags: BTreeMap<Identifier, Vec<Identifier>>,
}
impl EnvironmentDefinitions {
    pub fn load(resources: &ResourceManager) -> Result<Self, Error> {
        let defaults = environment_attribute::ALL
            .iter()
            .copied()
            .map(ScalarValue::native_default)
            .collect();
        let empty = Arc::new(AttributeMap::default());
        let mut biomes = Vec::new();
        let mut biome_ids = BTreeMap::new();
        let mut biome_attributes = vec![false; environment_attribute::ALL.len()];
        for (name, resource) in resources.registry("worldgen/biome") {
            let value = resource.json()?;
            // Missing optional fields share vanilla's EnvironmentAttributeMap.EMPTY.
            // Explicit maps retain their own identity for spatial accumulation.
            let attributes = match value.get("attributes").filter(|value| !value.is_null()) {
                Some(value) => Arc::new(AttributeMap::decode(value, true)?),
                None => empty.clone(),
            };
            for attribute in attributes.attributes() {
                biome_attributes[attribute.protocol_id() as usize] = true;
            }
            biome_ids.insert(name, biomes.len());
            biomes.push(attributes);
        }
        let timelines = resources
            .registry("timeline")
            .into_iter()
            .map(|(name, resource)| Ok((name, Arc::new(Timeline::decode(&resource.json()?)?))))
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        let tags = load_tags(resources, "timeline", |id, _| {
            timelines.contains_key(id).then(|| id.clone())
        });
        if !tags.diagnostics.is_empty() {
            return Err(Error(format!(
                "invalid timeline tags: {:?}",
                tags.diagnostics
            )));
        }
        Ok(Self {
            defaults,
            biome_attributes,
            biomes,
            biome_ids,
            timelines,
            timeline_tags: tags.values,
        })
    }
    pub fn biome_index(&self, id: &Identifier) -> Option<usize> {
        self.biome_ids.get(id).copied()
    }
    pub fn biome(&self, index: usize) -> Option<&Arc<AttributeMap>> {
        self.biomes.get(index)
    }
    pub fn timeline(&self, id: &Identifier) -> Option<&Arc<Timeline>> {
        self.timelines.get(id)
    }
    /// Port of DimensionType's attribute map and timeline holder-set fields.
    /// Holder tags preserve their datapack order; direct lists retain duplicates.
    pub fn dimension(&self, value: &Value) -> Result<DimensionEnvironment, Error> {
        let attributes = value
            .get("attributes")
            .filter(|value| !value.is_null())
            .map(|value| AttributeMap::decode(value, false))
            .transpose()?
            .unwrap_or_default();
        let mut timelines = Vec::new();
        let mut append = |value: &str| -> Result<(), Error> {
            let name: Identifier = value.parse()?;
            timelines.push(
                self.timelines
                    .get(&name)
                    .cloned()
                    .ok_or_else(|| Error(format!("unknown timeline {name}")))?,
            );
            Ok(())
        };
        if let Some(value) = value.get("timelines").filter(|value| !value.is_null()) {
            if let Some(name) = value.as_str() {
                if let Some(tag) = name.strip_prefix('#') {
                    let tag: Identifier = tag.parse()?;
                    let values = self
                        .timeline_tags
                        .get(&tag)
                        .ok_or_else(|| Error(format!("unknown timeline tag {tag}")))?;
                    for name in values {
                        append(&name.to_string())?;
                    }
                } else {
                    append(name)?;
                }
            } else {
                for name in crate::codec::list(value)? {
                    append(crate::codec::string(name)?)?;
                }
            }
        }
        Ok(DimensionEnvironment {
            attributes: Arc::new(attributes),
            timelines,
        })
    }
    pub(super) fn default_value(&self, id: Id) -> Result<ScalarValue, UnsupportedAttribute> {
        self.defaults[id.protocol_id() as usize]
    }
}
