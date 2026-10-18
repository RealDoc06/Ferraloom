//! Immutable biome bone-meal lists, resolved once with the feature registry.
use super::{Feature, FeatureLoader, FeatureRegistry, PlacedFeature};
use crate::{Error, Identifier};
use ferraloom_data::datapack::load_tags;
use serde_json::Value;
use std::sync::Arc;

impl PlacedFeature {
    /// Port of PlacedFeature#getFeatures, including composite branches in order.
    pub fn visit_features(&self, visit: &mut dyn FnMut(&Arc<dyn Feature>)) {
        visit(&self.feature);
        for child in self.feature.sub_features() {
            child.visit_features(visit);
        }
    }
}
impl FeatureRegistry {
    /// Port of BiomeGenerationSettings#getBoneMealFeatures. Duplicates remain
    /// weighted by occurrence. load_all populates this cache; partial loaders
    /// must call load_biome_features and load_bone_meal_features explicitly.
    pub fn bone_meal_features(&self, biome: &Identifier) -> Result<&[Arc<dyn Feature>], Error> {
        self.biome_bone_meal_features
            .get(biome)
            .map(Vec::as_slice)
            .ok_or_else(|| {
                Error(format!(
                    "bone-meal features are not loaded for biome {biome}"
                ))
            })
    }
}
impl FeatureLoader<'_> {
    /// Resolve holder tag membership and composite traversal at load, never on
    /// live bone-meal reads. Named holder references share the loader's Arc.
    pub fn load_bone_meal_features(&mut self) -> Result<(), Error> {
        let named = self.data.resources.registry("worldgen/feature");
        let tags = load_tags(self.data.resources, "worldgen/feature", |id, _| {
            named.contains_key(id).then(|| id.clone())
        });
        if !tags.diagnostics.is_empty() {
            return Err(Error("invalid feature tags".into()));
        }
        let mut tagged = Vec::new();
        if let Some(ids) = tags
            .values
            .get(&"minecraft:can_spawn_from_bone_meal".parse()?)
        {
            for id in ids {
                tagged.push(self.feature(&Value::String(id.to_string()))?);
            }
        }
        let biomes = self.registry.biome_features.clone();
        let mut lists = std::collections::BTreeMap::new();
        for (biome, steps) in biomes {
            let mut features = Vec::new();
            for id in steps.into_iter().flatten() {
                self.placed(&Value::String(id.to_string()))?
                    .visit_features(&mut |feature| {
                        if tagged.iter().any(|entry| Arc::ptr_eq(entry, feature)) {
                            features.push(feature.clone());
                        }
                    });
            }
            lists.insert(biome, features);
        }
        self.registry.biome_bone_meal_features = lists;
        Ok(())
    }
}
