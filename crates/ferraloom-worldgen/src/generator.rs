//! NoiseBasedChunkGenerator's biome and noise stages, using shared sections.
use crate::{
    Error, Identifier, aquifer::*, biome::*, biome_manager::*, climate::SearchContext, density::*,
    material::*, router::*, settings::*, surface::SurfaceSystem,
};
use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
use ferraloom_data::{
    Block, BlockState,
    datapack::{ResourceManager, TagSet, load_tags},
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

pub mod flat;
pub mod preset;
pub use preset::{ChunkGenerator, LevelStem, WorldDimensions, WorldPreset};

#[derive(Clone, Debug)]
pub struct GenerationData {
    pub density: Arc<DensityRegistry>,
    pub material: MaterialRegistry,
    pub carvers: crate::carver::CarverRegistry,
    pub uncarvable: HashSet<Block>,
    pub temperatures: Arc<BiomeTemperatures>,
    pub biome_names: Vec<Identifier>,
    pub biome_ids: BTreeMap<Identifier, BiomeId>,
    pub biome_tags: BTreeMap<Identifier, Vec<Identifier>>,
    pub block_tags: Arc<TagSet<Block>>,
    pub fluid_tags: Arc<TagSet<ferraloom_data::registries::fluid::Id>>,
    pub item_tags: Arc<TagSet<ferraloom_data::Item>>,
    motion_blocking: Arc<HashSet<Block>>,
    motion_blocking_no_leaves: Arc<HashSet<Block>>,
    biome_strategy: Strategy<BiomeId>,
}
impl GenerationData {
    pub fn load(resources: &ResourceManager) -> Result<Self, Error> {
        let density = Arc::new(DensityRegistry::load(resources)?);
        let material = MaterialRegistry::load(resources)?;
        let carvers = crate::carver::CarverRegistry::load(resources)?;
        let temperatures = Arc::new(BiomeTemperatures::load(resources)?);
        let biome_names = resources
            .registry("worldgen/biome")
            .into_keys()
            .collect::<Vec<_>>();
        let biome_ids = biome_names
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, id)| (id, BiomeId(i as u32)))
            .collect::<BTreeMap<_, _>>();
        let biome_tags = load_tags(resources, "worldgen/biome", |id, _| {
            biome_ids.contains_key(id).then(|| id.clone())
        });
        if !biome_tags.diagnostics.is_empty() {
            return Err(Error(format!(
                "invalid biome tags: {:?}",
                biome_tags.diagnostics
            )));
        }
        let block_tags = load_tags(resources, "block", |id, _| {
            Block::from_name(&id.to_string())
        });
        if !block_tags.diagnostics.is_empty() {
            return Err(Error(format!(
                "invalid block tags: {:?}",
                block_tags.diagnostics
            )));
        }
        let uncarvable = block_tags
            .values
            .get(&"uncarvable".parse()?)
            .ok_or_else(|| Error("missing uncarvable block tag".into()))?
            .iter()
            .copied()
            .collect();
        let motion_blocking = Arc::new(
            block_tags
                .values
                .get(&"blocks_motion_in_heightmap".parse()?)
                .ok_or_else(|| Error("missing heightmap block tag".into()))?
                .iter()
                .copied()
                .collect(),
        );
        let motion_blocking_no_leaves = Arc::new(
            block_tags
                .values
                .get(&"blocks_motion_in_heightmap_no_leaves".parse()?)
                .ok_or_else(|| Error("missing heightmap no-leaves block tag".into()))?
                .iter()
                .copied()
                .collect(),
        );
        let ids = (0..biome_names.len()).map(|i| BiomeId(i as u32)).collect();
        let block_tags = Arc::new(TagSet::from_tags(
            block_tags,
            ferraloom_data::registries::block::NAMES.len(),
        )?);
        let fluid_tags = Arc::new(TagSet::<ferraloom_data::registries::fluid::Id>::load(
            resources,
        )?);
        let item_tags = Arc::new(TagSet::<ferraloom_data::Item>::load(resources)?);
        for (name, diagnostics) in [
            ("fluid", &fluid_tags.diagnostics),
            ("item", &item_tags.diagnostics),
        ] {
            if !diagnostics.is_empty() {
                return Err(Error(format!("invalid {name} tags: {diagnostics:?}")));
            }
        }
        let biome_strategy = Strategy::biomes(Arc::new(
            IdMap::new(ids).map_err(|e| Error(format!("invalid biome registry: {e:?}")))?,
        ));
        Ok(Self {
            density,
            material,
            carvers,
            uncarvable,
            temperatures,
            biome_names,
            biome_ids,
            biome_tags: biome_tags.values,
            block_tags,
            fluid_tags,
            item_tags,
            motion_blocking,
            motion_blocking_no_leaves,
            biome_strategy,
        })
    }
}

