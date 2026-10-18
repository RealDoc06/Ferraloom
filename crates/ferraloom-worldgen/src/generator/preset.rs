//! Ports of ChunkGenerator dispatch, WorldPreset and WorldDimensions.
use super::{flat::*, *};
use crate::{
    codec::*,
    region::{Heightmap, WorldGenRegion},
};
use ferraloom_data::datapack::ResourceManager;
use serde_json::Value;

#[derive(Debug)]
pub enum ChunkGenerator {
    Noise(Box<NoiseBasedChunkGenerator>),
    Flat(Box<FlatLevelSource>),
    Debug(DebugLevelSource),
}
impl ChunkGenerator {
    pub fn decode(
        v: &Value,
        resources: &ResourceManager,
        data: Arc<GenerationData>,
        seed: i64,
    ) -> Result<Self, Error> {
        match kind(v)?.to_string().as_str() {
            "minecraft:noise" => {
                let setting = field(v, "settings")?;
                let noise =
                    if setting.is_string() {
                        let name = id(setting)?;
                        resources
                            .require(&name.with_path(format!(
                                "worldgen/noise_settings/{}.json",
                                name.path()
                            ))?)?
                            .json()?
                    } else {
                        setting.clone()
                    };
                Ok(Self::Noise(Box::new(NoiseBasedChunkGenerator::new(
                    data,
                    NoiseGeneratorSettings::decode(&noise)?,
                    BiomeSource::decode(field(v, "biome_source")?, resources)?,
                    seed,
                )?)))
            }
            "minecraft:flat" => Ok(Self::Flat(Box::new(FlatLevelSource::new(
                data,
                FlatLevelGeneratorSettings::decode(field(v, "settings")?, resources)?,
            )?))),
            "minecraft:debug" => Ok(Self::Debug(DebugLevelSource::new(data))),
            name => Err(Error(format!("unknown chunk generator {name}"))),
        }
    }
    pub fn access(&self) -> &dyn ChunkGeneratorAccess {
        match self {
            Self::Noise(g) => g.as_ref(),
            Self::Flat(g) => g.as_ref(),
            Self::Debug(g) => g,
        }
    }
    pub fn build_terrain(
        &self,
        p: [i32; 2],
        dimension: DimensionType,
        beardifier: Option<Arc<crate::structure::Beardifier>>,
    ) -> Result<GeneratedChunk, Error> {
        match self {
            Self::Noise(g) => {
                let mut chunk = g.new_chunk_in_dimension(p, dimension);
                g.create_biomes(&mut chunk);
                let mut noise = g.noise_chunk(&chunk);
                noise.samplers.set_beardifier(beardifier);
                g.fill_from_noise(&mut chunk, &mut noise);
                g.build_surface(&mut chunk, &mut noise);
                g.generate_carvers(&mut chunk, &mut noise, false)?;
                Ok(chunk)
            }
            Self::Flat(g) => Ok(g.build_terrain(p, dimension)),
            Self::Debug(g) => Ok(g.build_terrain(p, dimension)),
        }
    }
}
impl ChunkGeneratorAccess for ChunkGenerator {
    fn get_origin(&self) -> Result<[i32; 2], Error> {
        self.access().get_origin()
    }
    fn data(&self) -> &GenerationData {
        self.access().data()
    }
    fn biome_source(&self) -> &BiomeSource {
        self.access().biome_source()
    }
    fn min_y(&self) -> i32 {
        self.access().min_y()
    }
    fn gen_depth(&self) -> i32 {
        self.access().gen_depth()
    }
    fn sea_level(&self) -> i32 {
        self.access().sea_level()
    }
    fn get_spawn_height(&self, min_y: i32, height: i32) -> i32 {
        self.access().get_spawn_height(min_y, height)
    }
    fn get_base_height(
        &self,
        x: i32,
        z: i32,
        map: Heightmap,
        min_y: i32,
        height: i32,
    ) -> Result<i32, Error> {
        self.access().get_base_height(x, z, map, min_y, height)
    }
    fn get_base_column(
        &self,
        x: i32,
        z: i32,
        min_y: i32,
        height: i32,
    ) -> Result<Option<NoiseColumn>, Error> {
        self.access().get_base_column(x, z, min_y, height)
    }
}

