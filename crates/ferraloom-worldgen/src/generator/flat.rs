//! Ports of FlatLevelSource, FlatLayerInfo and FlatLevelGeneratorSettings.
use super::*;
use crate::{
    codec::*,
    feature::{FeatureRegistry, PlacedFeature},
    region::Heightmap,
};
use ferraloom_data::{Block, datapack::ResourceManager};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlatLayerInfo {
    pub height: i32,
    pub block: Block,
}
impl FlatLayerInfo {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        let name = id(field(v, "block")?)?;
        Ok(Self {
            height: int_range(field(v, "height")?, 0, 4064)?,
            block: Block::from_name(&name.to_string())
                .ok_or_else(|| Error(format!("unknown block {name}")))?,
        })
    }
}
#[derive(Clone, Debug)]
pub struct FlatLevelGeneratorSettings {
    pub structure_overrides: Option<Vec<Identifier>>,
    pub layers: Vec<FlatLayerInfo>,
    pub lakes: bool,
    pub features: bool,
    pub biome: Identifier,
}
impl FlatLevelGeneratorSettings {
    /// The height validator supplies a partial result. DedicatedServerProperties
    /// accepts that partial; ordinary codec consumers require a success.
    pub fn decode(v: &Value, resources: &ResourceManager) -> Result<Self, Error> {
        let settings = Self::decode_partial(v, resources)?;
        if settings
            .layers
            .iter()
            .map(|layer| i64::from(layer.height))
            .sum::<i64>()
            > 4064
        {
            return Err(Error("sum of flat layer heights exceeds 4064".into()));
        }
        Ok(settings)
    }
    pub fn decode_partial(v: &Value, resources: &ResourceManager) -> Result<Self, Error> {
        if v.get("biome").is_some_and(Value::is_object) {
            return Err(Error(
                "Unsupported: inline biome holders require section and protocol biome identity"
                    .into(),
            ));
        }
        let known_biomes = resources.registry("worldgen/biome");
        let biome = v
            .get("biome")
            .and_then(|v| id(v).ok())
            .filter(|name| known_biomes.contains_key(name))
            .unwrap_or_else(|| "plains".parse().unwrap());
        let structure_overrides = v.get("structure_overrides").and_then(|v| {
            let entries = resources.registry("worldgen/structure_set");
            let resolve = |v: &Value| -> Result<Identifier, Error> {
                let name = id(v)?;
                entries
                    .contains_key(&name)
                    .then_some(name)
                    .ok_or_else(|| Error("unknown flat structure set".into()))
            };
            if let Some(tag) = v.as_str().and_then(|text| text.strip_prefix('#')) {
                load_tags(resources, "worldgen/structure_set", |name, _| {
                    entries.contains_key(name).then(|| name.clone())
                })
                .values
                .get(&tag.parse().ok()?)
                .cloned()
            } else if v.is_string() {
                resolve(v).map(|name| vec![name]).ok()
            } else {
                list(v)
                    .and_then(|values| values.iter().map(resolve).collect())
                    .ok()
            }
        });
        Ok(Self {
            structure_overrides,
            layers: list(field(v, "layers")?)?
                .iter()
                .map(FlatLayerInfo::decode)
                .collect::<Result<_, _>>()?,
            lakes: optional(v, "lakes", false, boolean)?,
            features: optional(v, "features", false, boolean)?,
            biome,
        })
    }
    pub fn default_settings() -> Self {
        Self {
            structure_overrides: Some(
                ["strongholds", "villages"]
                    .map(|name| name.parse().unwrap())
                    .to_vec(),
            ),
            layers: [
                (1, Block::BEDROCK),
                (2, Block::DIRT),
                (1, Block::GRASS_BLOCK),
            ]
            .map(|(height, block)| FlatLayerInfo { height, block })
            .to_vec(),
            biome: "plains".parse().unwrap(),
            features: false,
            lakes: false,
        }
    }
    pub fn preset(name: &Identifier, resources: &ResourceManager) -> Result<Self, Error> {
        let v = resources
            .require(&name.with_path(format!(
                "worldgen/flat_level_generator_preset/{}.json",
                name.path()
            ))?)?
            .json()?;
        Self::decode(field(&v, "settings")?, resources)
    }
    fn states(&self) -> Vec<BlockState> {
        self.layers
            .iter()
            .flat_map(|layer| {
                std::iter::repeat_n(layer.block.default_state(), layer.height as usize)
            })
            .collect()
    }
}

