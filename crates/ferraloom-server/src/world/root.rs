//! MinecraftServer's sole LevelStorageAccess, WorldGenSettings and PrimaryLevelData.
use super::*;
use ferraloom_protocol::Identifier;
use ferraloom_worldgen::generator::LevelStem;

/// Immutable services shared by every dimension, never a copy of mutable world data.
pub struct WorldServices {
    pub access: Arc<LevelStorageAccess>,
    pub seed: i64,
    pub dimensions: WorldDimensions,
    dimension_types: std::collections::BTreeMap<Identifier, Option<Identifier>>,
    pub registry: Arc<RegistryAdapter>,
    pub registry_context: Arc<RegistrySet>,
    pub(super) worldgen: Arc<ferraloom_game::level::generation::WorldGenRegistries>,
    pub(super) generate_structures: bool,
    pub(super) bonus_chest: bool,
    pub(super) unported_behavior: ferraloom_game::block::UnportedBehavior,
    pub(super) generation: Arc<JobPool>,
    pub(super) light_pool: Arc<JobPool>,
    pub(super) saving: Arc<JobPool>,
}
impl WorldServices {
    /// Immutable worldgen definitions shared by generation and live placement.
    pub(crate) fn worldgen(&self) -> &Arc<ferraloom_game::level::generation::WorldGenRegistries> {
        &self.worldgen
    }

    /// Port of WorldDimensions#specialWorldProperty: debug/flat are global
    /// WorldData flags, including when the player's current level is not OW.
    pub fn is_debug_world(&self) -> bool {
        matches!(
            self.dimensions.overworld().generator.as_ref(),
            ChunkGenerator::Debug(_)
        )
    }
    pub fn is_flat_world(&self) -> bool {
        matches!(
            self.dimensions.overworld().generator.as_ref(),
            ChunkGenerator::Flat(_)
        )
    }
    /// CommonPlayerSpawnInfo uses the original DimensionType holder key, not
    /// the dimension key or a structurally equal decoded value.
    pub fn dimension_type_key(&self, dimension: &Identifier) -> Result<&Identifier> {
        self.stem(dimension)?;
        self.dimension_types
            .get(dimension)
            .context("registered dimension is missing its type holder")?
            .as_ref()
            .with_context(|| {
                format!("Unsupported: inline DimensionType network holder for {dimension}")
            })
    }
    pub fn stem(&self, dimension: &Identifier) -> Result<&LevelStem> {
        self.dimensions
            .dimensions
            .iter()
            .find(|stem| {
                stem.name.namespace() == dimension.namespace()
                    && stem.name.path() == dimension.path()
            })
            .with_context(|| format!("Unknown dimension {dimension}"))
    }
}

/// Owned constructor output, consumed exactly once before initial level publication.
pub(super) struct StartupSpawn {
    pub position: BlockPos,
}