/// Port of DebugLevelSource. State ids retain the block registry's state order.
#[derive(Debug)]
pub struct DebugLevelSource {
    data: Arc<GenerationData>,
    source: BiomeSource,
}
impl DebugLevelSource {
    pub fn new(data: Arc<GenerationData>) -> Self {
        Self {
            data,
            source: BiomeSource::Fixed("plains".parse().unwrap()),
        }
    }
    pub fn block_state_for(mut x: i32, mut z: i32) -> BlockState {
        let count = ferraloom_data::BLOCK_STATES.len();
        let width = ((f64::from(count as f32).sqrt() as f32).ceil()) as i32;
        let height = (count as f32 / width as f32).ceil() as i32;
        if x > 0 && z > 0 && x % 2 != 0 && z % 2 != 0 {
            x /= 2;
            z /= 2;
            if x <= width && z <= height {
                let index = x.wrapping_mul(width).wrapping_add(z).wrapping_abs();
                if index < count as i32 {
                    return BlockState::from_id(index as u16).unwrap();
                }
            }
        }
        Block::AIR.default_state()
    }
    pub fn build_terrain(&self, p: [i32; 2], dimension: DimensionType) -> GeneratedChunk {
        GeneratedChunk::new(
            p,
            NoiseSettings {
                min_y: dimension.min_y,
                height: dimension.height,
            },
            self.data.biome_ids[&"plains".parse().unwrap()],
            self.data.biome_strategy.clone(),
            self.data.motion_blocking.clone(),
            self.data.motion_blocking_no_leaves.clone(),
        )
    }
    pub fn apply_biome_decoration(&self, region: &mut WorldGenRegion<'_>) -> Result<(), Error> {
        let p = region.center();
        for x in 0..16 {
            for z in 0..16 {
                let x = p[0] * 16 + x;
                let z = p[1] * 16 + z;
                region.set_block([x, 60, z], Block::BARRIER.default_state(), 2)?;
                region.set_block([x, 70, z], Self::block_state_for(x, z), 2)?;
            }
        }
        Ok(())
    }
}
impl ChunkGeneratorAccess for DebugLevelSource {
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
        63
    }
    fn get_base_height(&self, _: i32, _: i32, _: Heightmap, _: i32, _: i32) -> Result<i32, Error> {
        Ok(0)
    }
    fn get_base_column(
        &self,
        _: i32,
        _: i32,
        _: i32,
        _: i32,
    ) -> Result<Option<NoiseColumn>, Error> {
        Ok(Some(NoiseColumn {
            min_y: 0,
            states: Vec::new(),
        }))
    }
}

