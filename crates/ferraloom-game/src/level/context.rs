//! Immutable dimension services known for the entire lifetime of a live Level.
use crate::entity::damage::{DamageSources, DamageTypeRegistry};
use ferraloom_protocol::play::Difficulty;
use ferraloom_worldgen::{feature::FeatureRegistry, generator::ChunkGenerator};
use std::sync::Arc;

/// Port of ServerLevel's WorldOptions seed and ServerChunkCache generator access.
/// Construction requires the actual dimension services, including in tests.
#[derive(Clone)]
pub struct LevelContext {
    pub seed: i64,
    pub sea_level: i32,
    pub generator: Arc<ChunkGenerator>,
    pub features: Arc<FeatureRegistry>,
    pub structures: Arc<super::StructureServices>,
    pub initial_difficulty: Difficulty,
    damage_types: Arc<DamageTypeRegistry>,
    block_transformers: Option<Arc<crate::item::block_transformer::BlockTransformerRegistry>>,
    registry_context: Option<Arc<dyn ferraloom_protocol::codec::RegistryContext>>,
}

impl LevelContext {
    pub fn new(
        seed: i64,
        sea_level: i32,
        generator: Arc<ChunkGenerator>,
        features: Arc<FeatureRegistry>,
        structures: Arc<super::StructureServices>,
        damage_types: Arc<DamageTypeRegistry>,
        initial_difficulty: Difficulty,
    ) -> Self {
        Self {
            seed,
            sea_level,
            generator,
            features,
            structures,
            damage_types,
            initial_difficulty,
            block_transformers: None,
            registry_context: None,
        }
    }

    /// Install the actual merged BlockTransformer registry without changing
    /// existing LevelContext constructor call sites.
    pub fn with_block_transformers(
        mut self,
        registry: Arc<crate::item::block_transformer::BlockTransformerRegistry>,
    ) -> Self {
        self.block_transformers = Some(registry);
        self
    }

    pub fn block_transformers(
        &self,
    ) -> Result<&Arc<crate::item::block_transformer::BlockTransformerRegistry>, super::LevelError>
    {
        self.block_transformers
            .as_ref()
            .ok_or(super::LevelError::MissingBlockTransformers)
    }

    /// Port of ServerLevel#registryAccess: the same immutable synchronized
    /// registry owner used by packet encoding.
    pub fn with_registry_context(
        mut self,
        context: Arc<dyn ferraloom_protocol::codec::RegistryContext>,
    ) -> Self {
        self.registry_context = Some(context);
        self
    }
    /// Required for native packets carrying a dynamic registry number.
    pub fn registry_context(
        &self,
    ) -> Result<&dyn ferraloom_protocol::codec::RegistryContext, super::LevelError> {
        self.registry_context
            .as_deref()
            .ok_or(super::LevelError::MissingRegistryContext)
    }

    pub fn damage_types(&self) -> &Arc<DamageTypeRegistry> {
        &self.damage_types
    }

    pub fn damage_sources(&self) -> &DamageSources {
        self.damage_types.sources()
    }

    /// Explicit controlled-level context. Immutable vanilla flat-generator
    /// services are loaded once; each fixture supplies its native seed/sea level
    /// and starts at the controlled LevelSettings default difficulty, Normal.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn fixture(seed: i64, sea_level: i32) -> Arc<Self> {
        use ferraloom_data::datapack::{Pack, ResourceManager};
        use ferraloom_worldgen::generator::{
            GenerationData,
            flat::{FlatLevelGeneratorSettings, FlatLevelSource},
        };
        use std::{path::Path, sync::OnceLock};
        struct Services {
            generator: Arc<ChunkGenerator>,
            features: Arc<FeatureRegistry>,
            structures: Arc<super::StructureServices>,
            damage_types: Arc<DamageTypeRegistry>,
            block_transformers: Arc<crate::item::block_transformer::BlockTransformerRegistry>,
            registry_context: Arc<dyn ferraloom_protocol::codec::RegistryContext>,
        }
        static SERVICES: OnceLock<Services> = OnceLock::new();
        let services = SERVICES.get_or_init(|| {
            let jar = std::env::var_os("FERRALOOM_VANILLA_JAR")
                .expect("builder supplies the vanilla datapack for level fixtures");
            let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
            let data = Arc::new(GenerationData::load(&resources).unwrap());
            let structures = Arc::new(super::StructureServices::load(&resources, &data).unwrap());
            let flat =
                FlatLevelSource::new(data, FlatLevelGeneratorSettings::default_settings()).unwrap();
            Services {
                generator: Arc::new(ChunkGenerator::Flat(Box::new(flat))),
                features: Arc::new(FeatureRegistry::default()),
                structures,
                damage_types: Arc::new(DamageTypeRegistry::load(&resources).unwrap()),
                block_transformers: Arc::new(
                    crate::item::block_transformer::BlockTransformerRegistry::load(&resources)
                        .unwrap(),
                ),
                registry_context: Arc::new(FixtureRegistryContext(resources)),
            }
        });
        Arc::new(
            Self::new(
                seed,
                sea_level,
                services.generator.clone(),
                services.features.clone(),
                services.structures.clone(),
                services.damage_types.clone(),
                Difficulty::Normal,
            )
            .with_block_transformers(services.block_transformers.clone())
            .with_registry_context(services.registry_context.clone()),
        )
    }
}

/// Controlled fixtures resolve the actual vanilla resource/registry snapshot.
/// Production installs the server's existing RegistrySet; this is test-only.
#[cfg(any(test, feature = "test-fixtures"))]
struct FixtureRegistryContext(ferraloom_data::datapack::ResourceManager);
#[cfg(any(test, feature = "test-fixtures"))]
impl ferraloom_protocol::codec::RegistryContext for FixtureRegistryContext {
    fn entry_id(
        &self,
        registry: &ferraloom_protocol::Identifier,
        entry: &ferraloom_protocol::Identifier,
    ) -> Option<i32> {
        if let Some(entries) = ferraloom_data::registry_entries(registry.as_str()) {
            entries
                .iter()
                .position(|name| *name == entry.as_str())
                .map(|id| id as i32)
        } else {
            self.0
                .registry(registry.path())
                .keys()
                .position(|name| name.to_string() == entry.as_str())
                .map(|id| id as i32)
        }
    }
    fn entry_key(
        &self,
        registry: &ferraloom_protocol::Identifier,
        id: i32,
    ) -> Option<ferraloom_protocol::Identifier> {
        let index = usize::try_from(id).ok()?;
        if let Some(entries) = ferraloom_data::registry_entries(registry.as_str()) {
            entries.get(index)?.parse().ok()
        } else {
            self.0
                .registry(registry.path())
                .keys()
                .nth(index)?
                .to_string()
                .parse()
                .ok()
        }
    }
    fn has_tag(
        &self,
        registry: &ferraloom_protocol::Identifier,
        tag: &ferraloom_protocol::Identifier,
    ) -> bool {
        self.0
            .registry(&format!("tags/{}", registry.path()))
            .keys()
            .any(|name| name.to_string() == tag.as_str())
    }
}