/// One owner per server. Dimension runtimes share only immutable services.
pub struct WorldRoot {
    pub services: Arc<WorldServices>,
    pub level_data: LevelData,
}
impl std::ops::Deref for WorldRoot {
    type Target = WorldServices;
    fn deref(&self) -> &Self::Target {
        &self.services
    }
}
impl WorldRoot {
    pub fn open(
        root: &Path,
        properties: &ServerProperties,
        resources: &ResourceManager,
        registries: &Arc<RegistrySet>,
        unported_behavior: ferraloom_game::block::UnportedBehavior,
    ) -> Result<Self> {
        let access = Arc::new(LevelStorageAccess::open(
            root.join(properties.get("level-name")),
        )?);
        let saved_data = SavedDataStorage::new(access.path().join("data"));
        let settings = saved_data.load::<CompoundTag>("minecraft:world_gen_settings")?;
        let seed = settings
            .as_ref()
            .map_or_else(|| properties.seed(), |s| s.value().get_long_or("seed", 0));
        let bonus_chest = settings
            .as_ref()
            .is_some_and(|settings| settings.value().get_boolean_or("bonus_chest", false));
        // WorldOptions.CODEC defaults a missing saved field to true. Server
        // properties supply the value only when creating a world.
        let generate_structures = settings.as_ref().map_or_else(
            || properties.boolean("generate-structures"),
            |settings| settings.value().get_boolean_or("generate_structures", true),
        );
        let data = Arc::new(GenerationData::load(resources)?);
        ensure!(
            data.biome_names
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                == registries.entries["minecraft:worldgen/biome"],
            "worldgen biome ids differ from synchronized registry"
        );
        let preset = if let Some(settings) = &settings {
            WorldPreset::decode(
                &nbt_json(&Tag::Compound(settings.value().clone()))?,
                resources,
                data.clone(),
                seed,
            )?
        } else {
            let text = properties.get("generator-settings");
            // GsonHelper reads the first strict JSON object through its type
            // adapter without requiring EOF. Only an empty property means {}.
            let mut parser =
                serde_json::Deserializer::from_str(if text.is_empty() { "{}" } else { text });
            let generator_settings = serde_json::Value::deserialize(&mut parser)
                .context("invalid generator-settings JSON")?;
            let preset = WorldPreset::from_server_properties(
                properties.get("level-type"),
                &generator_settings,
                resources,
                data.clone(),
                seed,
            )?;
            let mut settings = json_nbt(&preset.definition)
                .as_compound()
                .cloned()
                .context("world preset compound")?;
            settings.insert("seed", Tag::Long(seed));
            settings.insert(
                "generate_structures",
                Tag::Byte(i8::from(generate_structures)),
            );
            settings.insert("bonus_chest", Tag::Byte(0));
            saved_data.save(
                "minecraft:world_gen_settings",
                &mut SavedData::new(settings),
            )?;
            preset
        };
        let dimension_types = dimension_type_keys(&preset.definition)?;
        let worldgen = {
            let _timing = crate::generation::profile::Span::new("worldgen_registries", None);
            Arc::new(
                ferraloom_game::level::generation::WorldGenRegistries::load_for_world(
                    resources,
                    &data,
                    access.path(),
                )?,
            )
        };
        let mut level_data = if access.path().join("level.dat").exists() {
            access.read_level_data()?
        } else {
            new_level_data(properties)
        };
        // DedicatedServer#forceDifficulty ignores the saved lock at startup.
        force_initial_difficulty(&mut level_data.data, properties.get("difficulty"));
        access.save_level_data(&level_data)?;
        Ok(Self {
            services: Arc::new(WorldServices {
                access,
                seed,
                dimensions: preset.dimensions,
                dimension_types,
                registry: Arc::new(RegistryAdapter::new(registries)?),
                registry_context: registries.clone(),
                worldgen,
                generate_structures,
                bonus_chest,
                unported_behavior,
                generation: Arc::new(JobPool::new("ferraloom-worldgen", 2)?),
                light_pool: Arc::new(JobPool::new("ferraloom-generation-light", 1)?),
                saving: Arc::new(JobPool::new("ferraloom-save", 1)?),
            }),
            level_data,
        })
    }
    /// Port of MinecraftServer#createLevels, using the selected registered stem.
    pub fn open_dimension(
        &mut self,
        dimension: Identifier,
        properties: &ServerProperties,
    ) -> Result<World> {
        let initialize_spawn = dimension.as_str() == "minecraft:overworld"
            && !self.level_data.data.contains_key("spawn");
        let (world, startup_spawn) = World::open_dimension(
            self.services.clone(),
            dimension,
            properties,
            initialize_spawn,
            self.difficulty(),
        )?;
        if let Some(spawn) = startup_spawn {
            self.level_data
                .data
                .insert("spawn", spawn_tag(spawn.position));
            self.level_data.data.insert("initialized", Tag::Byte(1));
            self.access.save_level_data(&self.level_data)?;
        }
        Ok(world)
    }
    pub fn difficulty(&self) -> Difficulty {
        read_difficulty_settings(&self.level_data.data).0
    }
    pub fn spawn_position(&self) -> BlockPos {
        self.level_data
            .data
            .get_compound("spawn")
            .and_then(|tag| tag.get_int_array("pos"))
            .filter(|v| v.len() == 3)
            .map_or(BlockPos::new(0, 80, 0), |v| BlockPos::new(v[0], v[1], v[2]))
    }
    /// Capture owned global metadata before IO; no live Level crosses an await.
    pub(crate) fn begin_save_level_and_players(
        &mut self,
        game_time: i64,
        players: Vec<(String, CompoundTag)>,
    ) -> oneshot::Receiver<Result<()>> {
        self.level_data.data.insert("Time", Tag::Long(game_time));
        self.level_data.data.insert(
            "LastPlayed",
            Tag::Long(
                time::OffsetDateTime::now_utc()
                    .unix_timestamp_nanos()
                    .div_euclid(1_000_000) as i64,
            ),
        );
        let access = self.access.clone();
        let data = self.level_data.clone();
        self.saving.submit(move || -> Result<()> {
            for (uuid, tag) in players {
                access.player_data().save(&uuid, &tag)?;
            }
            access.save_level_data(&data)?;
            Ok(())
        })
    }
    pub(crate) fn begin_save_weather(
        &self,
        weather: ferraloom_game::level::weather::WeatherData,
    ) -> oneshot::Receiver<Result<()>> {
        let path = self.access.path().join("data");
        self.saving.submit(move || -> Result<()> {
            SavedDataStorage::new(path).save("minecraft:weather", &mut SavedData::new(weather))?;
            Ok(())
        })
    }
}

