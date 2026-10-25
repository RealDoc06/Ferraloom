//! MinecraftServer's sole LevelStorageAccess, WorldGenSettings and PrimaryLevelData.
use super::*;
use ferraloom_protocol::Identifier;
use ferraloom_worldgen::generator::LevelStem;

/// Immutable services shared by every dimension, never a copy of mutable world data.
pub struct WorldServices {
    pub access: Arc<LevelStorageAccess>,
    pub seed: i64,
    pub dimensions: WorldDimensions,
    pub registry: Arc<RegistryAdapter>,
    pub(super) resources: Arc<ResourceManager>,
    pub(super) generate_structures: bool,
    pub(super) bonus_chest: bool,
    pub(super) unported_behavior: ferraloom_game::block::UnportedBehavior,
    pub(super) generation: Arc<JobPool>,
    pub(super) light_pool: Arc<JobPool>,
    pub(super) saving: Arc<JobPool>,
}
impl WorldServices {
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
        registries: &RegistrySet,
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
                data,
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
                data,
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
                registry: Arc::new(RegistryAdapter::new(registries)?),
                resources: Arc::new(resources.clone()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_game::{Behaviors, level::Region};
    use ferraloom_util::ChunkPos;

    #[tokio::test]
    async fn dimensions_share_one_root_and_keep_their_actual_generators_and_io() {
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
        let resources = ResourceManager::new(vec![
            ferraloom_data::datapack::Pack::vanilla_jar(Path::new(&jar)).unwrap(),
        ]);
        let registries = RegistrySet::load(&resources).unwrap();
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
        let registries = RegistrySet::load(&resources).unwrap();
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
