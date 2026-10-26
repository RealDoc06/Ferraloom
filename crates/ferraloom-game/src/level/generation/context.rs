//! Immutable worldgen context and job-local execution of a single status step.
use super::{ChunkStatus, ChunkStatusTask, ChunkStep};
use crate::block::GameWorldgenBlockBehavior;
use ferraloom_data::datapack::ResourceManager;
use ferraloom_util::random::RandomSource;
use ferraloom_worldgen::{
    Error, Identifier,
    climate::SearchContext,
    decoration::DecorationPlan,
    density::SamplerContext,
    environment::{DimensionEnvironment, EnvironmentDefinitions},
    feature::{FeatureLoader, FeatureRegistry},
    generator::{ChunkGenerator, ChunkGeneratorAccess},
    region::{ProtoChunkLightEngine, RegionChunk, WorldGenRegion},
    settings::DimensionType,
    structure::{
        Beardifier,
        decoration::{RegionStructureDecoration, StructureStep},
        generation::{StructureGenerationContext, StructureGenerators},
        pipeline::{self, StructureRegistry},
        placement::{BiomeSet, ChunkGeneratorStructureState, StructurePlacement, find_ring_biome},
        pool::StructurePools,
        rule::BlockTags,
        template::StructureTemplateManager,
    },
};
use std::sync::Arc;

