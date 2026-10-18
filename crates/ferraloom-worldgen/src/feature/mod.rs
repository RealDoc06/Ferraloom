//! 26.3 Feature dispatch. A feature owns its typed configuration. Factory
//! callbacks consume JSON once during registry loading, never during placement.
use crate::{
    Error, Identifier,
    codec::*,
    generator::ChunkGeneratorAccess,
    placement::{modifier::PlacementModifier, predicate::PredicateDecodeData},
    region::WorldGenRegion,
};
use ferraloom_data::datapack::ResourceManager;
use ferraloom_util::random::WorldgenRandom;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    sync::Arc,
};

pub mod end;
pub mod misc;
pub mod nether;
pub mod state_provider;

pub trait Feature: Debug + Send + Sync {
    fn place(&self, context: &mut FeatureContext<'_, '_>) -> Result<bool, Error>;
}
pub struct FeatureContext<'a, 'r> {
    pub region: &'a mut WorldGenRegion<'r>,
    pub random: &'a mut WorldgenRandom,
    pub origin: [i32; 3],
    pub generator: &'a dyn ChunkGeneratorAccess,
    pub registry: &'a FeatureRegistry,
}
pub type FeatureFactory =
    for<'a> fn(&Value, &mut FeatureLoader<'a>) -> Result<Arc<dyn Feature>, Error>;