impl GenerationData {
    pub fn heightmap_matches(
        &self,
        heightmap: crate::region::Heightmap,
        state: BlockState,
    ) -> bool {
        use crate::region::Heightmap;
        match heightmap {
            Heightmap::WorldSurface | Heightmap::WorldSurfaceWg => !is_air(state),
            Heightmap::OceanFloor | Heightmap::OceanFloorWg => {
                self.motion_blocking.contains(&state.block())
            }
            Heightmap::MotionBlocking => {
                self.motion_blocking.contains(&state.block()) || !state.data().fluid.empty
            }
            Heightmap::MotionBlockingNoLeaves => {
                self.motion_blocking_no_leaves.contains(&state.block()) || !state.data().fluid.empty
            }
        }
    }
}
/// Port of NoiseColumn. Queries outside the captured interval return AIR.
#[derive(Clone, Debug)]
pub struct NoiseColumn {
    pub min_y: i32,
    pub states: Vec<BlockState>,
}
impl NoiseColumn {
    pub fn get_block(&self, y: i32) -> BlockState {
        usize::try_from(y.wrapping_sub(self.min_y))
            .ok()
            .and_then(|y| self.states.get(y))
            .copied()
            .unwrap_or(Block::AIR.default_state())
    }
}

/// Shared seed-specific state. Evaluation never mutates this graph or system.
#[derive(Debug)]
pub struct RandomState {
    pub seed: i64,
    pub compiler: DensityFunctionCompiler,
    pub router: CompiledRouter,
    pub aquifer: Option<CompiledAquifer>,
    pub surface: SurfaceSystem,
    pub spawn_targets: Vec<Vec<(NodeId, crate::climate::Parameter)>>,
}
#[derive(Debug)]
pub struct NoiseBasedChunkGenerator {
    pub data: Arc<GenerationData>,
    pub settings: NoiseGeneratorSettings,
    pub source: BiomeSource,
    pub random: RandomState,
}
/// Port of ChunkGenerator's shared feature and column-query interface.
pub trait ChunkGeneratorAccess: std::fmt::Debug + Send + Sync {
    fn data(&self) -> &GenerationData;
    fn biome_source(&self) -> &BiomeSource;
    fn min_y(&self) -> i32;
    fn gen_depth(&self) -> i32;
    fn sea_level(&self) -> i32;
    fn get_base_height(
        &self,
        x: i32,
        z: i32,
        heightmap: crate::region::Heightmap,
        min_y: i32,
        height: i32,
    ) -> Result<i32, Error>;
    fn get_base_column(
        &self,
        x: i32,
        z: i32,
        min_y: i32,
        height: i32,
    ) -> Result<Option<NoiseColumn>, Error>;
    fn get_spawn_height(&self, _min_y: i32, _height: i32) -> i32 {
        64
    }
    fn get_origin(&self) -> Result<[i32; 2], Error> {
        Ok([0, 0])
    }
}
impl ChunkGeneratorAccess for NoiseBasedChunkGenerator {
    fn get_origin(&self) -> Result<[i32; 2], Error> {
        Ok(crate::spawn::noise_spawn_position(&self.random).map(|value| value >> 4))
    }
    fn data(&self) -> &GenerationData {
        &self.data
    }
    fn biome_source(&self) -> &BiomeSource {
        &self.source
    }
    fn min_y(&self) -> i32 {
        self.settings.noise.min_y
    }
    fn gen_depth(&self) -> i32 {
        self.settings.noise.height
    }
    fn sea_level(&self) -> i32 {
        self.settings.sea_level
    }
    fn get_base_height(
        &self,
        x: i32,
        z: i32,
        heightmap: crate::region::Heightmap,
        min_y: i32,
        height: i32,
    ) -> Result<i32, Error> {
        self.get_base_height(x, z, heightmap, min_y, height)
    }
    fn get_base_column(
        &self,
        x: i32,
        z: i32,
        min_y: i32,
        height: i32,
    ) -> Result<Option<NoiseColumn>, Error> {
        self.get_base_column(x, z, min_y, height)
    }
}
impl NoiseBasedChunkGenerator {
    pub fn new(
        data: Arc<GenerationData>,
        settings: NoiseGeneratorSettings,
        source: BiomeSource,
        seed: i64,
    ) -> Result<Self, Error> {
        let mut compiler =
            DensityFunctionCompiler::new(data.density.clone(), seed, settings.legacy_random_source);
        let router = settings.router.compile(&mut compiler)?;
        let aquifer = settings
            .aquifers
            .as_ref()
            .map(|config| config.compile(&mut compiler))
            .transpose()?;
        let possible = source.possible_biomes();
        let surface = SurfaceSystem::new(
            &mut compiler,
            &data.material,
            &settings,
            &data.biome_tags,
            &possible,
            router.functions[6],
        )?;
        let spawn_targets = settings
            .spawn_target
            .iter()
            .map(|point| {
                point
                    .iter()
                    .map(|(function, range)| Ok((compiler.compile_reference(function)?, *range)))
                    .collect::<Result<Vec<_>, Error>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            data,
            settings,
            source,
            random: RandomState {
                seed,
                compiler,
                router,
                aquifer,
                surface,
                spawn_targets,
            },
        })
    }
    pub fn new_chunk(&self, position: [i32; 2]) -> GeneratedChunk {
        let default = self
            .data
            .biome_ids
            .get(&"plains".parse().unwrap())
            .copied()
            .unwrap_or(BiomeId(0));
        GeneratedChunk::new(
            position,
            self.settings.noise,
            default,
            self.data.biome_strategy.clone(),
            self.data.motion_blocking.clone(),
            self.data.motion_blocking_no_leaves.clone(),
        )
    }
    pub fn new_chunk_in_dimension(
        &self,
        position: [i32; 2],
        dimension: DimensionType,
    ) -> GeneratedChunk {
        let default = self
            .data
            .biome_ids
            .get(&"plains".parse().unwrap())
            .copied()
            .unwrap_or(BiomeId(0));
        GeneratedChunk::new(
            position,
            NoiseSettings {
                min_y: dimension.min_y,
                height: dimension.height,
            },
            default,
            self.data.biome_strategy.clone(),
            self.data.motion_blocking.clone(),
            self.data.motion_blocking_no_leaves.clone(),
        )
    }
    pub fn create_biomes(&self, chunk: &mut GeneratedChunk) {
        self.create_biomes_with_blender(chunk, None);
    }
    pub fn create_biomes_with_blender(
        &self,
        chunk: &mut GeneratedChunk,
        blender: Option<&crate::blending::Blender>,
    ) {
        let mut samplers = SamplerContext::cached();
        let mut search = SearchContext::default();
        let grid = self.source.sample_quart_grid_with_blender(
            &self.random.compiler,
            self.random.router,
            &mut samplers,
            &mut search,
            [
                chunk.position[0] * 4,
                chunk.min_y / 4,
                chunk.position[1] * 4,
            ],
            [4, chunk.height as usize / 4, 4],
            blender,
        );
        let size_y = chunk.height as usize / 4;
        for (section_index, section) in chunk.sections.iter_mut().enumerate() {
            for x in 0..4 {
                for y in 0..4 {
                    for z in 0..4 {
                        let id = &grid[(z * 4 + x) * size_y + section_index * 4 + y];
                        section.biomes_mut().set(
                            x as u8,
                            y as u8,
                            z as u8,
                            self.data.biome_ids[id],
                        );
                    }
                }
            }
        }
    }
    pub fn noise_chunk(&self, chunk: &GeneratedChunk) -> NoiseChunk {
        self.noise_chunk_with_blender(chunk, None)
    }
    pub fn noise_chunk_with_blender(
        &self,
        chunk: &GeneratedChunk,
        blender: Option<Arc<crate::blending::Blender>>,
    ) -> NoiseChunk {
        let min_y = chunk.min_y.max(self.settings.noise.min_y);
        let max_y = (chunk.min_y + chunk.height)
            .min(self.settings.noise.min_y + self.settings.noise.height);
        let volume = DensityVolume {
            size: [16, (max_y - min_y).max(0) as usize, 16],
            min: [chunk.position[0] * 16, min_y, chunk.position[1] * 16],
            step: [1; 3],
        };
        let mut samplers = SamplerContext::cached();
        samplers.set_beardifier(None);
        if let Some(blender) = blender {
            samplers.set_blender(blender, volume);
        }
        let picker = FluidPicker {
            sea_level: self.settings.sea_level,
            default_fluid: self.settings.default_fluid,
        };
        let aquifer = Aquifer::new(
            if volume.is_empty() {
                None
            } else {
                self.random.aquifer
            },
            picker,
            &self.random.compiler,
            &mut samplers,
            volume,
        );
        NoiseChunk {
            volume,
            samplers,
            aquifer,
        }
    }
    pub fn fill_from_noise(&self, chunk: &mut GeneratedChunk, noise: &mut NoiseChunk) {
        if noise.volume.is_empty() {
            return;
        }
        let values = self.random.compiler.sample_volume(
            self.random.router.functions[7],
            &mut noise.samplers,
            noise.volume,
        );
        for z in 0..16 {
            for x in 0..16 {
                for y in (0..noise.volume.size[1]).rev() {
                    let p = [
                        noise.volume.min[0] + x as i32,
                        noise.volume.min[1] + y as i32,
                        noise.volume.min[2] + z as i32,
                    ];
                    let state = noise
                        .aquifer
                        .compute_substance(
                            &self.random.compiler,
                            &mut noise.samplers,
                            p,
                            f64::from(values[noise.volume.index(x, y, z)]),
                        )
                        .unwrap_or(self.settings.default_block);
                    if state != Block::AIR.default_state() {
                        chunk.set_block(x as u8, p[1], z as u8, state);
                        if noise.aquifer.should_schedule_fluid_update() && !state.data().fluid.empty
                        {
                            chunk.post_processing.push(p);
                        }
                    }
                }
            }
        }
    }
    /// Port of NoiseBasedChunkGenerator#getBaseHeight. This samples a fresh
    /// one-block-wide noise column, never a generated chunk or structure input.
    pub fn get_base_height(
        &self,
        x: i32,
        z: i32,
        heightmap: crate::region::Heightmap,
        min_y: i32,
        height: i32,
    ) -> Result<i32, Error> {
        Ok(self
            .iterate_noise_column(x, z, min_y, height, Some(heightmap), false)
            .0
            .unwrap_or(min_y))
    }
    /// Port of ChunkGenerator#getFirstFreeHeight.
    pub fn get_first_free_height(
        &self,
        x: i32,
        z: i32,
        heightmap: crate::region::Heightmap,
        min_y: i32,
        height: i32,
    ) -> Result<i32, Error> {
        self.get_base_height(x, z, heightmap, min_y, height)
    }
    /// Port of ChunkGenerator#getFirstOccupiedHeight, including the empty
    /// column result of level minimum Y minus one.
    pub fn get_first_occupied_height(
        &self,
        x: i32,
        z: i32,
        heightmap: crate::region::Heightmap,
        min_y: i32,
        height: i32,
    ) -> Result<i32, Error> {
        Ok(self
            .get_base_height(x, z, heightmap, min_y, height)?
            .wrapping_sub(1))
    }
    /// Port of NoiseBasedChunkGenerator#getBaseColumn. Vanilla returns null if
    /// the level and generation height ranges do not overlap.
    pub fn get_base_column(
        &self,
        x: i32,
        z: i32,
        min_y: i32,
        height: i32,
    ) -> Result<Option<NoiseColumn>, Error> {
        Ok(self.iterate_noise_column(x, z, min_y, height, None, true).1)
    }
    fn iterate_noise_column(
        &self,
        x: i32,
        z: i32,
        level_min: i32,
        level_height: i32,
        tester: Option<crate::region::Heightmap>,
        capture: bool,
    ) -> (Option<i32>, Option<NoiseColumn>) {
        let min_y = self.settings.noise.min_y.max(level_min);
        let height = self
            .settings
            .noise
            .min_y
            .wrapping_add(self.settings.noise.height)
            .min(level_min.wrapping_add(level_height))
            .wrapping_sub(min_y);
        if height <= 0 {
            return (None, None);
        }
        let volume = DensityVolume {
            size: [1, height as usize, 1],
            min: [x, min_y, z],
            step: [1; 3],
        };
        let mut samplers = SamplerContext::cached();
        let mut aquifer = Aquifer::new(
            self.random.aquifer,
            FluidPicker {
                sea_level: self.settings.sea_level,
                default_fluid: self.settings.default_fluid,
            },
            &self.random.compiler,
            &mut samplers,
            volume,
        );
        let density = self.random.compiler.sample_volume(
            self.random.router.functions[7],
            &mut samplers,
            volume,
        );
        let mut column = capture.then(|| NoiseColumn {
            min_y,
            states: vec![Block::AIR.default_state(); height as usize],
        });
        for y in (0..height as usize).rev() {
            let block_y = min_y + y as i32;
            let state = aquifer
                .compute_substance(
                    &self.random.compiler,
                    &mut samplers,
                    [x, block_y, z],
                    f64::from(density[volume.index(0, y, 0)]),
                )
                .unwrap_or(self.settings.default_block);
            if let Some(column) = &mut column {
                column.states[y] = state;
            }
            if tester.is_some_and(|heightmap| self.data.heightmap_matches(heightmap, state)) {
                return (Some(block_y + 1), column);
            }
        }
        (None, column)
    }
    pub fn build_surface(&self, chunk: &mut GeneratedChunk, noise: &mut NoiseChunk) {
        if noise.volume.is_empty() {
            return;
        }
        let mut lookup = BiomeLookup::new(self, chunk, noise.samplers.blender().cloned());
        self.random.surface.build(
            chunk,
            noise,
            &self.random.compiler,
            &self.data.temperatures,
            &mut |p| lookup.at_block(self, p),
        );
    }
    /// Port of NoiseBasedChunkGenerator#generateCarvers. Mask generation and
    /// application share no mutable state with another chunk job.
    pub fn generate_carvers(
        &self,
        chunk: &mut GeneratedChunk,
        noise: &mut NoiseChunk,
        upgrading: bool,
    ) -> Result<(), Error> {
        if noise.volume.is_empty() {
            return Ok(());
        }
        let context = crate::placement::providers::WorldGenerationContext::new(
            self.settings.noise.min_y,
            self.settings.noise.height,
            chunk.min_y,
            chunk.height,
            self.settings.sea_level,
        );
        let mut search = SearchContext::default();
        let mut samplers = SamplerContext::uncached();
        let mask = self.data.carvers.generate_mask(
            context,
            self.random.seed,
            chunk.position,
            upgrading,
            |quart| {
                self.source.sample(
                    &self.random.compiler,
                    self.random.router,
                    &mut samplers,
                    &mut search,
                    quart,
                )
            },
        )?;
        let blender = noise.samplers.blender().cloned();
        let mut lookup = BiomeLookup::new(self, chunk, blender.clone());
        mask.visit(|x, z, bottom, top| {
            let mut has_grass = false;
            for y in (bottom..=top).rev() {
                if blender
                    .as_ref()
                    .is_some_and(|b| !b.can_carve(chunk.position, x, y, z))
                {
                    continue;
                }
                let old = chunk.block(x, y, z);
                if self.data.uncarvable.contains(&old.block()) {
                    continue;
                }
                if matches!(old.block(), Block::GRASS_BLOCK | Block::MYCELIUM) {
                    has_grass = true;
                }
                let p = [
                    chunk.position[0] * 16 + i32::from(x),
                    y,
                    chunk.position[1] * 16 + i32::from(z),
                ];
                if let Some(state) = noise.aquifer.compute_substance(
                    &self.random.compiler,
                    &mut noise.samplers,
                    p,
                    0.0,
                ) {
                    chunk.set_block(x, y, z, state);
                    if noise.aquifer.should_schedule_fluid_update() && !state.data().fluid.empty {
                        chunk.post_processing.push(p);
                    }
                    if has_grass && chunk.block(x, y - 1, z).block() == Block::DIRT {
                        let below = [p[0], y - 1, p[2]];
                        if let Some(material) = self.random.surface.top_material(
                            chunk,
                            noise,
                            &self.random.compiler,
                            &self.data.temperatures,
                            &mut |p| lookup.at_block(self, p),
                            below,
                            !state.data().fluid.empty,
                        ) {
                            chunk.set_block(x, y - 1, z, material);
                            if !material.data().fluid.empty {
                                chunk.post_processing.push(below);
                            }
                        }
                    }
                }
            }
        });
        Ok(())
    }
    /// Full 26.3 TERRAIN status: noise, surface, then carvers. Structure starts
    /// supply a Beardifier before density is sampled. `generate` remains the
    /// slice-1 noise/surface diagnostic API.
    pub fn build_terrain(
        &self,
        position: [i32; 2],
        beardifier: Option<Arc<crate::structure::Beardifier>>,
    ) -> Result<GeneratedChunk, Error> {
        let mut chunk = self.new_chunk(position);
        self.create_biomes(&mut chunk);
        let mut noise = self.noise_chunk(&chunk);
        noise.samplers.set_beardifier(beardifier);
        self.fill_from_noise(&mut chunk, &mut noise);
        self.build_surface(&mut chunk, &mut noise);
        self.generate_carvers(&mut chunk, &mut noise, false)?;
        Ok(chunk)
    }
    pub fn generate_in_dimension(
        &self,
        position: [i32; 2],
        dimension: DimensionType,
    ) -> GeneratedChunk {
        let mut chunk = self.new_chunk_in_dimension(position, dimension);
        self.create_biomes(&mut chunk);
        let mut noise = self.noise_chunk(&chunk);
        self.fill_from_noise(&mut chunk, &mut noise);
        self.build_surface(&mut chunk, &mut noise);
        chunk
    }
    pub fn generate_blended(
        &self,
        position: [i32; 2],
        blender: Arc<crate::blending::Blender>,
    ) -> GeneratedChunk {
        let mut chunk = self.new_chunk(position);
        self.create_biomes_with_blender(&mut chunk, Some(&blender));
        let mut noise = self.noise_chunk_with_blender(&chunk, Some(blender));
        self.fill_from_noise(&mut chunk, &mut noise);
        self.build_surface(&mut chunk, &mut noise);
        chunk
    }
    pub fn generate(&self, position: [i32; 2]) -> GeneratedChunk {
        let mut chunk = self.new_chunk(position);
        self.create_biomes(&mut chunk);
        let mut noise = self.noise_chunk(&chunk);
        self.fill_from_noise(&mut chunk, &mut noise);
        self.build_surface(&mut chunk, &mut noise);
        chunk
    }
}
/// Port of NoiseChunk: all density caches and aquifer mutations are job-local.
#[derive(Debug)]
pub struct NoiseChunk {
    pub volume: DensityVolume,
    pub samplers: SamplerContext,
    pub aquifer: Aquifer,
}

