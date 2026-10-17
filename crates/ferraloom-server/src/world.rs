//! World snapshots cross worker boundaries; only tick integration mutates Level.
use crate::{jobs::JobPool, properties::ServerProperties, registries::RegistrySet};
use anyhow::{Context, Result, ensure};
use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
use ferraloom_data::{Block, BlockState, datapack::ResourceManager};
use ferraloom_game::level::chunk::LevelChunk;
use ferraloom_light::{LevelLightEngine, LightLayer, SectionSnapshots};
use ferraloom_nbt::{CompoundTag, ListTag, NbtString, Tag};
use ferraloom_storage::{
    self as storage, ByteArray, IoWorker, LevelData, LevelStorageAccess, LongArray, SavedData,
    SavedDataStorage, SectionRegistry, SerializableChunkData,
};
use ferraloom_util::{BlockPos, ChunkPos};
use ferraloom_worldgen::{
    biome::BiomeSource,
    generator::{GenerationData, NoiseBasedChunkGenerator},
    settings::NoiseGeneratorSettings,
};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    path::Path,
    sync::{Arc, mpsc},
};
use tokio::sync::oneshot;

pub struct RegistryAdapter {
    biomes: Vec<String>,
    strategy: Strategy<BiomeId>,
}
impl RegistryAdapter {
    pub fn new(registries: &RegistrySet) -> Result<Self> {
        let biomes = registries.entries["minecraft:worldgen/biome"].clone();
        let strategy = Strategy::biomes(Arc::new(
            IdMap::new((0..biomes.len()).map(|i| BiomeId(i as u32)).collect())
                .map_err(|e| anyhow::anyhow!("{e:?}"))?,
        ));
        Ok(Self { biomes, strategy })
    }
}
impl SectionRegistry for RegistryAdapter {
    fn block_state(&self, entry: &storage::BlockStatePaletteEntry) -> storage::Result<BlockState> {
        let block = entry
            .name
            .as_ref()
            .and_then(|name| Block::from_name(&name.to_string_lossy()))
            .ok_or_else(|| storage::Error::Invalid("unknown block in chunk palette".into()))?;
        let properties = entry
            .properties
            .as_ref()
            .into_iter()
            .flat_map(|p| p.iter())
            .map(|(k, v)| (k.to_string_lossy(), v.to_string_lossy()))
            .collect::<Vec<_>>();
        BlockState::from_properties(
            block,
            properties.iter().map(|(k, v)| (k.as_str(), v.as_str())),
        )
        .ok_or_else(|| storage::Error::Invalid("invalid block state".into()))
    }
    fn block_state_entry(
        &self,
        state: BlockState,
    ) -> storage::Result<storage::BlockStatePaletteEntry> {
        Ok(storage::BlockStatePaletteEntry {
            name: Some(state.block().name().into()),
            properties: Some(
                state
                    .properties()
                    .map(|(k, v)| (k.into(), v.into()))
                    .collect(),
            ),
            shorthand: state == state.block().default_state(),
            extra: CompoundTag::new(),
        })
    }
    fn biome(&self, name: &NbtString) -> storage::Result<BiomeId> {
        self.biomes
            .iter()
            .position(|s| s == &name.to_string_lossy())
            .map(|i| BiomeId(i as u32))
            .ok_or_else(|| storage::Error::Invalid(format!("unknown biome {name}")))
    }
    fn biome_name(&self, biome: BiomeId) -> storage::Result<NbtString> {
        self.biomes
            .get(biome.0 as usize)
            .map(|s| s.as_str().into())
            .ok_or_else(|| storage::Error::Invalid("unknown biome id".into()))
    }
    fn biome_strategy(&self) -> Strategy<BiomeId> {
        self.strategy.clone()
    }
    fn default_biome(&self) -> BiomeId {
        self.biome(&"minecraft:plains".into())
            .expect("plains biome")
    }
}

pub struct Terrain {
    pub pos: ChunkPos,
    pub sections: Vec<LevelChunkSection>,
    pub disk: SerializableChunkData,
    pub surface: [i32; 256],
}
#[derive(Clone, Default)]
pub struct ChunkLight {
    pub sky: BTreeMap<i32, Vec<u8>>,
    pub block: BTreeMap<i32, Vec<u8>>,
}
pub struct LitChunk {
    pub request: u64,
    pub pos: ChunkPos,
    pub updates: BTreeMap<i64, ChunkLight>,
}
struct LightJob {
    request: u64,
    pos: ChunkPos,
    sections: Vec<LevelChunkSection>,
}