#[derive(Debug)]
pub struct FlatLevelSource {
    pub data: Arc<GenerationData>,
    pub settings: FlatLevelGeneratorSettings,
    source: BiomeSource,
    layers: Vec<Option<BlockState>>,
    deferred: Vec<(i32, BlockState)>,
    void_gen: bool,
}
impl FlatLevelSource {
    pub fn layers(&self) -> &[Option<BlockState>] {
        &self.layers
    }
    pub fn feature_registry(&self, resources: &ResourceManager) -> Result<FeatureRegistry, Error> {
        self.load_features(resources).map(|(registry, _)| registry)
    }
    /// The current server can run deferred layers and an unmodified void
    /// platform locally. Other records require the owned-neighborhood scheduler.
    pub fn chunk_local_feature_registry(
        &self,
        resources: &ResourceManager,
    ) -> Result<Option<FeatureRegistry>, Error> {
        let (registry, local) = self.load_features(resources)?;
        Ok(local.then_some(registry))
    }
    fn load_features(&self, resources: &ResourceManager) -> Result<(FeatureRegistry, bool), Error> {
        let mut loader = crate::feature::FeatureLoader::new(resources)?;
        loader.load_biome_features()?;
        let mut named = Vec::new();
        if (!self.void_gen || self.settings.biome == "the_void".parse()?) && self.settings.features
        {
            for (step, entries) in loader.registry().biome_features[&self.settings.biome]
                .iter()
                .enumerate()
            {
                if step != 3 && step != 4 && (!self.settings.lakes || step != 1) {
                    named.extend(entries.iter().cloned());
                }
            }
        }
        if self.settings.lakes {
            named.extend([
                "lake_lava_underground".parse()?,
                "lake_lava_surface".parse()?,
            ]);
        }
        let mut local = true;
        for name in named {
            loader.placed(&Value::String(name.to_string()))?;
            let path = name.with_path(format!("worldgen/placed_feature/{}.json", name.path()))?;
            let Ok(resource) = resources.require(&path) else {
                // Inline holders have no registry resource to inspect. Keep
                // them on the general owned-region path conservatively.
                local = false;
                continue;
            };
            let placed = resource.json()?;
            let encoded = field(&placed, "feature")?;
            let feature = if encoded.is_string() {
                let id = id(encoded)?;
                resources
                    .require(&id.with_path(format!("worldgen/feature/{}.json", id.path()))?)?
                    .json()?
            } else {
                encoded.clone()
            };
            local &= list(field(&placed, "placement")?)?.is_empty()
                && matches!(
                    kind(&feature)?.to_string().as_str(),
                    "minecraft:void_start_platform" | "minecraft:fill_layer" | "minecraft:no_op"
                );
        }
        Ok((self.adjust_feature_registry(&loader.finish())?, local))
    }
    pub fn new(
        data: Arc<GenerationData>,
        settings: FlatLevelGeneratorSettings,
    ) -> Result<Self, Error> {
        if !data.biome_ids.contains_key(&settings.biome) {
            return Err(Error(format!("unknown flat biome {}", settings.biome)));
        }
        let states = settings.states();
        let void_gen = states.iter().all(|state| state.block() == Block::AIR);
        let mut deferred = Vec::new();
        let layers = states
            .into_iter()
            .enumerate()
            .map(|(i, state)| {
                if data.heightmap_matches(Heightmap::MotionBlocking, state) {
                    Some(state)
                } else {
                    deferred.push((i as i32, state));
                    None
                }
            })
            .collect();
        Ok(Self {
            source: BiomeSource::Fixed(settings.biome.clone()),
            data,
            settings,
            layers,
            deferred,
            void_gen,
        })
    }
    /// Port of adjustGenerationSettings, including excluded structure steps and
    /// deferred non-motion-blocking layers at TOP_LAYER_MODIFICATION.
    pub fn adjust_feature_registry(
        &self,
        registry: &FeatureRegistry,
    ) -> Result<FeatureRegistry, Error> {
        let mut adjusted = registry.clone();
        let mut steps = Vec::<Vec<Identifier>>::new();
        if self.settings.lakes {
            steps.resize(2, Vec::new());
            for name in ["lake_lava_underground", "lake_lava_surface"] {
                let id = name.parse()?;
                if !registry.placed.contains_key(&id) {
                    return Err(Error(format!("missing flat lake {id}")));
                }
                steps[1].push(id);
            }
        }
        if (!self.void_gen || self.settings.biome == "the_void".parse()?) && self.settings.features
        {
            let original = registry
                .biome_features
                .get(&self.settings.biome)
                .ok_or_else(|| Error("missing flat biome feature settings".into()))?;
            for (step, features) in original.iter().enumerate() {
                if step == 3 || step == 4 || (self.settings.lakes && step == 1) {
                    continue;
                }
                if !features.is_empty() {
                    steps.resize_with(steps.len().max(step + 1), Vec::new);
                    steps[step].extend(features.iter().cloned());
                }
            }
        }
        for &(height, state) in &self.deferred {
            let mut salt = 0;
            let id = loop {
                let id: Identifier = format!("ferraloom:flat_layer/{height}/{salt}").parse()?;
                if !adjusted.placed.contains_key(&id) && !adjusted.features.contains_key(&id) {
                    break id;
                }
                salt += 1;
            };
            adjusted.placed.insert(
                id.clone(),
                Arc::new(PlacedFeature {
                    id: Some(id.clone()),
                    feature: crate::feature::surface::fill_layer(height, state),
                    placement: Vec::new(),
                }),
            );
            steps.resize_with(steps.len().max(11), Vec::new);
            steps[10].push(id);
        }
        adjusted
            .biome_features
            .insert(self.settings.biome.clone(), steps);
        Ok(adjusted)
    }
    pub fn build_terrain(&self, position: [i32; 2], dimension: DimensionType) -> GeneratedChunk {
        let biome = self.data.biome_ids[&self.settings.biome];
        let mut chunk = GeneratedChunk::new(
            position,
            NoiseSettings {
                min_y: dimension.min_y,
                height: dimension.height,
            },
            biome,
            self.data.biome_strategy.clone(),
            self.data.motion_blocking.clone(),
            self.data.motion_blocking_no_leaves.clone(),
        );
        for (i, state) in self
            .layers
            .iter()
            .take(dimension.height as usize)
            .enumerate()
        {
            if let Some(state) = state {
                for x in 0..16 {
                    for z in 0..16 {
                        chunk.set_block(x, dimension.min_y + i as i32, z, *state);
                    }
                }
            }
        }
        chunk
    }
}
impl ChunkGeneratorAccess for FlatLevelSource {
    fn data(&self) -> &GenerationData {
        &self.data
    }
    fn biome_source(&self) -> &BiomeSource {
        &self.source
    }
    fn min_y(&self) -> i32 {
        0
    }
    fn gen_depth(&self) -> i32 {
        384
    }
    fn sea_level(&self) -> i32 {
        -63
    }
    fn get_spawn_height(&self, min_y: i32, height: i32) -> i32 {
        min_y + height.min(self.layers.len() as i32)
    }
    fn get_base_height(
        &self,
        _x: i32,
        _z: i32,
        map: Heightmap,
        min_y: i32,
        height: i32,
    ) -> Result<i32, Error> {
        // Vanilla uses the accessor's absolute max Y here, not its height.
        let top = (self.layers.len() as i32 - 1).min(min_y + height - 1);
        for i in (0..=top).rev() {
            if self.layers[i as usize].is_some_and(|state| self.data.heightmap_matches(map, state))
            {
                return Ok(min_y + i + 1);
            }
        }
        Ok(min_y)
    }
    fn get_base_column(
        &self,
        _x: i32,
        _z: i32,
        min_y: i32,
        height: i32,
    ) -> Result<Option<NoiseColumn>, Error> {
        Ok(Some(NoiseColumn {
            min_y,
            states: self
                .layers
                .iter()
                .take(height as usize)
                .map(|state| state.unwrap_or(Block::AIR.default_state()))
                .collect(),
        }))
    }
}