#[derive(Debug)]
pub struct LevelStem {
    pub name: Identifier,
    pub dimension: DimensionType,
    pub generator: Arc<ChunkGenerator>,
}
#[derive(Debug)]
pub struct WorldDimensions {
    /// Vanilla's built-in order first, followed by remaining dimension entries.
    pub dimensions: Vec<LevelStem>,
}
impl WorldDimensions {
    pub fn overworld(&self) -> &LevelStem {
        self.dimensions
            .iter()
            .find(|stem| stem.name == "overworld".parse().unwrap())
            .expect("validated overworld")
    }
}
#[derive(Debug)]
pub struct WorldPreset {
    pub definition: Value,
    pub dimensions: WorldDimensions,
}
impl WorldPreset {
    pub fn decode(
        v: &Value,
        resources: &ResourceManager,
        data: Arc<GenerationData>,
        seed: i64,
    ) -> Result<Self, Error> {
        let entries = field(v, "dimensions")?
            .as_object()
            .ok_or_else(|| Error("expected dimension map".into()))?;
        let mut dimensions = Vec::new();
        for (name, stem) in entries {
            let name: Identifier = name.parse()?;
            let encoded_type = field(stem, "type")?;
            let dimension = if encoded_type.is_string() {
                let name = id(encoded_type)?;
                DimensionType::decode(
                    &resources
                        .require(&name.with_path(format!("dimension_type/{}.json", name.path()))?)?
                        .json()?,
                )?
            } else {
                DimensionType::decode(encoded_type)?
            };
            dimensions.push(LevelStem {
                name,
                dimension,
                generator: Arc::new(ChunkGenerator::decode(
                    field(stem, "generator")?,
                    resources,
                    data.clone(),
                    seed,
                )?),
            });
        }
        let builtins: [Identifier; 3] =
            ["overworld", "the_nether", "the_end"].map(|name| name.parse().unwrap());
        if !dimensions.iter().any(|stem| stem.name == builtins[0]) {
            return Err(Error("missing overworld dimension".into()));
        }
        dimensions.sort_by_key(|stem| {
            builtins
                .iter()
                .position(|name| name == &stem.name)
                .unwrap_or(3)
        });
        Ok(Self {
            definition: v.clone(),
            dimensions: WorldDimensions { dimensions },
        })
    }
    pub fn load(
        name: &Identifier,
        resources: &ResourceManager,
        data: Arc<GenerationData>,
        seed: i64,
    ) -> Result<Self, Error> {
        Self::decode(
            &resources
                .require(&name.with_path(format!("worldgen/world_preset/{}.json", name.path()))?)?
                .json()?,
            resources,
            data,
            seed,
        )
    }
    /// Port of DedicatedServerProperties.WorldDimensionData#create. The legacy
    /// names are considered only if Identifier parsing itself fails, as vanilla.
    pub fn from_server_properties(
        level_type: &str,
        generator_settings: &Value,
        resources: &ResourceManager,
        data: Arc<GenerationData>,
        seed: i64,
    ) -> Result<Self, Error> {
        if !generator_settings.is_object() {
            return Err(Error("generator-settings must be an object".into()));
        }
        let known = resources.registry("worldgen/world_preset");
        let normal: Identifier = "normal".parse()?;
        let fallback = if known.contains_key(&normal) {
            normal
        } else {
            known
                .keys()
                .next()
                .cloned()
                .ok_or_else(|| Error("no world presets".into()))?
        };
        let level_type = level_type.to_lowercase();
        let selected = level_type
            .parse::<Identifier>()
            .ok()
            .or_else(|| match level_type.as_str() {
                "default" => Some("normal".parse().unwrap()),
                "largebiomes" => Some("large_biomes".parse().unwrap()),
                _ => None,
            })
            .filter(|name| known.contains_key(name))
            .unwrap_or(fallback);
        let mut preset = Self::load(&selected, resources, data.clone(), seed)?;
        let flat_settings = if selected == "flat".parse()? {
            match FlatLevelGeneratorSettings::decode_partial(generator_settings, resources) {
                Ok(settings) => Some(settings),
                Err(error) if error.0.starts_with("Unsupported:") => return Err(error),
                Err(_) => None,
            }
        } else {
            None
        };
        if let Some(settings) = flat_settings {
            let encoded = settings.encode();
            let generator = Arc::new(ChunkGenerator::Flat(Box::new(FlatLevelSource::new(
                data, settings,
            )?)));
            let overworld = preset
                .dimensions
                .dimensions
                .iter_mut()
                .find(|stem| stem.name == "overworld".parse().unwrap())
                .unwrap();
            overworld.generator = generator;
            preset.definition["dimensions"]["minecraft:overworld"]["generator"]["settings"] =
                encoded;
        }
        Ok(preset)
    }
}
