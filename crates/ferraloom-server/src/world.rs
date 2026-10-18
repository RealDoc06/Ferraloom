//! World snapshots cross worker boundaries; only tick integration mutates Level.
use crate::{jobs::JobPool, properties::ServerProperties, registries::RegistrySet};
use anyhow::{Context, Result, ensure};
use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
use ferraloom_data::{Block, BlockClass, BlockState, EntityType, datapack::ResourceManager};
use ferraloom_game::block::GameWorldgenBlockBehavior as LocalBlockBehavior;
use ferraloom_game::level::{
    Level,
    chunk::LevelChunk,
    entity::{EntityLoadRequestId, EntityStorageRequest},
};
use ferraloom_light::{LevelLightEngine, LightLayer, SectionSnapshots};
use ferraloom_nbt::{CompoundTag, ListTag, NbtString, Tag};
use ferraloom_storage::{
    self as storage, ByteArray, IoWorker, LevelData, LevelStorageAccess, LongArray, SavedData,
    SavedDataStorage, SectionRegistry, SerializableChunkData,
};
use ferraloom_util::{Aabb, BlockPos, ChunkPos};
use ferraloom_worldgen::{
    decoration::{DecorationPlan, GenerationStep, StructureDecoration},
    feature::FeatureRegistry,
    generator::{
        ChunkGenerator, ChunkGeneratorAccess, GenerationData, WorldDimensions, WorldPreset,
    },
    region::{ChunkStatus, Heightmap, RegionChunk, WorldGenRegion},
    settings::DimensionType,
    spawn::{self, SpawnWorld},
};
use serde::Deserialize;
use std::{
    borrow::Cow,
    collections::{BTreeMap, VecDeque},
    future::Future,
    path::Path,
    pin::Pin,
    sync::{Arc, mpsc},
    task::{Context as TaskContext, Poll, Waker},
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

struct FlatDecoration {
    registry: Option<FeatureRegistry>,
    plan: Option<DecorationPlan>,
}

/// Startup owns every queried chunk. The same outputs are later transferred
/// into the normal request/integration path, including decoration writes.
struct SpawnChunks {
    generator: Arc<ChunkGenerator>,
    registry: Arc<RegistryAdapter>,
    dimension: DimensionType,
    flat_decoration: Option<Arc<FlatDecoration>>,
    seed: i64,
    io: IoWorker,
    chunks: BTreeMap<i64, Terrain>,
}
impl SpawnChunks {
    fn state(&mut self, p: [i32; 3]) -> std::result::Result<BlockState, ferraloom_worldgen::Error> {
        if p[1] < self.dimension.min_y || p[1] >= self.dimension.min_y + self.dimension.height {
            return Ok(Block::VOID_AIR.default_state());
        }
        self.ensure_chunk([p[0] >> 4, p[2] >> 4])?;
        let terrain = &self.chunks[&ChunkPos::new(p[0] >> 4, p[2] >> 4).to_long()];
        Ok(
            terrain.sections[((p[1] - self.dimension.min_y) >> 4) as usize].get_block_state(
                (p[0] & 15) as u8,
                (p[1] & 15) as u8,
                (p[2] & 15) as u8,
            ),
        )
    }
}
impl SpawnWorld for SpawnChunks {
    fn dimension(&self) -> DimensionType {
        self.dimension
    }
    fn ensure_chunk(
        &mut self,
        position: [i32; 2],
    ) -> std::result::Result<(), ferraloom_worldgen::Error> {
        let pos = ChunkPos::new(position[0], position[1]);
        if let std::collections::btree_map::Entry::Vacant(entry) = self.chunks.entry(pos.to_long())
        {
            let terrain = (|| -> Result<Terrain> {
                if let Some(tag) = self.io.load(pos).wait()? {
                    load_terrain(tag, &self.registry, pos)
                } else {
                    generate_terrain(
                        &self.generator,
                        self.dimension,
                        self.flat_decoration.as_deref(),
                        self.seed,
                        pos,
                    )
                }
            })()
            .map_err(|error| ferraloom_worldgen::Error(error.to_string()))?;
            entry.insert(terrain);
        }
        Ok(())
    }
    fn height(
        &mut self,
        map: Heightmap,
        x: i32,
        z: i32,
    ) -> std::result::Result<i32, ferraloom_worldgen::Error> {
        for y in (self.dimension.min_y..self.dimension.min_y + self.dimension.height).rev() {
            let state = self.state([x, y, z])?;
            if self.generator.data().heightmap_matches(map, state) {
                return Ok(y);
            }
        }
        Ok(self.dimension.min_y - 1)
    }
    fn has_fluid(&mut self, p: [i32; 3]) -> std::result::Result<bool, ferraloom_worldgen::Error> {
        Ok(!self.state(p)?.data().fluid.empty)
    }
    fn collision_up_face_full(
        &mut self,
        p: [i32; 3],
    ) -> std::result::Result<bool, ferraloom_worldgen::Error> {
        spawn::cached_collision_up_face_full(self.state(p)?).ok_or_else(|| {
            ferraloom_worldgen::Error("Unsupported: dynamic spawn collision shape".into())
        })
    }
    fn no_collision_no_liquid(
        &mut self,
        p: [i32; 3],
    ) -> std::result::Result<bool, ferraloom_worldgen::Error> {
        // Startup has no live entities; EntityGetter contributes no collisions.
        no_spawn_block_collision(p, |p| self.state(p))
    }
}

fn no_spawn_block_collision(
    p: [i32; 3],
    mut state: impl FnMut([i32; 3]) -> std::result::Result<BlockState, ferraloom_worldgen::Error>,
) -> std::result::Result<bool, ferraloom_worldgen::Error> {
    let dimensions = EntityType::PLAYER.data();
    let radius = f64::from(dimensions.width) / 2.0;
    let bbox = Aabb::new(
        f64::from(p[0]) + 0.5 - radius,
        f64::from(p[1]),
        f64::from(p[2]) + 0.5 - radius,
        f64::from(p[0]) + 0.5 + radius,
        f64::from(p[1]) + f64::from(dimensions.height),
        f64::from(p[2]) + 0.5 + radius,
    );
    for x in (bbox.min_x - 1e-7).floor() as i32 - 1..=(bbox.max_x + 1e-7).floor() as i32 + 1 {
        for y in (bbox.min_y - 1e-7).floor() as i32 - 1..=(bbox.max_y + 1e-7).floor() as i32 + 1 {
            for z in (bbox.min_z - 1e-7).floor() as i32 - 1..=(bbox.max_z + 1e-7).floor() as i32 + 1
            {
                let state = state([x, y, z])?;
                // LiquidBlock's alwaysCollideWithFluid collision is a full cube.
                if state.block().is_instance_of(BlockClass::LiquidBlock) {
                    if bbox.intersects(Aabb::new(
                        f64::from(x),
                        f64::from(y),
                        f64::from(z),
                        f64::from(x + 1),
                        f64::from(y + 1),
                        f64::from(z + 1),
                    )) {
                        return Ok(false);
                    }
                } else {
                    if state.cached_collision_full_block().is_none() {
                        return Err(ferraloom_worldgen::Error(
                            "Unsupported: dynamic spawn collision shape".into(),
                        ));
                    }
                    if state.data().collision.boxes().iter().any(|shape| {
                        bbox.intersects(Aabb::new(
                            shape.min[0] + f64::from(x),
                            shape.min[1] + f64::from(y),
                            shape.min[2] + f64::from(z),
                            shape.max[0] + f64::from(x),
                            shape.max[1] + f64::from(y),
                            shape.max[2] + f64::from(z),
                        ))
                    }) {
                        return Ok(false);
                    }
                }
            }
        }
    }
    Ok(true)
}

fn spawn_tag(p: BlockPos) -> Tag {
    Tag::Compound(
        [
            ("dimension", Tag::String("minecraft:overworld".into())),
            ("pos", Tag::IntArray(vec![p.x, p.y, p.z])),
            ("yaw", Tag::Float(0.0)),
            ("pitch", Tag::Float(0.0)),
        ]
        .into_iter()
        .collect(),
    )
}
struct NoStructures;
impl StructureDecoration for NoStructures {
    fn place_step(
        &mut self,
        _: GenerationStep,
        _: &mut WorldGenRegion<'_>,
        _: &mut ferraloom_util::random::WorldgenRandom,
        _: i64,
    ) -> std::result::Result<(), ferraloom_worldgen::Error> {
        Ok(())
    }
}

fn saved_tick(position: [i32; 3], name: &str, priority: i32) -> storage::SavedTick {
    storage::SavedTick {
        id: Some(name.into()),
        x: Some(position[0]),
        y: Some(position[1]),
        z: Some(position[2]),
        delay: Some(0),
        priority: Some(priority),
        ..Default::default()
    }
}

pub struct World {
    pub access: Arc<LevelStorageAccess>,
    pub level_data: LevelData,
    pub seed: i64,
    pub spawn: BlockPos,
    pub registry: Arc<RegistryAdapter>,
    pub io: IoWorker,
    entity_io: Option<EntityIo>,
    pub dimensions: WorldDimensions,
    generator: Arc<ChunkGenerator>,
    dimension: DimensionType,
    flat_decoration: Option<Arc<FlatDecoration>>,
    initial_chunks: BTreeMap<i64, Terrain>,
    generation: Arc<JobPool>,
    saving: JobPool,
    pub pending: BTreeMap<u64, oneshot::Receiver<Result<Terrain>>>,
    light_tx: mpsc::Sender<LightJob>,
    pub light_rx: tokio::sync::mpsc::UnboundedReceiver<LitChunk>,
    pub saved: BTreeMap<i64, SerializableChunkData>,
    pub lights: BTreeMap<i64, ChunkLight>,
    pub ready: std::collections::HashSet<ChunkPos>,
}
/// The entity region worker is distinct from block-chunk storage. Completion
/// polling returns owned responses; the game integrates its request ordinals.
struct EntityIo {
    worker: IoWorker,
    loads: VecDeque<(EntityLoadRequestId, storage::IoTask<Option<CompoundTag>>)>,
    stores: VecDeque<storage::IoTask<()>>,
}
impl EntityIo {
    fn submit(&mut self, requests: Vec<EntityStorageRequest>) -> Result<()> {
        for request in requests {
            match request {
                EntityStorageRequest::Load { id, pos } => {
                    self.loads.push_back((id, self.worker.load(pos)));
                }
                EntityStorageRequest::Store { pos, chunk } => {
                    let tag = chunk.map(|chunk| chunk.write()).transpose()?;
                    self.stores.push_back(self.worker.store(pos, tag));
                }
            }
        }
        Ok(())
    }
    fn poll(&mut self, level: &mut Level) -> Result<()> {
        let mut context = TaskContext::from_waker(Waker::noop());
        while let Some((id, task)) = self.loads.front_mut() {
            let Poll::Ready(result) = Pin::new(task).poll(&mut context) else {
                break;
            };
            let id = *id;
            self.loads.pop_front();
            level.complete_entity_chunk_load(id, result)?;
        }
        while let Some(task) = self.stores.front_mut() {
            let Poll::Ready(result) = Pin::new(task).poll(&mut context) else {
                break;
            };
            self.stores.pop_front();
            result?;
        }
        Ok(())
    }
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
        let bonus_chest = settings
            .as_ref()
            .is_some_and(|settings| settings.value().get_boolean_or("bonus_chest", false));
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
                "generate_features",
                Tag::Byte(i8::from(properties.boolean("generate-structures"))),
            );
            settings.insert("bonus_chest", Tag::Byte(0));
            saved_data.save(
                "minecraft:world_gen_settings",
                &mut SavedData::new(settings),
            )?;
            preset
        };
        let dimension = preset.dimensions.overworld().dimension;
        let generator = preset.dimensions.overworld().generator.clone();
        let flat_decoration = if let ChunkGenerator::Flat(flat) = generator.as_ref() {
            let registry = flat.chunk_local_feature_registry(resources)?;
            Some(Arc::new(FlatDecoration {
                plan: registry
                    .as_ref()
                    .map(|registry| {
                        DecorationPlan::new(registry, &flat.biome_source().possible_biomes())
                    })
                    .transpose()?,
                registry,
            }))
        } else {
            None
        };
        let mut level_data = if access.path().join("level.dat").exists() {
            access.read_level_data()?
        } else {
            new_level_data(properties)
        };
        access.save_level_data(&level_data)?;
        let mut spawn = level_data
            .data
            .get_compound("spawn")
            .and_then(|t| t.get_int_array("pos"))
            .filter(|v| v.len() == 3)
            .map_or(BlockPos::new(0, 80, 0), |v| BlockPos::new(v[0], v[1], v[2]));
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
        let generation = Arc::new(JobPool::new("ferraloom-worldgen", 2)?);
        let mut initial_chunks = BTreeMap::new();
        let io = if !level_data.data.contains_key("spawn") {
            let (reply, result) = mpsc::channel();
            let spawn_generator = generator.clone();
            let registry = registry.clone();
            let flat_decoration = flat_decoration.clone();
            let _task = generation.submit(move || {
                let outcome = (|| -> Result<_> {
                    let mut world = SpawnChunks {
                        generator: spawn_generator,
                        registry,
                        dimension,
                        flat_decoration,
                        seed,
                        io,
                        chunks: BTreeMap::new(),
                    };
                    let generator = world.generator.clone();
                    let position = spawn::initial_spawn(
                        &mut world,
                        generator.as_ref(),
                        matches!(generator.as_ref(), ChunkGenerator::Debug(_)),
                        bonus_chest,
                    )?;
                    Ok((world.io, world.chunks, position))
                })();
                let _ = reply.send(outcome);
            });
            let (io, chunks, position) = result.recv().context("spawn worker terminated")??;
            initial_chunks = chunks;
            spawn = BlockPos::new(position[0], position[1], position[2]);
            level_data.data.insert("spawn", spawn_tag(spawn));
            level_data.data.insert("initialized", Tag::Byte(1));
            access.save_level_data(&level_data)?;
            if matches!(generator.as_ref(), ChunkGenerator::Noise(_)) {
                println!(
                    "Initial spawn search used real generated chunks; feature and structure statuses are passthrough."
                );
            }
            io
        } else {
            io
        };
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
            entity_io: None,
            dimensions: preset.dimensions,
            generator,
            dimension,
            flat_decoration,
            initial_chunks,
            generation,
            saving: JobPool::new("ferraloom-save", 1)?,
            pending: BTreeMap::new(),
            light_tx,
            light_rx,
            saved: BTreeMap::new(),
            lights: BTreeMap::new(),
            ready: Default::default(),
        })
    }
    fn entity_io(&mut self, properties: &ServerProperties) -> Result<&mut EntityIo> {
        if self.entity_io.is_none() {
            let version = match properties.get("region-file-compression") {
                "none" => storage::RegionFileVersion::None,
                "lz4" => storage::RegionFileVersion::Lz4,
                _ => storage::RegionFileVersion::Deflate,
            };
            self.entity_io = Some(EntityIo {
                worker: IoWorker::new(
                    self.access.dimension("minecraft:overworld")?.entities,
                    version,
                    properties.boolean("sync-chunk-writes"),
                )?,
                loads: VecDeque::new(),
                stores: VecDeque::new(),
            });
        }
        Ok(self.entity_io.as_mut().unwrap())
    }
    /// PersistentEntitySectionManager#tick polling point, also used while frozen.
    pub fn tick_entity_management(
        &mut self,
        level: &mut Level,
        properties: &ServerProperties,
    ) -> Result<()> {
        let io = self.entity_io(properties)?;
        io.poll(level)?;
        level.tick_entity_management()?;
        io.submit(level.take_entity_storage_requests())?;
        Ok(())
    }
    pub async fn save_entities(
        &mut self,
        level: &mut Level,
        properties: &ServerProperties,
        flush: bool,
    ) -> Result<()> {
        let io = self.entity_io(properties)?;
        if !flush {
            level.auto_save_entities()?;
            io.submit(level.take_entity_storage_requests())?;
            return Ok(());
        }
        level.begin_entity_save_all()?;
        loop {
            io.submit(level.take_entity_storage_requests())?;
            io.worker.synchronize(false).await?;
            io.poll(level)?;
            if level.poll_entity_save_all()? {
                io.submit(level.take_entity_storage_requests())?;
                io.worker.synchronize(true).await?;
                io.poll(level)?;
                return Ok(());
            }
        }
    }
    pub async fn close_entity_storage(&mut self) -> Result<()> {
        if let Some(io) = &mut self.entity_io {
            io.worker.shutdown().await?;
        }
        Ok(())
    }
    pub fn request(&mut self, request: u64, pos: ChunkPos) {
        if let Some(terrain) = self.initial_chunks.remove(&pos.to_long()) {
            let (reply, answer) = oneshot::channel();
            let _ = reply.send(Ok(terrain));
            self.pending.insert(request, answer);
            return;
        }
        let load = self.io.load(pos);
        let generator = self.generator.clone();
        let dimension = self.dimension;
        let flat_decoration = self.flat_decoration.clone();
        let seed = self.seed;
        let registry = self.registry.clone();
        let pool = self.generation.clone();
        let (reply, answer) = oneshot::channel();
        tokio::spawn(async move {
            let result = async {
                let disk = load.await?;
                pool.run(move || -> Result<Terrain> {
                    if let Some(tag) = disk {
                        load_terrain(tag, &registry, pos)
                    } else {
                        generate_terrain(
                            &generator,
                            dimension,
                            flat_decoration.as_deref(),
                            seed,
                            pos,
                        )
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
        let mut chunk = LevelChunk::from_sections(terrain.pos, -4, terrain.sections);
        chunk.set_inhabited_time(terrain.disk.inhabited_time.unwrap_or(0));
        chunk
            .load_saved_ticks(&terrain.disk)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        self.saved.insert(terrain.pos.pack(), terrain.disk);
        Ok(chunk)
    }
    /// Capture owned sections here; palette/NBT encoding runs in the save pool.
    pub fn capture_chunk(
        &self,
        level: &Level,
        pos: ChunkPos,
        game_time: i64,
    ) -> Result<ChunkSaveSnapshot> {
        let chunk = level.chunk(pos).context("save of missing full chunk")?;
        let mut disk = self.saved[&pos.pack()].clone();
        level
            .save_chunk_ticks(pos, &mut disk)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        disk.inhabited_time = Some(chunk.inhabited_time());
        let maps = disk.heightmaps.get_or_insert_with(BTreeMap::new);
        use ferraloom_game::level::heightmap::Heightmap;
        for (map, name) in [
            (Heightmap::WorldSurface, "WORLD_SURFACE"),
            (Heightmap::OceanFloor, "OCEAN_FLOOR"),
            (Heightmap::MotionBlocking, "MOTION_BLOCKING"),
            (
                Heightmap::MotionBlockingNoLeaves,
                "MOTION_BLOCKING_NO_LEAVES",
            ),
        ] {
            maps.insert(
                name.into(),
                LongArray(heightmap(
                    chunk
                        .heightmap(map)
                        .context("full chunk heightmaps not primed")?,
                )),
            );
        }
        Ok(ChunkSaveSnapshot {
            pos,
            sections: chunk.sections().cloned().collect(),
            disk,
            light: self.lights.get(&pos.pack()).cloned(),
            game_time,
            light_correct: self.ready.contains(&pos),
        })
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
        let snapshots = std::mem::take(&mut self.initial_chunks)
            .into_values()
            .map(|terrain| ChunkSaveSnapshot {
                pos: terrain.pos,
                sections: terrain.sections,
                disk: terrain.disk,
                light: None,
                game_time,
                light_correct: false,
            })
            .collect();
        for (pos, tag) in self.encode_saves(snapshots).await? {
            self.io.store(pos, Some(tag)).await?;
        }
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

/// Inverse of the world-settings JSON-to-NBT conversion. Codec booleans are
/// bytes in NBT; other integral widths retain their full value.
fn nbt_json(tag: &Tag) -> Result<serde_json::Value> {
    use serde_json::{Value, json};
    Ok(match tag {
        Tag::End => anyhow::bail!("end tag in world settings"),
        Tag::Byte(value @ (0 | 1)) => Value::Bool(*value != 0),
        Tag::Byte(value) => json!(value),
        Tag::Short(value) => json!(value),
        Tag::Int(value) => json!(value),
        Tag::Long(value) => json!(value),
        Tag::Float(value) => {
            ensure!(value.is_finite(), "nonfinite world setting");
            json!(value)
        }
        Tag::Double(value) => {
            ensure!(value.is_finite(), "nonfinite world setting");
            json!(value)
        }
        Tag::String(value) => Value::String(value.to_string_lossy()),
        Tag::List(values) => Value::Array(values.0.iter().map(nbt_json).collect::<Result<_>>()?),
        Tag::Compound(values) => Value::Object(
            values
                .iter()
                .map(|(name, value)| Ok((name.to_string_lossy(), nbt_json(value)?)))
                .collect::<Result<_>>()?,
        ),
        Tag::ByteArray(values) => json!(values),
        Tag::IntArray(values) => json!(values),
        Tag::LongArray(values) => json!(values),
    })
}

fn generate_terrain(
    generator: &ChunkGenerator,
    dimension: DimensionType,
    flat_decoration: Option<&FlatDecoration>,
    seed: i64,
    pos: ChunkPos,
) -> Result<Terrain> {
    let generated = generator.build_terrain([pos.x, pos.z], dimension, None)?;
    let mut chunks = [RegionChunk::new(generated, ChunkStatus::Terrain)];
    {
        let mut region = WorldGenRegion::new(
            &mut chunks,
            [pos.x, pos.z],
            &[ChunkStatus::Terrain],
            0,
            None,
            &LocalBlockBehavior,
            seed,
            generator.sea_level(),
        )?;
        region.set_has_skylight(dimension.has_skylight);
        region.set_fast_lava(dimension.fast_lava);
        region.set_random(generator.region_random([pos.x, pos.z], seed));
        match generator {
            ChunkGenerator::Debug(debug) => debug.apply_biome_decoration(&mut region)?,
            ChunkGenerator::Flat(_) => {
                let flat = flat_decoration.context("flat decoration plan")?;
                let registry = flat.registry.as_ref().context("Unsupported: flat features and lakes require owned-neighborhood chunk-status scheduling")?;
                let biomes = generator
                    .biome_source()
                    .possible_biomes()
                    .into_iter()
                    .collect();
                flat.plan.as_ref().unwrap().apply_for_biomes(
                    &mut region,
                    generator,
                    registry,
                    &mut NoStructures,
                    &biomes,
                )?;
            }
            ChunkGenerator::Noise(_) => {}
        }
    }
    let [chunk] = chunks;
    ensure!(
        chunk.entities.is_empty(),
        "Unsupported: generation entities need owned-region integration"
    );
    let mut post_processing = vec![Vec::new(); (dimension.height / 16) as usize];
    for [x, y, z] in &chunk.chunk.post_processing {
        post_processing[((y - dimension.min_y) >> 4) as usize]
            .push(((x & 15) | ((y & 15) << 4) | ((z & 15) << 8)) as i16);
    }
    let disk = SerializableChunkData {
        data_version: Some(storage::DATA_VERSION),
        x_pos: Some(pos.x),
        y_pos: Some(dimension.min_y >> 4),
        z_pos: Some(pos.z),
        status: Some("minecraft:full".into()),
        is_light_on: Some(true),
        block_entities: Some(chunk.block_entities.into_values().collect()),
        block_ticks: Some(
            chunk
                .block_ticks
                .ticks()
                .iter()
                .map(|tick| saved_tick(tick.position, tick.kind.name(), tick.priority))
                .collect(),
        ),
        fluid_ticks: Some(
            chunk
                .fluid_ticks
                .ticks()
                .iter()
                .map(|tick| saved_tick(tick.position, tick.kind.name(), tick.priority))
                .collect(),
        ),
        post_processing: Some(post_processing),
        structures: Some(storage::StructureData::default()),
        ..Default::default()
    };
    Ok(Terrain {
        pos,
        surface: surface_heights(&chunk.chunk.sections),
        sections: chunk.chunk.sections,
        disk,
    })
}

fn load_terrain(tag: CompoundTag, registry: &RegistryAdapter, pos: ChunkPos) -> Result<Terrain> {
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
            && let Some(runtime) = section.to_runtime(registry)?
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
}

#[cfg(test)]
mod entity_io_tests {
    use super::*;
    use ferraloom_game::{
        Behaviors,
        level::{Region, entity::Visibility},
    };
    use ferraloom_storage::{EntityChunk, IntArray};

    #[tokio::test]
    async fn entity_region_worker_routes_loads_writes_and_deletions() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let dir = tempfile::tempdir().unwrap();
        let mut io = EntityIo {
            worker: IoWorker::new(dir.path(), storage::RegionFileVersion::Deflate, false).unwrap(),
            loads: VecDeque::new(),
            stores: VecDeque::new(),
        };
        let pos = ChunkPos::new(-2, 3);
        let mut extra = CompoundTag::new();
        extra.insert("future", Tag::Int(42));
        io.submit(vec![EntityStorageRequest::Store {
            pos,
            chunk: Some(EntityChunk {
                data_version: Some(storage::DATA_VERSION),
                position: Some(IntArray(vec![pos.x, pos.z])),
                entities: Some(Vec::new()),
                extra,
            }),
        }])
        .unwrap();
        io.worker.synchronize(false).await.unwrap();
        let mut level = Level::new(Region::new(&BEHAVIORS));
        level
            .update_entity_chunk_status(pos, Visibility::Tracked)
            .unwrap();
        io.submit(level.take_entity_storage_requests()).unwrap();
        io.worker.synchronize(false).await.unwrap();
        assert!(!level.are_entities_loaded(pos));
        io.poll(&mut level).unwrap();
        assert!(!level.are_entities_loaded(pos));
        level.tick_entity_management().unwrap();
        assert!(level.are_entities_loaded(pos));
        assert_eq!(
            io.worker
                .load(pos)
                .await
                .unwrap()
                .unwrap()
                .get_int("future"),
            Some(42)
        );
        io.submit(vec![EntityStorageRequest::Store { pos, chunk: None }])
            .unwrap();
        io.worker.synchronize(true).await.unwrap();
        io.poll(&mut level).unwrap();
        assert!(io.worker.load(pos).await.unwrap().is_none());
        io.worker.shutdown().await.unwrap();
    }
}