/// Generation output uses the reusable, vanilla paletted LevelChunkSection.
/// Heightmaps store first available Y (highest matching block plus one).
#[derive(Clone, Debug)]
pub struct GeneratedChunk {
    pub position: [i32; 2],
    pub min_y: i32,
    pub height: i32,
    pub sections: Vec<LevelChunkSection>,
    pub world_surface: [i32; 256],
    pub ocean_floor: [i32; 256],
    pub post_processing: Vec<[i32; 3]>,
    motion_blocking: Arc<HashSet<Block>>,
    motion_blocking_no_leaves: Arc<HashSet<Block>>,
}
impl GeneratedChunk {
    fn new(
        position: [i32; 2],
        height: NoiseSettings,
        default_biome: BiomeId,
        biomes: Strategy<BiomeId>,
        motion_blocking: Arc<HashSet<Block>>,
        motion_blocking_no_leaves: Arc<HashSet<Block>>,
    ) -> Self {
        let blocks = Strategy::<BlockState>::block_states();
        let sections = (0..height.height / 16)
            .map(|_| LevelChunkSection::new(blocks.clone(), biomes.clone(), default_biome))
            .collect();
        Self {
            position,
            min_y: height.min_y,
            height: height.height,
            sections,
            world_surface: [height.min_y; 256],
            ocean_floor: [height.min_y; 256],
            post_processing: Vec::new(),
            motion_blocking,
            motion_blocking_no_leaves,
        }
    }
    pub fn block(&self, x: u8, y: i32, z: u8) -> BlockState {
        if y < self.min_y || y >= self.min_y + self.height {
            return Block::VOID_AIR.default_state();
        }
        self.sections[((y - self.min_y) / 16) as usize].get_block_state(
            x,
            y.rem_euclid(16) as u8,
            z,
        )
    }
    pub fn set_block(&mut self, x: u8, y: i32, z: u8, state: BlockState) {
        if y < self.min_y || y >= self.min_y + self.height {
            return;
        }
        self.sections[((y - self.min_y) / 16) as usize].set_block_state(
            x,
            y.rem_euclid(16) as u8,
            z,
            state,
        );
        let index = z as usize * 16 + x as usize;
        for surface in [true, false] {
            let opaque = |state: BlockState| {
                if surface {
                    !is_air(state)
                } else {
                    self.motion_blocking.contains(&state.block())
                }
            };
            let height = if surface {
                self.world_surface[index]
            } else {
                self.ocean_floor[index]
            };
            let new = if opaque(state) && y >= height {
                y + 1
            } else if y == height - 1 && !opaque(state) {
                let mut h = self.min_y;
                for lower in (self.min_y..y).rev() {
                    if opaque(self.block(x, lower, z)) {
                        h = lower + 1;
                        break;
                    }
                }
                h
            } else {
                height
            };
            if surface {
                self.world_surface[index] = new;
            } else {
                self.ocean_floor[index] = new;
            }
        }
    }
    pub(crate) fn heightmap_matches(
        &self,
        map: crate::region::Heightmap,
        state: BlockState,
    ) -> bool {
        use crate::region::Heightmap;
        match map {
            Heightmap::WorldSurface | Heightmap::WorldSurfaceWg => !is_air(state),
            Heightmap::OceanFloor | Heightmap::OceanFloorWg => {
                self.motion_blocking.contains(&state.block())
            }
            Heightmap::MotionBlocking => {
                self.motion_blocking.contains(&state.block()) || !state.data().fluid.empty
            }
            Heightmap::MotionBlockingNoLeaves => {
                self.motion_blocking_no_leaves.contains(&state.block()) || !state.data().fluid.empty
            }
        }
    }
    pub(crate) fn scan_height(&self, map: crate::region::Heightmap, x: u8, z: u8) -> i32 {
        self.scan_height_below(map, x, z, self.min_y + self.height)
    }
    pub(crate) fn scan_height_below(
        &self,
        map: crate::region::Heightmap,
        x: u8,
        z: u8,
        top: i32,
    ) -> i32 {
        for y in (self.min_y..top).rev() {
            if self.heightmap_matches(map, self.block(x, y, z)) {
                return y + 1;
            }
        }
        self.min_y
    }
    pub fn biome(&self, quart: [i32; 3]) -> BiomeId {
        let y = quart[1].clamp(self.min_y >> 2, ((self.min_y + self.height) >> 2) - 1);
        let relative = y - (self.min_y >> 2);
        self.sections[(relative / 4) as usize].biomes().get(
            (quart[0] & 3) as u8,
            (y & 3) as u8,
            (quart[2] & 3) as u8,
        )
    }
}
pub(crate) fn is_air(state: BlockState) -> bool {
    matches!(
        state.block(),
        Block::AIR | Block::CAVE_AIR | Block::VOID_AIR
    )
}
struct BiomeLookup {
    blender: Option<Arc<crate::blending::Blender>>,
    chunks: HashMap<[i32; 2], Vec<BiomeId>>,
    samplers: SamplerContext,
    search: SearchContext,
    zoom_seed: i64,
    min_y: i32,
    height: i32,
}
impl BiomeLookup {
    fn new(
        generator: &NoiseBasedChunkGenerator,
        chunk: &GeneratedChunk,
        blender: Option<Arc<crate::blending::Blender>>,
    ) -> Self {
        let mut chunks = HashMap::new();
        let size_y = chunk.height as usize / 4;
        let mut biomes = vec![BiomeId(0); 16 * size_y];
        for z in 0..4 {
            for x in 0..4 {
                for y in 0..size_y {
                    biomes[(z * 4 + x) * size_y + y] =
                        chunk.biome([x as i32, chunk.min_y / 4 + y as i32, z as i32]);
                }
            }
        }
        chunks.insert(chunk.position, biomes);
        Self {
            blender,
            chunks,
            samplers: SamplerContext::cached(),
            search: SearchContext::default(),
            zoom_seed: obfuscate_seed(generator.random.seed),
            min_y: chunk.min_y,
            height: chunk.height,
        }
    }
    fn at_block(&mut self, generator: &NoiseBasedChunkGenerator, p: [i32; 3]) -> Identifier {
        let quart = zoom_quart(self.zoom_seed, p);
        let key = [quart[0] >> 2, quart[2] >> 2];
        let size_y = self.height as usize / 4;
        let biomes = self.chunks.entry(key).or_insert_with(|| {
            generator
                .source
                .sample_quart_grid_with_blender(
                    &generator.random.compiler,
                    generator.random.router,
                    &mut self.samplers,
                    &mut self.search,
                    [key[0] * 4, self.min_y / 4, key[1] * 4],
                    [4, size_y, 4],
                    self.blender.as_deref(),
                )
                .into_iter()
                .map(|id| generator.data.biome_ids[&id])
                .collect()
        });
        let y = quart[1].clamp(self.min_y / 4, (self.min_y + self.height) / 4 - 1) - self.min_y / 4;
        let id = biomes[(((quart[2] & 3) * 4 + (quart[0] & 3)) as usize) * size_y + y as usize];
        generator.data.biome_names[id.0 as usize].clone()
    }
}