pub struct World {
    pub access: Arc<LevelStorageAccess>,
    pub level_data: LevelData,
    pub seed: i64,
    pub spawn: BlockPos,
    pub registry: Arc<RegistryAdapter>,
    pub io: IoWorker,
    generator: Arc<NoiseBasedChunkGenerator>,
    generation: Arc<JobPool>,
    saving: JobPool,
    pub pending: BTreeMap<u64, oneshot::Receiver<Result<Terrain>>>,
    light_tx: mpsc::Sender<LightJob>,
    pub light_rx: tokio::sync::mpsc::UnboundedReceiver<LitChunk>,
    pub saved: BTreeMap<i64, SerializableChunkData>,
    pub lights: BTreeMap<i64, ChunkLight>,
    pub ready: std::collections::HashSet<ChunkPos>,
}
impl World {
    pub fn open(
        root: &Path,
        properties: &ServerProperties,
        resources: &ResourceManager,
        registries: &RegistrySet,
    ) -> Result<Self> {
        let access = Arc::new(LevelStorageAccess::open(
            root.join(properties.get("level-name")),
        )?);
        let saved_data = SavedDataStorage::new(access.path().join("data"));
        let settings = saved_data.load::<CompoundTag>("minecraft:world_gen_settings")?;
        let seed = settings
            .as_ref()
            .map_or_else(|| properties.seed(), |s| s.value().get_long_or("seed", 0));
        if settings.is_none() {
            let preset = resources
                .require(&"worldgen/world_preset/normal.json".parse()?)?
                .json()?;
            let mut settings = json_nbt(&preset)
                .as_compound()
                .cloned()
                .context("world preset compound")?;
            settings.insert("seed", Tag::Long(seed));
            settings.insert(
                "generate_features",
                Tag::Byte(i8::from(properties.boolean("generate-structures"))),
            );
            settings.insert("bonus_chest", Tag::Byte(0));
            saved_data.save(
                "minecraft:world_gen_settings",
                &mut SavedData::new(settings),
            )?;
        }
        let level_data = if access.path().join("level.dat").exists() {
            access.read_level_data()?
        } else {
            new_level_data(properties)
        };
        access.save_level_data(&level_data)?;
        let spawn = level_data
            .data
            .get_compound("spawn")
            .and_then(|t| t.get_int_array("pos"))
            .filter(|v| v.len() == 3)
            .map_or(BlockPos::new(0, 80, 0), |v| BlockPos::new(v[0], v[1], v[2]));
        let data = Arc::new(GenerationData::load(resources)?);
        ensure!(
            data.biome_names
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                == registries.entries["minecraft:worldgen/biome"],
            "worldgen biome ids differ from synchronized registry"
        );
        let noise = resources
            .require(&"worldgen/noise_settings/overworld.json".parse()?)?
            .json()?;
        let settings = NoiseGeneratorSettings::decode(&noise)?;
        let source = BiomeSource::decode(
            &serde_json::json!({"type":"minecraft:multi_noise","preset":"minecraft:overworld"}),
            resources,
        )?;
        let generator = Arc::new(NoiseBasedChunkGenerator::new(data, settings, source, seed)?);
        let version = match properties.get("region-file-compression") {
            "none" => storage::RegionFileVersion::None,
            "lz4" => storage::RegionFileVersion::Lz4,
            _ => storage::RegionFileVersion::Deflate,
        };
        let io = IoWorker::new(
            access.dimension("minecraft:overworld")?.region,
            version,
            properties.boolean("sync-chunk-writes"),
        )?;
        let registry = Arc::new(RegistryAdapter::new(registries)?);
        let (light_tx, light_in) = mpsc::channel::<LightJob>();
        let (light_out, light_rx) = tokio::sync::mpsc::unbounded_channel();
        std::thread::Builder::new()
            .name("ferraloom-light".into())
            .spawn(move || {
                let mut snapshots = SectionSnapshots::new(-4, 20);
                let mut engine = LevelLightEngine::new(true, true);
                let mut lights: BTreeMap<i64, ChunkLight> = BTreeMap::new();
                while let Ok(job) = light_in.recv() {
                    snapshots.insert_chunk(
                        job.pos,
                        job.sections
                            .into_iter()
                            .map(|s| Some(Cow::Owned(s)))
                            .collect(),
                    );
                    let initialize = engine.initialize_light(&snapshots, job.pos, false);
                    let update = engine.light_chunk(&snapshots, job.pos, false);
                    let mut affected = std::collections::BTreeSet::new();
                    affected.insert(job.pos.pack());
                    for change in initialize.sections.into_iter().chain(update.sections) {
                        let key = change.section.chunk().pack();
                        affected.insert(key);
                        let light = lights.entry(key).or_default();
                        let layers = match change.layer {
                            LightLayer::Sky => &mut light.sky,
                            LightLayer::Block => &mut light.block,
                        };
                        if let Some(data) = change.data {
                            layers.insert(change.section.y, data.to_bytes().to_vec());
                        } else {
                            layers.remove(&change.section.y);
                        }
                    }
                    let updates = affected
                        .into_iter()
                        .filter_map(|key| lights.get(&key).map(|data| (key, data.clone())))
                        .collect();
                    if light_out
                        .send(LitChunk {
                            request: job.request,
                            pos: job.pos,
                            updates,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            access,
            level_data,
            seed,
            spawn,
            registry,
            io,
            generator,
            generation: Arc::new(JobPool::new("ferraloom-worldgen", 2)?),
            saving: JobPool::new("ferraloom-save", 1)?,
            pending: BTreeMap::new(),
            light_tx,
            light_rx,
            saved: BTreeMap::new(),
            lights: BTreeMap::new(),
            ready: Default::default(),
        })
    }
    pub fn request(&mut self, request: u64, pos: ChunkPos) {
        let load = self.io.load(pos);
        let generator = self.generator.clone();
        let registry = self.registry.clone();
        let pool = self.generation.clone();
        let (reply, answer) = oneshot::channel();
        tokio::spawn(async move {
            let result = async {
                let disk = load.await?;
                pool.run(move || -> Result<Terrain> {
                    if let Some(tag) = disk {
                        let disk = SerializableChunkData::parse(tag)?;
                        ensure!(
                            disk.status
                                .as_ref()
                                .is_some_and(|s| s.to_string_lossy() == "minecraft:full"),
                            "resuming protochunk generation is not ported"
                        );
                        let mut sections = (0..24)
                            .map(|_| {
                                LevelChunkSection::new(
                                    Strategy::block_states(),
                                    registry.biome_strategy(),
                                    registry.default_biome(),
                                )
                            })
                            .collect::<Vec<_>>();
                        for section in disk.sections.as_deref().unwrap_or_default() {
                            if let Some(y) = section.y
                                && (-4..20).contains(&i32::from(y))
                                && let Some(runtime) = section.to_runtime(registry.as_ref())?
                            {
                                sections[(i32::from(y) + 4) as usize] = runtime;
                            }
                        }
                        let surface = surface_heights(&sections);
                        Ok(Terrain {
                            pos,
                            sections,
                            disk,
                            surface,
                        })
                    } else {
                        let chunk = generator.generate([pos.x, pos.z]);
                        let disk = SerializableChunkData {
                            data_version: Some(storage::DATA_VERSION),
                            x_pos: Some(pos.x),
                            y_pos: Some(-4),
                            z_pos: Some(pos.z),
                            status: Some("minecraft:full".into()),
                            is_light_on: Some(true),
                            block_entities: Some(Vec::new()),
                            block_ticks: Some(Vec::new()),
                            fluid_ticks: Some(Vec::new()),
                            post_processing: Some(vec![Vec::new(); 24]),
                            structures: Some(storage::StructureData::default()),
                            ..Default::default()
                        };
                        Ok(Terrain {
                            pos,
                            sections: chunk.sections,
                            disk,
                            surface: chunk.world_surface,
                        })
                    }
                })
                .await?
            }
            .await;
            let _ = reply.send(result);
        });
        self.pending.insert(request, answer);
    }
    /// Poll at ServerChunkCache's integration point, strictly by request ordinal.
    pub fn poll_terrain(&mut self) -> Result<Vec<(u64, Terrain)>> {
        let mut done = Vec::new();
        while let Some((&request, receiver)) = self.pending.first_key_value() {
            let _ = receiver;
            let receiver = self.pending.get_mut(&request).unwrap();
            match receiver.try_recv() {
                Ok(result) => {
                    self.pending.remove(&request);
                    done.push((request, result?));
                }
                Err(oneshot::error::TryRecvError::Empty) => break,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(done)
    }
    pub fn integrate(&mut self, request: u64, terrain: Terrain) -> Result<LevelChunk> {
        self.light_tx.send(LightJob {
            request,
            pos: terrain.pos,
            sections: terrain.sections.clone(),
        })?;
        self.saved.insert(terrain.pos.pack(), terrain.disk);
        Ok(LevelChunk::from_sections(terrain.pos, -4, terrain.sections))
    }
    /// Capture owned sections here; palette/NBT encoding runs in the save pool.
    pub fn capture_chunk(&self, chunk: &LevelChunk, game_time: i64) -> ChunkSaveSnapshot {
        ChunkSaveSnapshot {
            pos: chunk.pos(),
            sections: chunk.sections().cloned().collect(),
            disk: self.saved[&chunk.pos().pack()].clone(),
            light: self.lights.get(&chunk.pos().pack()).cloned(),
            game_time,
            light_correct: self.ready.contains(&chunk.pos()),
        }
    }
    pub async fn encode_saves(
        &self,
        snapshots: Vec<ChunkSaveSnapshot>,
    ) -> Result<Vec<(ChunkPos, CompoundTag)>> {
        let registry = self.registry.clone();
        self.saving
            .run(move || {
                snapshots
                    .into_iter()
                    .map(|snapshot| {
                        let pos = snapshot.pos;
                        Ok((pos, snapshot.encode(registry.as_ref())?))
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .await?
    }
    pub async fn save_level_and_players(
        &mut self,
        game_time: i64,
        players: Vec<(String, CompoundTag)>,
    ) -> Result<()> {
        self.level_data.data.insert("Time", Tag::Long(game_time));
        self.level_data.data.insert(
            "LastPlayed",
            Tag::Long(
                time::OffsetDateTime::now_utc()
                    .unix_timestamp_nanos()
                    .div_euclid(1_000_000) as i64,
            ),
        );
        let mut spawn = CompoundTag::new();
        spawn.insert("dimension", Tag::String("minecraft:overworld".into()));
        spawn.insert(
            "pos",
            Tag::IntArray(vec![self.spawn.x, self.spawn.y, self.spawn.z]),
        );
        spawn.insert("yaw", Tag::Float(0.));
        spawn.insert("pitch", Tag::Float(0.));
        self.level_data.data.insert("spawn", Tag::Compound(spawn));
        let access = self.access.clone();
        let level = self.level_data.clone();
        self.saving
            .run(move || -> Result<()> {
                for (uuid, tag) in players {
                    access.player_data().save(&uuid, &tag)?;
                }
                access.save_level_data(&level)?;
                Ok(())
            })
            .await??;
        Ok(())
    }
}
pub struct ChunkSaveSnapshot {
    pos: ChunkPos,
    sections: Vec<LevelChunkSection>,
    disk: SerializableChunkData,
    light: Option<ChunkLight>,
    game_time: i64,
    light_correct: bool,
}
impl ChunkSaveSnapshot {
    fn encode(mut self, registry: &RegistryAdapter) -> Result<CompoundTag> {
        let mut existing = self
            .disk
            .sections
            .take()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|section| section.y.map(|y| (y, section)))
            .collect::<BTreeMap<_, _>>();
        let mut sections = Vec::new();
        for (index, runtime) in self.sections.iter().enumerate() {
            let y = -4 + index as i32;
            let mut section = existing.remove(&(y as i8)).unwrap_or_default();
            section.y = Some(y as i8);
            section.update_from_runtime(runtime, registry)?;
            section.sky_light = self
                .light
                .as_ref()
                .and_then(|l| l.sky.get(&y))
                .map(|v| ByteArray(v.iter().map(|v| *v as i8).collect()));
            section.block_light = self
                .light
                .as_ref()
                .and_then(|l| l.block.get(&y))
                .map(|v| ByteArray(v.iter().map(|v| *v as i8).collect()));
            sections.push(section);
        }
        for y in [-5, 20] {
            if let Some(light) = &self.light {
                let sky = light.sky.get(&y);
                let block = light.block.get(&y);
                if sky.is_some() || block.is_some() {
                    let mut section = existing.remove(&(y as i8)).unwrap_or_default();
                    section.y = Some(y as i8);
                    section.sky_light =
                        sky.map(|v| ByteArray(v.iter().map(|v| *v as i8).collect()));
                    section.block_light =
                        block.map(|v| ByteArray(v.iter().map(|v| *v as i8).collect()));
                    sections.push(section);
                }
            }
        }
        sections.extend(existing.into_values());
        sections.sort_by_key(|s| s.y);
        self.disk.sections = Some(sections);
        self.disk.last_update = Some(self.game_time);
        self.disk.is_light_on = Some(self.light_correct);
        let surface = LongArray(heightmap(&surface_heights(&self.sections)));
        let maps = self.disk.heightmaps.get_or_insert_with(BTreeMap::new);
        maps.insert("WORLD_SURFACE".into(), surface.clone());
        maps.insert("MOTION_BLOCKING".into(), surface);
        Ok(self.disk.write()?)
    }
}
pub fn surface_heights(sections: &[LevelChunkSection]) -> [i32; 256] {
    std::array::from_fn(|index| {
        let x = (index & 15) as u8;
        let z = (index >> 4) as u8;
        for y in (-64..320).rev() {
            if !matches!(
                sections[((y + 64) / 16) as usize]
                    .get_block_state(x, (y & 15) as u8, z)
                    .block(),
                Block::AIR | Block::CAVE_AIR | Block::VOID_AIR
            ) {
                return y + 1;
            }
        }
        -64
    })
}
/// Port of Heightmap's padded 9-bit SimpleBitStorage for an overworld height.
pub fn heightmap(heights: &[i32; 256]) -> Vec<i64> {
    let mut words = vec![0u64; 256usize.div_ceil(7)];
    for (i, &y) in heights.iter().enumerate() {
        words[i / 7] |= ((y + 64) as u64 & 511) << ((i % 7) * 9);
    }
    words.into_iter().map(|w| w as i64).collect()
}
fn new_level_data(properties: &ServerProperties) -> LevelData {
    let mut data = CompoundTag::new();
    for (k, v) in [
        ("DataVersion", Tag::Int(storage::DATA_VERSION)),
        ("version", Tag::Int(19133)),
        (
            "LevelName",
            Tag::String(properties.get("level-name").into()),
        ),
        (
            "GameType",
            Tag::Int(match properties.get("gamemode") {
                "creative" => 1,
                "adventure" => 2,
                "spectator" => 3,
                _ => 0,
            }),
        ),
        ("Time", Tag::Long(0)),
        ("initialized", Tag::Byte(1)),
        ("allowCommands", Tag::Byte(1)),
        ("WasModded", Tag::Byte(1)),
    ] {
        data.insert(k, v);
    }
    let mut difficulty = CompoundTag::new();
    difficulty.insert(
        "difficulty",
        Tag::String(properties.get("difficulty").into()),
    );
    difficulty.insert(
        "hardcore",
        Tag::Byte(i8::from(properties.boolean("hardcore"))),
    );
    difficulty.insert("locked", Tag::Byte(0));
    data.insert("difficulty_settings", Tag::Compound(difficulty));
    let mut version = CompoundTag::new();
    version.insert("Name", Tag::String("26.3".into()));
    version.insert("Id", Tag::Int(storage::DATA_VERSION));
    version.insert("Snapshot", Tag::Byte(0));
    version.insert("Series", Tag::String("main".into()));
    data.insert("Version", Tag::Compound(version));
    data.insert(
        "version_history",
        Tag::List(ListTag(vec![Tag::Int(storage::DATA_VERSION)])),
    );
    let mut packs = CompoundTag::new();
    packs.insert(
        "Enabled",
        Tag::List(ListTag(vec![Tag::String("vanilla".into())])),
    );
    packs.insert("Disabled", Tag::List(ListTag::default()));
    data.insert("DataPacks", Tag::Compound(packs));
    data.insert(
        "enabled_features",
        Tag::List(ListTag(vec![Tag::String("minecraft:vanilla".into())])),
    );
    data.insert(
        "ServerBrands",
        Tag::List(ListTag(vec![Tag::String("Ferraloom".into())])),
    );
    LevelData {
        data,
        root_extra: CompoundTag::new(),
    }
}
pub fn json_nbt(value: &serde_json::Value) -> Tag {
    match value {
        serde_json::Value::Null => Tag::String("".into()),
        serde_json::Value::Bool(v) => Tag::Byte(i8::from(*v)),
        serde_json::Value::Number(n) => {
            if let Some(n) = n.as_i64() {
                if let Ok(n) = i32::try_from(n) {
                    Tag::Int(n)
                } else {
                    Tag::Long(n)
                }
            } else {
                Tag::Double(n.as_f64().unwrap())
            }
        }
        serde_json::Value::String(s) => Tag::String(s.as_str().into()),
        serde_json::Value::Array(a) => Tag::List(ListTag(a.iter().map(json_nbt).collect())),
        serde_json::Value::Object(o) => Tag::Compound(
            o.iter()
                .map(|(k, v)| (NbtString::from(k.as_str()), json_nbt(v)))
                .collect(),
        ),
    }
}
