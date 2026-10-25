//! Server-owned worldgen definitions, independent of dimension and seed.
use ferraloom_data::datapack::ResourceManager;
use ferraloom_worldgen::{
    Error,
    feature::{FeatureLoader, FeatureRegistry},
    generator::GenerationData,
    structure::{
        decoration::StructureStep, generation::StructureGenerators, pipeline::StructureRegistry,
        pool::StructurePools, rule::BlockTags, template::StructureTemplateManager,
    },
};

/// RegistryAccess and MinecraftServer's StructureTemplateManager are shared by
/// every level. Placement state, samplers and per-chunk structures live elsewhere.
pub struct WorldGenRegistries {
    pub resources: ResourceManager,
    pub features: FeatureRegistry,
    pub structures: StructureGenerators,
    pub templates: StructureTemplateManager,
    pub pools: StructurePools,
    pub structure_steps: Vec<StructureStep>,
    pub tags: BlockTags,
}
impl WorldGenRegistries {
    pub fn load(resources: &ResourceManager, data: &GenerationData) -> Result<Self, Error> {
        let mut loader = FeatureLoader::new(resources)?;
        let registry = StructureRegistry::load(resources, data)?;
        let structures = StructureGenerators::load(resources, registry, &mut |v| loader.placed(v))?;
        let pools = StructurePools::load(resources, &mut |v| loader.placed(v))?;
        Ok(Self {
            resources: resources.clone(),
            features: loader.load_all()?,
            structures,
            templates: StructureTemplateManager::load_all(resources)?,
            pools,
            structure_steps: StructureStep::load(resources)?,
            tags: BlockTags::load(resources)?,
        })
    }
}
