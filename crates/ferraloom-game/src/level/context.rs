//! Immutable dimension services known for the entire lifetime of a live Level.
use crate::entity::damage::{DamageSources, DamageTypeRegistry};
use ferraloom_protocol::play::Difficulty;
use ferraloom_worldgen::{feature::FeatureRegistry, generator::ChunkGenerator};
use std::sync::Arc;

/// Port of ServerLevel's WorldOptions seed and ServerChunkCache generator access.
/// Construction requires the actual dimension services, including in tests.
pub struct LevelContext {
    pub seed: i64,
    pub sea_level: i32,
    pub generator: Arc<ChunkGenerator>,
    pub features: Arc<FeatureRegistry>,
    pub structures: Arc<super::StructureServices>,
    pub initial_difficulty: Difficulty,
    damage_types: Arc<DamageTypeRegistry>,
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
        }
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
            }
        });
        Arc::new(Self::new(
            seed,
            sea_level,
            services.generator.clone(),
            services.features.clone(),
            services.structures.clone(),
            services.damage_types.clone(),
            Difficulty::Normal,
        ))
    }
}
