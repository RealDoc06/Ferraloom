//! Immutable dimension services known for the entire lifetime of a live Level.
use ferraloom_worldgen::{feature::FeatureRegistry, generator::ChunkGenerator};
use std::sync::Arc;

/// Port of ServerLevel's WorldOptions seed and ServerChunkCache generator access.
/// Construction requires the actual dimension services, including in tests.
pub struct LevelContext {
    pub seed: i64,
    pub sea_level: i32,
    pub generator: Arc<ChunkGenerator>,
    pub features: Arc<FeatureRegistry>,
}

impl LevelContext {
    pub fn new(
        seed: i64,
        sea_level: i32,
        generator: Arc<ChunkGenerator>,
        features: Arc<FeatureRegistry>,
    ) -> Self {
        Self {
            seed,
            sea_level,
            generator,
            features,
        }
    }

    /// Explicit controlled-level context. Immutable vanilla flat-generator
    /// services are loaded once; each fixture supplies its native seed/sea level.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn fixture(seed: i64, sea_level: i32) -> Arc<Self> {
        use ferraloom_data::datapack::{Pack, ResourceManager};
        use ferraloom_worldgen::generator::{
            GenerationData,
            flat::{FlatLevelGeneratorSettings, FlatLevelSource},
        };
        use std::{path::Path, sync::OnceLock};
        static SERVICES: OnceLock<(Arc<ChunkGenerator>, Arc<FeatureRegistry>)> = OnceLock::new();
        let (generator, features) = SERVICES.get_or_init(|| {
            let jar = std::env::var_os("FERRALOOM_VANILLA_JAR")
                .expect("builder supplies the vanilla datapack for level fixtures");
            let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
            let data = Arc::new(GenerationData::load(&resources).unwrap());
            let flat =
                FlatLevelSource::new(data, FlatLevelGeneratorSettings::default_settings()).unwrap();
            (
                Arc::new(ChunkGenerator::Flat(Box::new(flat))),
                Arc::new(FeatureRegistry::default()),
            )
        });
        Arc::new(Self::new(
            seed,
            sea_level,
            generator.clone(),
            features.clone(),
        ))
    }
}