// Retain holder identity while the raw preset and resolved stems are together.
// Inline holders remain representable here; the network boundary rejects them.
fn dimension_type_keys(
    definition: &serde_json::Value,
) -> Result<std::collections::BTreeMap<Identifier, Option<Identifier>>> {
    definition
        .get("dimensions")
        .and_then(serde_json::Value::as_object)
        .context("world preset dimension map")?
        .iter()
        .map(|(dimension, stem)| {
            let holder = stem.get("type").context("dimension type holder")?;
            let key = match holder.as_str() {
                Some(name) => Some(name.parse()?),
                None => {
                    ensure!(
                        holder.is_object(),
                        "dimension type holder is neither named nor inline"
                    );
                    None
                }
            };
            Ok((dimension.parse()?, key))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_game::{Behaviors, level::Region};
    use ferraloom_util::ChunkPos;

    #[test]
    fn dimension_type_holder_identity_is_independent_of_dimension_name() {
        let holders = dimension_type_keys(&serde_json::json!({"dimensions": {
            "fixture:moon": {"type": "minecraft:overworld"},
            "fixture:other": {"type": "fixture:custom_type"},
            "fixture:inline": {"type": {"min_y": 0, "height": 256}}
        }}))
        .unwrap();
        assert_eq!(
            holders[&"fixture:moon".parse().unwrap()]
                .as_ref()
                .unwrap()
                .as_str(),
            "minecraft:overworld"
        );
        assert_eq!(
            holders[&"fixture:other".parse().unwrap()]
                .as_ref()
                .unwrap()
                .as_str(),
            "fixture:custom_type"
        );
        assert!(holders[&"fixture:inline".parse().unwrap()].is_none());
        assert!(!holders.contains_key(&"minecraft:overworld".parse().unwrap()));
    }

    #[tokio::test]
    async fn native_world_flags_follow_overworld_data_in_every_dimension() {
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
        let resources = ResourceManager::new(vec![
            ferraloom_data::datapack::Pack::vanilla_jar(Path::new(&jar)).unwrap(),
        ]);
        let registries = Arc::new(RegistrySet::load(&resources).unwrap());
        let cases: Vec<_> = include_str!("../../tests/java/world_data/flags.tsv")
            .lines()
            .map(|row| row.split('\t').collect::<Vec<_>>())
            .collect();
        assert_eq!(cases.len(), 9);
        for preset in [
            "minecraft:normal",
            "minecraft:flat",
            "minecraft:debug_all_block_states",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let mut properties = ServerProperties::from_values(Default::default());
            properties.set("level-type", preset);
            properties.set("level-seed", 0);
            let mut root = WorldRoot::open(
                directory.path(),
                &properties,
                &resources,
                &registries,
                ferraloom_game::block::UnportedBehavior::Strict,
            )
            .unwrap();
            for fields in cases.iter().filter(|fields| fields[0] == preset) {
                assert_eq!(root.is_debug_world(), fields[2] == "true", "{fields:?}");
                assert_eq!(root.is_flat_world(), fields[3] == "true", "{fields:?}");
                assert_eq!(
                    root.is_flat_world(),
                    fields[4] == "true",
                    "native ServerLevel#isFlat: {fields:?}"
                );
            }
            let mut world = root
                .open_dimension("minecraft:the_nether".parse().unwrap(), &properties)
                .unwrap();
            let context = world.level_context(&resources).unwrap();
            let mut level = Level::new(Region::new(&Behaviors::EMPTY), context);
            world.configure_live_rules(&mut level);
            assert_eq!(level.rules().debug, root.is_debug_world());
            assert!(!world.dimension.has_skylight);
            world.close_chunk_storage().await.unwrap();
        }
    }

    #[tokio::test]
    async fn dimensions_share_one_root_and_keep_their_actual_generators_and_io() {
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
        let resources = ResourceManager::new(vec![
            ferraloom_data::datapack::Pack::vanilla_jar(Path::new(&jar)).unwrap(),
        ]);
        let registries = Arc::new(RegistrySet::load(&resources).unwrap());
        let directory = tempfile::tempdir().unwrap();
        let mut properties = ServerProperties::from_values(Default::default());
        properties.set("level-type", "minecraft:flat");
        properties.set("level-seed", 1234);
        let mut root = WorldRoot::open(
            directory.path(),
            &properties,
            &resources,
            &registries,
            ferraloom_game::block::UnportedBehavior::Strict,
        )
        .unwrap();
        let mut worlds = Vec::new();
        for key in [
            "minecraft:overworld",
            "minecraft:the_nether",
            "minecraft:the_end",
        ] {
            let before = root.level_data.clone();
            let mut world = root
                .open_dimension(key.parse().unwrap(), &properties)
                .unwrap();
            if key != "minecraft:overworld" {
                assert_eq!(root.level_data, before);
            }
            world
                .validate_dimension_binding(&key.parse().unwrap())
                .unwrap();
            assert!(
                world
                    .validate_dimension_binding(&"fixture:absent".parse().unwrap())
                    .is_err()
            );
            let generator = world.generator.clone();
            let wrong_stem = root
                .dimensions
                .dimensions
                .iter()
                .find(|stem| stem.name.to_string() != key)
                .unwrap();
            world.generator = wrong_stem.generator.clone();
            assert!(
                world
                    .validate_dimension_binding(&key.parse().unwrap())
                    .is_err()
            );
            world.generator = generator;
            let height = world.dimension.height;
            world.dimension.height += 16;
            assert!(
                world
                    .validate_dimension_binding(&key.parse().unwrap())
                    .is_err()
            );
            world.dimension.height = height;
            let saving = world.saving.clone();
            world.saving = Arc::new(JobPool::new("fixture-wrong-save-service", 1).unwrap());
            assert!(
                world
                    .validate_dimension_binding(&key.parse().unwrap())
                    .is_err()
            );
            world.saving = saving;
            world
                .validate_dimension_binding(&key.parse().unwrap())
                .unwrap();
            assert!(Arc::ptr_eq(&root.services, &world.services));
            assert!(Arc::ptr_eq(&root.access, &world.access));
            let stem = root.stem(&key.parse().unwrap()).unwrap();
            assert!(Arc::ptr_eq(&world.generator, &stem.generator));
            assert_eq!(
                world.build_height(),
                stem.dimension.min_y..stem.dimension.min_y + stem.dimension.height
            );
            let context = world.level_context(&resources).unwrap();
            assert_eq!(context.seed, 1234);
            assert_eq!(context.sea_level, stem.generator.sea_level());
            assert!(Arc::ptr_eq(&context.generator, &stem.generator));
            assert_eq!(
                context.dimension_type().unwrap().infiniburn,
                stem.dimension.infiniburn
            );
            static EMPTY: Behaviors = Behaviors::EMPTY;
            let mut level = Level::new(Region::new(&EMPTY), context);
            world.configure_live_rules(&mut level);
            assert_eq!(level.has_skylight().unwrap(), stem.dimension.has_skylight);
            let mut marker = CompoundTag::new();
            marker.insert("DataVersion", Tag::Int(storage::DATA_VERSION));
            marker.insert("dimension", Tag::String(key.into()));
            world.io.store(ChunkPos::ZERO, Some(marker)).await.unwrap();
            world.io.synchronize(true).await.unwrap();
            worlds.push(world);
        }
        let metadata = root.level_data.clone();
        for world in &mut worlds {
            let chunk = world.io.load(ChunkPos::ZERO).await.unwrap().unwrap();
            assert_eq!(
                chunk.get_string("dimension").unwrap().to_string_lossy(),
                world.dimension_key.as_str()
            );
            let expected = root
                .access
                .path()
                .join("dimensions")
                .join(world.dimension_key.namespace())
                .join(world.dimension_key.path());
            assert!(expected.join("region/r.0.0.mca").is_file());
            world.close_chunk_storage().await.unwrap();
        }
        assert_eq!(root.level_data, metadata);
        assert!(root.access.path().join("level.dat").is_file());
        assert!(
            !root
                .access
                .path()
                .join("dimensions/minecraft/the_nether/level.dat")
                .exists()
        );
        assert!(
            !root
                .access
                .path()
                .join("dimensions/minecraft/the_end/level.dat")
                .exists()
        );
        assert!(matches!(
            worlds[0].generator.as_ref(),
            ChunkGenerator::Flat(_)
        ));
        assert!(matches!(
            worlds[1].generator.as_ref(),
            ChunkGenerator::Noise(_)
        ));
        assert!(matches!(
            worlds[2].generator.as_ref(),
            ChunkGenerator::Noise(_)
        ));
    }

    #[test]
    fn unknown_dimension_does_not_fall_back_to_overworld_or_mutate_metadata() {
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
        let resources = ResourceManager::new(vec![
            ferraloom_data::datapack::Pack::vanilla_jar(Path::new(&jar)).unwrap(),
        ]);
        let registries = Arc::new(RegistrySet::load(&resources).unwrap());
        let directory = tempfile::tempdir().unwrap();
        let properties = ServerProperties::from_values(Default::default());
        let mut root = WorldRoot::open(
            directory.path(),
            &properties,
            &resources,
            &registries,
            ferraloom_game::block::UnportedBehavior::Strict,
        )
        .unwrap();
        let metadata = root.level_data.clone();
        assert!(
            root.open_dimension("fixture:absent".parse().unwrap(), &properties)
                .is_err()
        );
        assert_eq!(root.level_data, metadata);
        assert!(
            !root
                .access
                .path()
                .join("dimensions/fixture/absent")
                .exists()
        );
    }
}