/// Port of WorldGenContext. Registries are immutable; samplers, region RNG and
/// structure mutation stay inside one owned job neighborhood.
pub struct WorldGenContext {
    registry_context: Option<Arc<dyn ferraloom_protocol::codec::RegistryContext>>,
    pub generator: Arc<ChunkGenerator>,
    pub dimension: DimensionType,
    pub seed: i64,
    pub resources: ResourceManager,
    pub features: FeatureRegistry,
    pub structures: StructureGenerators,
    pub templates: StructureTemplateManager,
    pub pools: StructurePools,
    pub structure_state: ChunkGeneratorStructureState,
    pub possible_sets: Vec<Identifier>,
    pub structure_steps: Vec<StructureStep>,
    pub tags: BlockTags,
    pub decoration: DecorationPlan,
    pub generate_structures: bool,
    pub environment: Arc<EnvironmentDefinitions>,
    pub dimension_environment: DimensionEnvironment,
    pub unported_behavior: crate::block::UnportedBehavior,
}
impl WorldGenContext {
    pub fn load(
        resources: &ResourceManager,
        generator: Arc<ChunkGenerator>,
        dimension: DimensionType,
        seed: i64,
        generate_structures: bool,
        environment: Arc<EnvironmentDefinitions>,
        dimension_environment: DimensionEnvironment,
    ) -> Result<Self, Error> {
        let mut loader = FeatureLoader::new(resources)?;
        let registry = StructureRegistry::load(resources, generator.data())?;
        let structures = StructureGenerators::load(resources, registry, &mut |v| loader.placed(v))?;
        let pools = StructurePools::load(resources, &mut |v| loader.placed(v))?;
        let features = match generator.as_ref() {
            ChunkGenerator::Flat(flat) => flat.feature_registry(resources)?,
            _ => loader.load_all()?,
        };
        let templates = StructureTemplateManager::load_all(resources)?;
        let mut structure_state =
            ChunkGeneratorStructureState::load(resources, seed, generator.get_origin()?)?;
        let mut possible_sets = structures.registry.possible_structure_sets(
            &structure_state,
            &generator.biome_source().possible_biomes(),
        )?;
        if let ChunkGenerator::Flat(flat) = generator.as_ref()
            && let Some(overrides) = &flat.settings.structure_overrides
        {
            possible_sets.retain(|id| overrides.contains(id));
        }
        if matches!(generator.as_ref(), ChunkGenerator::Debug(_)) || !generate_structures {
            possible_sets.clear();
        }
        for id in &possible_sets {
            if let StructurePlacement::ConcentricRings(rings) = &structure_state.sets[id].placement
            {
                let ring_seed = if matches!(generator.as_ref(), ChunkGenerator::Flat(_)) {
                    0
                } else {
                    seed
                };
                let positions = rings.generate_positions(
                    ring_seed,
                    |position, biomes, random| match generator.as_ref() {
                        ChunkGenerator::Noise(noise) => {
                            find_ring_biome(noise, position, biomes, random)
                        }
                        ChunkGenerator::Flat(flat) => {
                            let preferred = match biomes {
                                BiomeSet::Direct(ids) => ids.as_slice(),
                                BiomeSet::Tag(tag) => generator
                                    .data()
                                    .biome_tags
                                    .get(tag)
                                    .ok_or_else(|| {
                                        Error(format!("missing preferred ring biome tag {tag}"))
                                    })?
                                    .as_slice(),
                            };
                            Ok(preferred.contains(&flat.settings.biome).then(|| {
                                position.map(|chunk| {
                                    (chunk.wrapping_mul(16).wrapping_add(8) - 112
                                        + random.next_int_bound(225))
                                        >> 4
                                })
                            }))
                        }
                        ChunkGenerator::Debug(_) => Ok(None),
                    },
                )?;
                structure_state.rings.push((rings.clone(), positions));
            }
        }
        let decoration =
            DecorationPlan::new(&features, &generator.biome_source().possible_biomes())?;
        Ok(Self {
            registry_context: None,
            generator,
            dimension,
            seed,
            resources: resources.clone(),
            features,
            structures,
            templates,
            pools,
            structure_state,
            possible_sets,
            structure_steps: StructureStep::load(resources)?,
            tags: BlockTags::load(resources)?,
            decoration,
            generate_structures,
            environment,
            dimension_environment,
            unported_behavior: crate::block::UnportedBehavior::Strict,
        })
    }
    /// The synchronized registry owner is shared with LevelContext, including
    /// initial spawn generation before a live level has been constructed.
    pub fn with_registry_context(
        mut self,
        registry: Arc<dyn ferraloom_protocol::codec::RegistryContext>,
    ) -> Self {
        self.registry_context = Some(registry);
        self
    }
    /// EMPTY owns empty sections. Flat's layer materialization is deferred until
    /// TERRAIN even though its biome palette can be created without noise.
    pub fn empty_chunk(&self, position: [i32; 2]) -> Result<RegionChunk, Error> {
        let generated = match self.generator.as_ref() {
            ChunkGenerator::Noise(noise) => {
                noise.new_chunk_in_dimension(position, self.dimension.clone())
            }
            ChunkGenerator::Debug(debug) => debug.build_terrain(position, self.dimension.clone()),
            ChunkGenerator::Flat(flat) => {
                flat.new_chunk_in_dimension(position, self.dimension.clone())
            }
        };
        Ok(RegionChunk::new(generated, ChunkStatus::Empty))
    }
    pub fn region<'a>(
        &'a self,
        chunks: &'a mut [RegionChunk],
        center: [i32; 2],
        step: &'a ChunkStep,
    ) -> Result<WorldGenRegion<'a>, Error> {
        let mut region = WorldGenRegion::new(
            chunks,
            center,
            step.direct.by_radius(),
            step.block_state_write_radius,
            None,
            &GameWorldgenBlockBehavior,
            self.seed,
            self.generator.sea_level(),
        )?;
        if let Some(context) = self.registry_context.as_deref() {
            region.set_registry_context(context);
        }
        region.set_skip_unsupported(self.unported_behavior == crate::block::UnportedBehavior::Warn);
        region.set_has_skylight(self.dimension.has_skylight);
        region.set_fast_lava(self.dimension.fast_lava);
        region.set_block_tags(self.generator.data().block_tags.clone());
        region.set_fluid_tags(self.generator.data().fluid_tags.clone());
        region.set_environment_attributes(
            self.environment.clone(),
            &self.dimension_environment,
            &self.generator.data().biome_names,
        )?;
        region.set_random(self.generator.region_random(center, self.seed));
        Ok(region)
    }
    /// Port of ChunkStatusTasks through FEATURES. Light executes on its own
    /// executor; FULL performs game-thread attachment. Neither is a passthrough.
    pub fn apply(
        &self,
        chunks: &mut [RegionChunk],
        center: [i32; 2],
        step: &ChunkStep,
    ) -> Result<Vec<(String, String)>, Error> {
        self.apply_step(chunks, center, step, None)
    }
    /// Port of WorldGenContext's shared ThreadedLevelLightEngine for proto
    /// writes. The caller lends its existing engine and flushes it before any
    /// affected proto save/FULL transport, including after a partial-write error.
    pub fn apply_with_light(
        &self,
        chunks: &mut [RegionChunk],
        center: [i32; 2],
        step: &ChunkStep,
        engine: &mut dyn ProtoChunkLightEngine,
    ) -> Result<Vec<(String, String)>, Error> {
        self.apply_step(chunks, center, step, Some(engine))
    }
    fn apply_step(
        &self,
        chunks: &mut [RegionChunk],
        center: [i32; 2],
        step: &ChunkStep,
        engine: Option<&mut dyn ProtoChunkLightEngine>,
    ) -> Result<Vec<(String, String)>, Error> {
        let index = chunks
            .iter()
            .position(|c| c.chunk.position == center)
            .ok_or_else(|| Error("generation center missing".into()))?;
        let mut skipped = Vec::new();
        match step.task {
            ChunkStatusTask::PassThrough | ChunkStatusTask::LoadStructureStarts => {}
            ChunkStatusTask::GenerateStructureStarts => {
                let mut sampler = SamplerContext::cached();
                let mut search = SearchContext::default();
                let mut biome = |quart| {
                    Ok(match self.generator.as_ref() {
                        ChunkGenerator::Noise(noise) => noise.source.sample(
                            &noise.random.compiler,
                            noise.random.router,
                            &mut sampler,
                            &mut search,
                            quart,
                        ),
                        ChunkGenerator::Flat(flat) => flat.settings.biome.clone(),
                        ChunkGenerator::Debug(_) => "plains".parse()?,
                    })
                };
                pipeline::create_structures(
                    &self.structure_state,
                    &self.possible_sets,
                    &mut chunks[index].structures,
                    &mut |id, references| {
                        self.structures.generate(
                            id,
                            references,
                            &mut StructureGenerationContext {
                                seed: self.seed,
                                chunk: center,
                                min_y: self.dimension.min_y,
                                height: self.dimension.height,
                                generator: self.generator.as_ref(),
                                templates: &self.templates,
                                pools: &self.pools,
                                biome: &mut biome,
                            },
                        )
                    },
                )?;
            }
            ChunkStatusTask::GenerateStructureReferences => {
                let mut references = std::mem::take(&mut chunks[index].structures.references);
                let readable: &[RegionChunk] = chunks;
                pipeline::create_references(center, &mut references, &mut |position| {
                    readable
                        .iter()
                        .find(|c| {
                            c.chunk.position == position && c.status >= ChunkStatus::StructureStarts
                        })
                        .map(|c| &c.structures.starts)
                        .ok_or_else(|| Error(format!("missing structure starts at {position:?}")))
                })?;
                chunks[index].structures.references = references;
            }
            ChunkStatusTask::GenerateBiomes => {
                if let ChunkGenerator::Noise(noise) = self.generator.as_ref() {
                    noise.create_biomes(&mut chunks[index].chunk);
                }
            }
            ChunkStatusTask::BuildTerrain => {
                let region = self.region(chunks, center, step)?;
                let order = self
                    .structure_steps
                    .iter()
                    .map(|s| s.id.clone())
                    .collect::<Vec<_>>();
                let beardifier = Beardifier::from_region(&region, &order)?;
                let mut generated = match self.generator.as_ref() {
                    ChunkGenerator::Noise(noise) => {
                        let mut generated = chunks[index].chunk.clone();
                        let mut noise_chunk = noise.noise_chunk(&generated);
                        noise_chunk
                            .samplers
                            .set_beardifier(Some(Arc::new(beardifier)));
                        noise.fill_from_noise(&mut generated, &mut noise_chunk);
                        noise.build_surface(&mut generated, &mut noise_chunk);
                        noise.generate_carvers(&mut generated, &mut noise_chunk, false)?;
                        generated
                    }
                    _ => self.generator.build_terrain(
                        center,
                        self.dimension.clone(),
                        Some(Arc::new(beardifier)),
                    )?,
                };
                for (section, previous) in generated
                    .sections
                    .iter_mut()
                    .zip(&chunks[index].chunk.sections)
                {
                    *section.biomes_mut() = previous.biomes().clone();
                }
                let previous = std::mem::replace(
                    &mut chunks[index],
                    RegionChunk::new(generated, ChunkStatus::Terrain),
                );
                chunks[index].structures = previous.structures;
                chunks[index].block_entities = previous.block_entities;
                chunks[index].instantiated_block_entities = previous.instantiated_block_entities;
                chunks[index].entities = previous.entities;
                chunks[index].block_ticks = previous.block_ticks;
                chunks[index].fluid_ticks = previous.fluid_ticks;
                chunks[index].poi_changes = previous.poi_changes;
            }
            ChunkStatusTask::GenerateFeatures => {
                let mut region = self.region(chunks, center, step)?;
                if let Some(engine) = engine {
                    region.set_light_engine(engine);
                }
                match self.generator.as_ref() {
                    ChunkGenerator::Debug(debug) => debug.apply_biome_decoration(&mut region)?,
                    _ => self.decoration.apply_biome_decoration(
                        &mut region,
                        self.generator.as_ref(),
                        &self.features,
                        &mut RegionStructureDecoration::new(
                            if self.generate_structures {
                                &self.structure_steps
                            } else {
                                &[]
                            },
                            &self.templates,
                            &self.tags,
                            self.generator.as_ref(),
                            &self.features,
                        ),
                    )?,
                }
                skipped = region.take_skipped_units();
            }
            ChunkStatusTask::GenerateSpawn => {
                return Err(Error(
                    "Pending: NaturalSpawner#spawnMobsForChunkGeneration".into(),
                ));
            }
            ChunkStatusTask::InitializeLight | ChunkStatusTask::Light | ChunkStatusTask::Full => {
                return Err(Error("status requires its runtime executor".into()));
            }
        }
        chunks[index].status = chunks[index].status.max(step.target);
        Ok(skipped)
    }
    pub fn restore_structures(
        &self,
        position: [i32; 2],
        tag: &ferraloom_nbt::CompoundTag,
    ) -> Result<pipeline::ChunkStructures, Error> {
        let mut loader = FeatureLoader::new(&self.resources)?;
        pipeline::ChunkStructures::load_from_nbt(
            position,
            self.seed,
            tag,
            &self.structures.registry,
            &self.templates,
            &self.resources,
            &mut |v| loader.placed(v),
        )
    }
}
