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
    dimension_type: Option<ferraloom_worldgen::settings::DimensionType>,
    damage_types: Arc<DamageTypeRegistry>,
    block_transformers: Option<Arc<crate::item::block_transformer::BlockTransformerRegistry>>,
    enchantments: Option<Arc<crate::item::enchantment::EnchantmentRegistry>>,
    mob_variants: Option<Arc<crate::entity::variant::MobVariants>>,
    registry_context: Option<Arc<dyn ferraloom_protocol::codec::RegistryContext>>,
    game_event_tags:
        Option<Arc<ferraloom_data::datapack::TagSet<ferraloom_data::registries::game_event::Id>>>,
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
            dimension_type: None,
            block_transformers: None,
            enchantments: None,
            mob_variants: None,
            registry_context: None,
            game_event_tags: None,
        }
    }

    /// Install the decoded active LevelStem's DimensionType, including inline types.
    pub fn with_dimension_type(
        mut self,
        dimension: ferraloom_worldgen::settings::DimensionType,
    ) -> Self {
        self.dimension_type = Some(dimension);
        self
    }

    /// Port of Level#dimensionType. An uninstalled service is a required error.
    pub fn dimension_type(
        &self,
    ) -> Result<&ferraloom_worldgen::settings::DimensionType, super::LevelError> {
        self.dimension_type
            .as_ref()
            .ok_or(super::LevelError::MissingDimensionType)
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

    /// Attach the actual loaded registry, shared with every level in this world.
    pub fn with_enchantments(
        mut self,
        registry: Arc<crate::item::enchantment::EnchantmentRegistry>,
    ) -> Self {
        self.enchantments = Some(registry);
        self
    }
    pub fn enchantments(
        &self,
    ) -> Result<&Arc<crate::item::enchantment::EnchantmentRegistry>, super::LevelError> {
        self.enchantments
            .as_ref()
            .ok_or(super::LevelError::MissingEnchantments)
    }
    /// Install the merged immutable spawn-conditioned registries once per load.
    pub fn with_mob_variants(mut self, variants: Arc<crate::entity::variant::MobVariants>) -> Self {
        self.mob_variants = Some(variants);
        self
    }
    pub fn mob_variants(
        &self,
    ) -> Result<&Arc<crate::entity::variant::MobVariants>, super::LevelError> {
        self.mob_variants
            .as_ref()
            .ok_or(super::LevelError::MissingMobVariants)
    }
    /// Install the actual merged GameEventTags used by vibration listeners.
    pub fn with_game_event_tags(
        mut self,
        tags: Arc<ferraloom_data::datapack::TagSet<ferraloom_data::registries::game_event::Id>>,
    ) -> Self {
        self.game_event_tags = Some(tags);
        self
    }

    pub fn game_event_tags(
        &self,
    ) -> Result<
        &ferraloom_data::datapack::TagSet<ferraloom_data::registries::game_event::Id>,
        super::LevelError,
    > {
        self.game_event_tags
            .as_deref()
            .ok_or(super::LevelError::MissingGameEventTags)
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
            dimension_type: ferraloom_worldgen::settings::DimensionType,
            generator: Arc<ChunkGenerator>,
            features: Arc<FeatureRegistry>,
            structures: Arc<super::StructureServices>,
            damage_types: Arc<DamageTypeRegistry>,
            block_transformers: Arc<crate::item::block_transformer::BlockTransformerRegistry>,
            enchantments: Arc<crate::item::enchantment::EnchantmentRegistry>,
            mob_variants: Arc<crate::entity::variant::MobVariants>,
            registry_context: Arc<dyn ferraloom_protocol::codec::RegistryContext>,
            game_event_tags:
                Arc<ferraloom_data::datapack::TagSet<ferraloom_data::registries::game_event::Id>>,
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
                dimension_type: ferraloom_worldgen::settings::DimensionType::decode(
                    &resources
                        .require(&"minecraft:dimension_type/overworld.json".parse().unwrap())
                        .unwrap()
                        .json()
                        .unwrap(),
                )
                .unwrap(),
                generator: Arc::new(ChunkGenerator::Flat(Box::new(flat))),
                features: Arc::new(FeatureRegistry::default()),
                structures,
                damage_types: Arc::new(DamageTypeRegistry::load(&resources).unwrap()),
                block_transformers: Arc::new(
                    crate::item::block_transformer::BlockTransformerRegistry::load(&resources)
                        .unwrap(),
                ),
                enchantments: Arc::new(
                    crate::item::enchantment::EnchantmentRegistry::load(&resources).unwrap(),
                ),
                mob_variants: Arc::new(
                    crate::entity::variant::MobVariants::load(&resources).unwrap(),
                ),
                game_event_tags: Arc::new(
                    ferraloom_data::datapack::TagSet::load(&resources).unwrap(),
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
            .with_dimension_type(services.dimension_type.clone())
            .with_block_transformers(services.block_transformers.clone())
            .with_enchantments(services.enchantments.clone())
            .with_mob_variants(services.mob_variants.clone())
            .with_registry_context(services.registry_context.clone())
            .with_game_event_tags(services.game_event_tags.clone()),
        )
    }
}

#[cfg(test)]
#[path = "dimension_tests.rs"]
mod tests;

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