#[derive(Clone, Debug)]
pub struct PlacedFeature {
    pub id: Option<Identifier>,
    pub feature: Arc<dyn Feature>,
    pub placement: Vec<PlacementModifier>,
}
impl PlacedFeature {
    /// Port of FeaturePlacer's stack, preserving eager modifier RNG consumption
    /// and DFS order. Last-modifier outputs place features immediately in order.
    pub fn place(
        &self,
        context: &mut FeatureContext<'_, '_>,
        biome_check: bool,
    ) -> Result<bool, Error> {
        if self.placement.is_empty() {
            return self.feature.place(context);
        }
        let top = if biome_check { Some(self) } else { None };
        let mut stack = vec![(context.origin, 0)];
        let mut placed = false;
        let origin = context.origin;
        while let Some((p, index)) = stack.pop() {
            let positions = self.placement[index].modify(context, context.registry, top, p)?;
            if index + 1 < self.placement.len() {
                stack.extend(positions.into_iter().rev().map(|p| (p, index + 1)));
            } else {
                for p in positions {
                    context.origin = p;
                    placed |= self.feature.place(context)?;
                }
            }
        }
        context.origin = origin;
        Ok(placed)
    }
}
#[derive(Clone, Debug, Default)]
pub struct FeatureRegistry {
    pub features: BTreeMap<Identifier, Arc<dyn Feature>>,
    pub placed: BTreeMap<Identifier, Arc<PlacedFeature>>,
    pub biome_features: BTreeMap<Identifier, Vec<Vec<Identifier>>>,
}
/// Loader caches named holders, preserving canonical placed feature identity.
/// Unsupported feature types are errors; there is no fallback placement.
pub struct FeatureLoader<'a> {
    pub data: PredicateDecodeData<'a>,
    factories: BTreeMap<Identifier, FeatureFactory>,
    registry: FeatureRegistry,
    loading: BTreeSet<(String, Identifier)>,
    providers: BTreeMap<Identifier, Arc<state_provider::BlockStateProvider>>,
    next_inline: u64,
}
impl<'a> FeatureLoader<'a> {
    pub fn new(resources: &'a ResourceManager) -> Result<Self, Error> {
        let mut loader = Self {
            data: PredicateDecodeData::load(resources)?,
            factories: BTreeMap::new(),
            registry: FeatureRegistry::default(),
            loading: BTreeSet::new(),
            providers: BTreeMap::new(),
            next_inline: 0,
        };
        loader.register("no_op".parse()?, |_, _| Ok(Arc::new(NoOpFeature)))?;
        nether::register(&mut loader)?;
        end::register(&mut loader)?;
        misc::register(&mut loader)?;
        Ok(loader)
    }
    pub fn register(&mut self, kind: Identifier, factory: FeatureFactory) -> Result<(), Error> {
        match self.factories.entry(kind) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(factory);
                Ok(())
            }
            std::collections::btree_map::Entry::Occupied(entry) => {
                Err(Error(format!("duplicate feature decoder {}", entry.key())))
            }
        }
    }
    fn resource(&self, registry: &str, id: &Identifier) -> Result<Value, Error> {
        Ok(self
            .data
            .resources
            .require(&id.with_path(format!("worldgen/{registry}/{}.json", id.path()))?)?
            .json()?)
    }
    pub fn feature(&mut self, v: &Value) -> Result<Arc<dyn Feature>, Error> {
        if v.is_string() {
            let id = id(v)?;
            if let Some(feature) = self.registry.features.get(&id) {
                return Ok(feature.clone());
            }
            let key = ("feature".into(), id.clone());
            if !self.loading.insert(key.clone()) {
                return Err(Error(format!("cyclic feature reference {id}")));
            }
            let result = self
                .resource("feature", &id)
                .and_then(|value| self.feature(&value));
            self.loading.remove(&key);
            let feature = result?;
            self.registry.features.insert(id, feature.clone());
            return Ok(feature);
        }
        let kind = kind(v)?;
        let factory = *self
            .factories
            .get(&kind)
            .ok_or_else(|| Error(format!("unsupported feature type {kind}")))?;
        factory(v, self)
    }
    pub fn placed(&mut self, v: &Value) -> Result<Arc<PlacedFeature>, Error> {
        if v.is_string() {
            let id = id(v)?;
            if let Some(feature) = self.registry.placed.get(&id) {
                return Ok(feature.clone());
            }
            let key = ("placed_feature".into(), id.clone());
            if !self.loading.insert(key.clone()) {
                return Err(Error(format!("cyclic placed feature reference {id}")));
            }
            let result = self
                .resource("placed_feature", &id)
                .and_then(|value| self.decode_placed(&value, Some(id.clone())));
            self.loading.remove(&key);
            let feature = Arc::new(result?);
            self.registry.placed.insert(id, feature.clone());
            return Ok(feature);
        }
        Ok(Arc::new(self.decode_placed(v, None)?))
    }
    fn decode_placed(&mut self, v: &Value, id: Option<Identifier>) -> Result<PlacedFeature, Error> {
        Ok(PlacedFeature {
            id,
            feature: self.feature(field(v, "feature")?)?,
            placement: list(field(v, "placement")?)?
                .iter()
                .map(|v| PlacementModifier::decode(v, &self.data))
                .collect::<Result<_, _>>()?,
        })
    }
    pub fn state_provider(
        &mut self,
        v: &Value,
    ) -> Result<Arc<state_provider::BlockStateProvider>, Error> {
        if let Some(text) = v.as_str() {
            let id: Identifier = text.parse()?;
            // BlockState FULL_CODEC rejects the compact string form, so a
            // provider string always names the provider registry.
            if let Some(provider) = self.providers.get(&id) {
                return Ok(provider.clone());
            }
            let key = ("block_state_provider".into(), id.clone());
            if !self.loading.insert(key.clone()) {
                return Err(Error(format!("cyclic state provider reference {id}")));
            }
            let result = self
                .resource("block_state_provider", &id)
                .and_then(|value| self.state_provider(&value));
            self.loading.remove(&key);
            let provider = result?;
            self.providers.insert(id, provider.clone());
            return Ok(provider);
        }
        Ok(Arc::new(state_provider::BlockStateProvider::decode(
            v, self,
        )?))
    }
    pub fn load_all(mut self) -> Result<FeatureRegistry, Error> {
        for id in self.data.resources.registry("worldgen/feature").into_keys() {
            self.feature(&Value::String(id.to_string()))?;
        }
        for id in self
            .data
            .resources
            .registry("worldgen/placed_feature")
            .into_keys()
        {
            self.placed(&Value::String(id.to_string()))?;
        }
        self.load_biome_features()?;
        Ok(self.registry)
    }
    pub fn load_biome_features(&mut self) -> Result<(), Error> {
        let named = self.data.resources.registry("worldgen/placed_feature");
        let tags = ferraloom_data::datapack::load_tags(
            self.data.resources,
            "worldgen/placed_feature",
            |id, _| named.contains_key(id).then(|| id.clone()),
        );
        if !tags.diagnostics.is_empty() {
            return Err(Error("invalid placed feature tags".into()));
        }
        for (id, resource) in self.data.resources.registry("worldgen/biome") {
            let value = resource.json()?;
            let mut steps = Vec::new();
            for step in list(field(&value, "features")?)? {
                if let Some(tag) = step.as_str().and_then(|s| s.strip_prefix('#')) {
                    steps.push(
                        tags.values
                            .get(&tag.parse()?)
                            .cloned()
                            .ok_or_else(|| Error(format!("missing placed feature tag {tag}")))?,
                    );
                    continue;
                }
                let mut entries = Vec::new();
                // LIST_OF_LISTS_CODEC uses alwaysUseList: a bare feature holder
                // is rejected here, although a named tag is accepted above.
                for entry in list(step)? {
                    let id = if entry.is_string() {
                        let id = crate::codec::id(entry)?;
                        if !named.contains_key(&id) {
                            return Err(Error(format!("missing placed feature {id}")));
                        }
                        id
                    } else {
                        let id = loop {
                            let candidate: Identifier =
                                format!("ferraloom:__inline/{}", self.next_inline).parse()?;
                            self.next_inline += 1;
                            if !named.contains_key(&candidate)
                                && !self.registry.placed.contains_key(&candidate)
                            {
                                break candidate;
                            }
                        };
                        let feature = Arc::new(self.decode_placed(entry, Some(id.clone()))?);
                        self.registry.placed.insert(id.clone(), feature);
                        id
                    };
                    entries.push(id);
                }
                steps.push(entries);
            }
            self.registry.biome_features.insert(id, steps);
        }
        Ok(())
    }
    pub fn finish(self) -> FeatureRegistry {
        self.registry
    }
}
#[derive(Debug)]
struct NoOpFeature;
impl Feature for NoOpFeature {
    fn place(&self, _: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        Ok(true)
    }
}
