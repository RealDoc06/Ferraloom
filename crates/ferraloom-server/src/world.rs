//! World snapshots cross worker boundaries; only tick integration mutates Level.
use crate::{
    generation::{GenerationCompletion, GenerationOutput, GenerationRequest, GenerationState},
    jobs::JobPool,
    properties::ServerProperties,
    registries::RegistrySet,
};
use anyhow::{Context, Result, ensure};
use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
use ferraloom_data::{Block, BlockClass, BlockState, EntityType, datapack::ResourceManager};
use ferraloom_game::level::{
    Level,
    chunk::LevelChunk,
    chunk_task_dispatcher::{ChunkTaskDispatch, ThrottlingChunkTaskDispatcher},
    entity::{EntityLoadRequestId, EntityStorageRequest},
};
use ferraloom_light::{LevelLightEngine, LightLayer, SectionSnapshots};
use ferraloom_nbt::{CompoundTag, ListTag, NbtString, Tag};
use ferraloom_storage::{
    self as storage, ByteArray, IoWorker, LevelData, LevelStorageAccess, LongArray, SavedData,
    SavedDataStorage, SectionRegistry, SerializableChunkData,
};
use ferraloom_util::{Aabb, BlockPos, ChunkPos, SectionPos};
use ferraloom_worldgen::{
    generator::{
        ChunkGenerator, ChunkGeneratorAccess, GenerationData, WorldDimensions, WorldPreset,
    },
    region::{ChunkStatus, Heightmap},
    settings::DimensionType,
    spawn::{self, SpawnWorld},
};
use serde::Deserialize;
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, VecDeque},
    future::Future,
    path::Path,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
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

pub struct GenerationLightNeighbor {
    pub pos: ChunkPos,
    pub sections: Vec<LevelChunkSection>,
    pub disk: SerializableChunkData,
}
pub struct Terrain {
    pub pos: ChunkPos,
    pub sections: Vec<LevelChunkSection>,
    pub disk: SerializableChunkData,
    pub surface: [i32; 256],
    pub light_neighbors: Vec<GenerationLightNeighbor>,
}
#[derive(Clone, Default)]
pub struct ChunkLight {
    pub sky: BTreeMap<i32, Vec<u8>>,
    pub block: BTreeMap<i32, Vec<u8>>,
}
#[derive(Clone, Copy, Default)]
pub struct LightDirtySections {
    pub sky: u64,
    pub block: u64,
}
impl ChunkLight {
    pub fn snapshot(&self) -> ferraloom_game::level::weather::ChunkLightSnapshot {
        let convert = |layers: &BTreeMap<i32, Vec<u8>>| {
            layers
                .iter()
                .map(|(&y, data)| {
                    (
                        y,
                        ferraloom_light::DataLayer::from_bytes(
                            data.clone()
                                .into_boxed_slice()
                                .try_into()
                                .expect("light engine publishes 2048-byte sections"),
                        ),
                    )
                })
                .collect()
        };
        ferraloom_game::level::weather::ChunkLightSnapshot {
            sky: convert(&self.sky),
            block: convert(&self.block),
        }
    }
}
pub struct LitChunk {
    /// Delta relighting never completes or resets a generation request.
    pub initialized: Option<(u64, ChunkPos)>,
    pub updates: BTreeMap<i64, ChunkLight>,
    pub dirty: BTreeMap<i64, LightDirtySections>,
}
enum LightJob {
    Initialize {
        request: u64,
        pos: ChunkPos,
        sections: Vec<LevelChunkSection>,
        saved_light: Option<Box<SerializableChunkData>>,
        neighbors: Vec<GenerationLightNeighbor>,
    },
    Unload {
        pos: ChunkPos,
    },
    Changed {
        chunks: Vec<(ChunkPos, Vec<LevelChunkSection>)>,
        changes: Vec<LightMutation>,
    },
}
enum LightMutation {
    CheckBlock(BlockPos),
    SectionStatus(SectionPos, bool),
}

/// Off-tick ThreadedLevelLightEngine worker. Messages and dirty effects retain
/// submission order; only owned section snapshots cross the worker boundary.
fn run_light_worker(
    light_in: mpsc::Receiver<LightJob>,
    light_out: tokio::sync::mpsc::UnboundedSender<LitChunk>,
    min_section: i32,
    max_section: i32,
    has_skylight: bool,
) {
    let mut snapshots = SectionSnapshots::new(min_section, max_section);
    let mut engine = LevelLightEngine::new(true, has_skylight);
    let mut lights: BTreeMap<i64, ChunkLight> = BTreeMap::new();
    let mut full_chunks = std::collections::HashSet::new();
    while let Ok(job) = light_in.recv() {
        let (initialized, changes) = match job {
            LightJob::Initialize {
                request,
                pos,
                sections,
                saved_light,
                neighbors,
            } => {
                snapshots.insert_chunk(
                    pos,
                    sections
                        .into_iter()
                        .map(|section| Some(Cow::Owned(section)))
                        .collect(),
                );
                let neighbors = neighbors
                    .into_iter()
                    .filter(|neighbor| !full_chunks.contains(&neighbor.pos))
                    .collect::<Vec<_>>();
                for neighbor in &neighbors {
                    snapshots.insert_chunk(
                        neighbor.pos,
                        neighbor
                            .sections
                            .iter()
                            .cloned()
                            .map(|section| Some(Cow::Owned(section)))
                            .collect(),
                    );
                }
                let mut changes = Vec::new();
                for neighbor in neighbors {
                    for section in neighbor.disk.sections.as_deref().unwrap_or_default() {
                        if let Some(y) = section.y {
                            for (layer, data) in [
                                (LightLayer::Sky, &section.sky_light),
                                (LightLayer::Block, &section.block_light),
                            ] {
                                if let Some(data) = data {
                                    let bytes = data
                                        .0
                                        .iter()
                                        .map(|&b| b as u8)
                                        .collect::<Vec<_>>()
                                        .into_boxed_slice()
                                        .try_into()
                                        .expect("validated generation neighbor light");
                                    engine.queue_section_data(
                                        layer,
                                        SectionPos::new(
                                            neighbor.pos.x,
                                            i32::from(y),
                                            neighbor.pos.z,
                                        ),
                                        Some(ferraloom_light::DataLayer::from_bytes(bytes)),
                                    );
                                }
                            }
                        }
                    }
                    changes.extend(
                        engine
                            .initialize_light(
                                &snapshots,
                                neighbor.pos,
                                neighbor.disk.is_light_on == Some(true),
                            )
                            .sections,
                    );
                }
                let lighted = saved_light
                    .as_ref()
                    .is_some_and(|disk| disk.is_light_on == Some(true));
                if let Some(disk) = saved_light {
                    for section in disk.sections.as_deref().unwrap_or_default() {
                        if let Some(y) = section.y {
                            for (layer, data) in [
                                (LightLayer::Sky, &section.sky_light),
                                (LightLayer::Block, &section.block_light),
                            ] {
                                if let Some(data) = data {
                                    let bytes = data
                                        .0
                                        .iter()
                                        .map(|&b| b as u8)
                                        .collect::<Vec<_>>()
                                        .into_boxed_slice()
                                        .try_into()
                                        .expect("validated generation light");
                                    engine.queue_section_data(
                                        layer,
                                        SectionPos::new(pos.x, i32::from(y), pos.z),
                                        Some(ferraloom_light::DataLayer::from_bytes(bytes)),
                                    );
                                }
                            }
                        }
                    }
                }
                changes.extend(engine.initialize_light(&snapshots, pos, lighted).sections);
                changes.extend(engine.light_chunk(&snapshots, pos, lighted).sections);
                (Some((request, pos)), changes)
            }
            LightJob::Unload { pos } => {
                for y in min_section..max_section {
                    engine.update_section_status(SectionPos::new(pos.x, y, pos.z), true);
                }
                engine.set_light_enabled(&snapshots, pos, false);
                snapshots.remove_chunk(pos);
                engine.forget_chunk_sources(pos);
                lights.remove(&pos.pack());
                full_chunks.remove(&pos);
                (None, engine.run_light_updates(&snapshots).sections)
            }
            LightJob::Changed { chunks, changes } => {
                for (pos, sections) in chunks {
                    snapshots.insert_chunk(
                        pos,
                        sections
                            .into_iter()
                            .map(|section| Some(Cow::Owned(section)))
                            .collect(),
                    );
                }
                for change in changes {
                    match change {
                        LightMutation::CheckBlock(pos) => engine.check_block(pos),
                        LightMutation::SectionStatus(pos, empty) => {
                            engine.update_section_status(pos, empty)
                        }
                    }
                }
                (None, engine.run_light_updates(&snapshots).sections)
            }
        };
        let mut affected = std::collections::BTreeSet::new();
        let mut dirty = BTreeMap::<i64, LightDirtySections>::new();
        if let Some((_, pos)) = initialized {
            full_chunks.insert(pos);
            affected.insert(pos.pack());
            lights.entry(pos.pack()).or_default();
        }
        for change in changes {
            let key = change.section.chunk().pack();
            affected.insert(key);
            let mask = dirty.entry(key).or_default();
            let bit = 1_u64 << (change.section.y - min_section + 1);
            match change.layer {
                LightLayer::Sky => mask.sky |= bit,
                LightLayer::Block => mask.block |= bit,
            }
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
                initialized,
                updates,
                dirty,
            })
            .is_err()
        {
            break;
        }
    }
}

/// Startup owns every queried chunk. The same outputs are later transferred
/// into the normal request/integration path, including decoration writes.
struct SpawnChunks {
    generator: Arc<ChunkGenerator>,
    registry: Arc<RegistryAdapter>,
    dimension: DimensionType,
    state: GenerationState,
    io: IoWorker,
    chunks: BTreeMap<i64, Terrain>,
    skips: Vec<(String, String)>,
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
            let generated = self
                .state
                .generate(0, pos, ChunkStatus::Full, &self.registry, &self.io, None)
                .map_err(|error| ferraloom_worldgen::Error(error.to_string()))?;
            for (neighbor, disk) in generated.neighbors {
                self.io
                    .store(
                        neighbor,
                        Some(
                            disk.write()
                                .map_err(|e| ferraloom_worldgen::Error(e.to_string()))?,
                        ),
                    )
                    .wait()
                    .map_err(|e| ferraloom_worldgen::Error(e.to_string()))?;
            }
            self.skips.extend(generated.skips);
            let terrain = generated.terrain;
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
struct ChunkUnload {
    pos: ChunkPos,
    epoch: u64,
    saving: oneshot::Receiver<Result<()>>,
}
pub struct World {
    pub access: Arc<LevelStorageAccess>,
    pub level_data: LevelData,
    pub seed: i64,
    pub spawn: BlockPos,
    pub registry: Arc<RegistryAdapter>,
    pub io: Arc<IoWorker>,
    entity_io: Option<EntityIo>,
    pub dimensions: WorldDimensions,
    generator: Arc<ChunkGenerator>,
    dimension: DimensionType,
    initial_chunks: BTreeMap<i64, Terrain>,
    generation: Arc<JobPool>,
    generation_state: Option<GenerationState>,
    generation_work: Option<oneshot::Receiver<GenerationCompletion>>,
    generation_dispatcher: ThrottlingChunkTaskDispatcher<GenerationRequest>,
    active_generation: BTreeMap<ChunkPos, (u64, Arc<AtomicBool>)>,
    running_generation: Option<(u64, ChunkPos)>,
    retained_generation: BTreeSet<ChunkPos>,
    pub failed_generation: Vec<(ChunkPos, u64)>,
    proto_saves: VecDeque<storage::IoTask<()>>,
    unloading: VecDeque<ChunkUnload>,
    evictions: Vec<ChunkPos>,
    saving: JobPool,
    pending: BTreeMap<u64, (ChunkPos, oneshot::Receiver<Result<GenerationOutput>>)>,
    light_tx: mpsc::Sender<LightJob>,
    pub light_rx: tokio::sync::mpsc::UnboundedReceiver<LitChunk>,
    pub saved: BTreeMap<i64, SerializableChunkData>,
    pub lights: BTreeMap<i64, ChunkLight>,
    pub ready: std::collections::HashSet<ChunkPos>,
    pub light_ready: std::collections::HashSet<ChunkPos>,
    pub generation_skips: Vec<(String, String)>,
    pub pending_initial_spawns: BTreeSet<ChunkPos>,
    pub generation_statuses: BTreeMap<ChunkPos, ChunkStatus>,
    pub pending_light_updates: std::collections::HashMap<ChunkPos, LightDirtySections>,
    pub light_chunks_to_broadcast: ferraloom_util::collections::LongLinkedOpenHashSet,
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
        let light_pool = Arc::new(JobPool::new("ferraloom-generation-light", 1)?);
        let mut context = ferraloom_game::level::generation::WorldGenContext::load(
            resources,
            generator.clone(),
            dimension,
            seed,
            properties.boolean("generate-structures"),
        )?;
        context.unported_behavior = unported_behavior;
        let mut generation_state = GenerationState::new(Arc::new(context), light_pool);
        let mut initial_chunks = BTreeMap::new();
        let mut generation_skips = Vec::new();
        let io = if !level_data.data.contains_key("spawn") {
            let (reply, result) = mpsc::channel();
            let spawn_generator = generator.clone();
            let registry = registry.clone();
            let _task = generation.submit(move || {
                let outcome = (|| -> Result<_> {
                    let mut world = SpawnChunks {
                        generator: spawn_generator,
                        registry,
                        dimension,
                        state: generation_state,
                        io,
                        chunks: BTreeMap::new(),
                        skips: Vec::new(),
                    };
                    let generator = world.generator.clone();
                    let position = spawn::initial_spawn(
                        &mut world,
                        generator.as_ref(),
                        matches!(generator.as_ref(), ChunkGenerator::Debug(_)),
                        bonus_chest,
                    )?;
                    Ok((world.io, world.chunks, position, world.state, world.skips))
                })();
                let _ = reply.send(outcome);
            });
            let (io, chunks, position, state, skips) =
                result.recv().context("spawn worker terminated")??;
            initial_chunks = chunks;
            generation_state = state;
            generation_skips = skips;
            spawn = BlockPos::new(position[0], position[1], position[2]);
            level_data.data.insert("spawn", spawn_tag(spawn));
            level_data.data.insert("initialized", Tag::Byte(1));
            access.save_level_data(&level_data)?;
            if matches!(generator.as_ref(), ChunkGenerator::Noise(_)) {
                println!(
                    "Initial spawn terrain will continue through the chunk generation pipeline."
                );
            }
            io
        } else {
            io
        };
        let pending_initial_spawns = generation_state.pending_initial_spawns();
        let (light_tx, light_in) = mpsc::channel::<LightJob>();
        let (light_out, light_rx) = tokio::sync::mpsc::unbounded_channel();
        std::thread::Builder::new()
            .name("ferraloom-light".into())
            .spawn(move || {
                run_light_worker(
                    light_in,
                    light_out,
                    dimension.min_y >> 4,
                    (dimension.min_y + dimension.height) >> 4,
                    dimension.has_skylight,
                )
            })?;
        Ok(Self {
            access,
            level_data,
            seed,
            spawn,
            registry,
            io: Arc::new(io),
            entity_io: None,
            dimensions: preset.dimensions,
            generator: generator.clone(),
            dimension,
            initial_chunks,
            generation,
            generation_state: Some(generation_state),
            generation_work: None,
            generation_dispatcher: ThrottlingChunkTaskDispatcher::new(4),
            active_generation: BTreeMap::new(),
            running_generation: None,
            retained_generation: BTreeSet::new(),
            failed_generation: Vec::new(),
            proto_saves: VecDeque::new(),
            unloading: VecDeque::new(),
            evictions: Vec::new(),
            saving: JobPool::new("ferraloom-save", 1)?,
            pending: BTreeMap::new(),
            light_tx,
            light_rx,
            saved: BTreeMap::new(),
            lights: BTreeMap::new(),
            ready: Default::default(),
            light_ready: Default::default(),
            generation_skips,
            pending_initial_spawns,
            generation_statuses: BTreeMap::new(),
            pending_light_updates: Default::default(),
            light_chunks_to_broadcast: Default::default(),
        })
    }
    pub fn configure_live_rules(&self, level: &mut Level) {
        let data = self.generator.data();
        level.set_weather_environment(ferraloom_game::level::weather::WeatherEnvironment {
            temperatures: data.temperatures.clone(),
            biome_names: Arc::new(data.biome_names.clone()),
            biome_zoom_seed: ferraloom_worldgen::biome_manager::obfuscate_seed(self.seed),
            sea_level: self.generator.sea_level(),
        });
        let rules = level.rules_mut();
        rules.debug = matches!(self.generator.as_ref(), ChunkGenerator::Debug(_));
        rules.fast_lava = self.dimension.fast_lava;
        rules.can_have_weather = self.dimension.has_skylight && !self.dimension.has_ceiling;
    }
    pub async fn save_live_level_context(&mut self, level: &Level) -> Result<()> {
        // Startup's generated spawn-search chunks may not have entered Level.
        // Prime their actual predicates without creating another simulation RNG.
        let tags = level
            .block_tags()
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        for terrain in self.initial_chunks.values_mut() {
            let mut chunk = LevelChunk::from_sections(
                terrain.pos,
                terrain.disk.y_pos.context("saved chunk minimum section")?,
                terrain.sections.clone(),
            );
            chunk.prime_heightmaps(tags);
            write_runtime_heightmaps(&chunk, &mut terrain.disk)?;
        }
        let weather = *level.weather();
        let border = level.entity_world_border().clone();
        let path = self.access.path().join("data");
        self.saving
            .run(move || -> Result<()> {
                let saved = SavedDataStorage::new(path);
                saved.save("minecraft:weather", &mut SavedData::new(weather))?;
                saved.save(
                    ferraloom_game::level::border::SAVED_DATA_ID,
                    &mut SavedData::new(border),
                )?;
                Ok(())
            })
            .await??;
        Ok(())
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
    pub fn submit_entity_requests(
        &mut self,
        requests: Vec<EntityStorageRequest>,
        properties: &ServerProperties,
    ) -> Result<()> {
        if !requests.is_empty() {
            self.entity_io(properties)?.submit(requests)?;
        }
        Ok(())
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
    pub fn update_generation_priority(&mut self, pos: ChunkPos, level: i32) {
        self.generation_dispatcher
            .on_level_change(pos, level.clamp(0, 45));
        if level <= 44 {
            self.retained_generation.insert(pos);
        } else {
            self.retained_generation.remove(&pos);
        }
    }
    pub fn cancel_generation(&mut self, pos: ChunkPos) -> Option<u64> {
        if let Some((request, cancelled)) = self.active_generation.get(&pos) {
            let request = *request;
            if !cancelled.swap(true, Ordering::Relaxed) {
                self.pending.remove(&request);
                self.failed_generation.push((pos, request));
                self.generation_dispatcher.release(pos, true, None);
            }
            if self.running_generation != Some((request, pos)) {
                self.active_generation.remove(&pos);
            }
            return Some(request);
        }
        None
    }
    pub fn request(&mut self, request: u64, pos: ChunkPos) {
        let (reply, answer) = oneshot::channel();
        self.pending.insert(request, (pos, answer));
        let cancelled = Arc::new(AtomicBool::new(false));
        self.active_generation
            .insert(pos, (request, cancelled.clone()));
        self.generation_dispatcher.submit(
            pos,
            self.generation_dispatcher.queue_level(pos),
            GenerationRequest {
                id: request,
                pos,
                reply,
                cancelled,
            },
        );
    }
    fn start_generation(&mut self) -> Result<()> {
        if self.generation_work.is_some() {
            return Ok(());
        }
        let mut next = None;
        while let Some(dispatch) = self.generation_dispatcher.next_dispatch() {
            match dispatch {
                ChunkTaskDispatch::Batch { pos: _, mut tasks } => {
                    next = tasks.pop();
                    ensure!(tasks.is_empty(), "duplicate generation request");
                    break;
                }
                ChunkTaskDispatch::Released(_) => {
                    unreachable!("generation release has no callback")
                }
            }
        }
        let Some(request) = next else {
            return Ok(());
        };
        let mut state = self
            .generation_state
            .take()
            .context("generation state in flight")?;
        for pos in self.evictions.drain(..) {
            state.forget(pos);
        }
        self.initial_chunks.remove(&request.pos.pack());
        let registry = self.registry.clone();
        let io = self.io.clone();
        let mut retained = self.retained_generation.clone();
        for center in self.active_generation.keys() {
            for x in center.x - 11..=center.x + 11 {
                for z in center.z - 11..=center.z + 11 {
                    retained.insert(ChunkPos::new(x, z));
                }
            }
        }
        self.running_generation = Some((request.id, request.pos));
        self.generation_work = Some(self.generation.submit(move || {
            let mut result = state.generate(
                request.id,
                request.pos,
                ChunkStatus::Full,
                &registry,
                &io,
                Some(&request.cancelled),
            );
            let cleanup = if request.cancelled.load(Ordering::Relaxed) {
                state.save_proto_chunks(&registry, &io).and_then(|()| {
                    if let Ok(output) = &result {
                        io.store(request.pos, Some(output.terrain.disk.write()?))
                            .wait()?;
                    }
                    Ok(())
                })
            } else if result.is_ok() {
                state.save_unclaimed_protos(&retained, &registry, &io)
            } else {
                Ok(())
            };
            if let Ok(output) = &mut result {
                output.statuses = state.completed_statuses();
            }
            GenerationCompletion {
                state,
                result,
                request,
                cleanup,
            }
        }));
        Ok(())
    }
    fn complete_generation(&mut self, output: GenerationCompletion) -> Result<()> {
        let GenerationCompletion {
            mut state,
            mut result,
            request,
            cleanup,
        } = output;
        let pos = request.pos;
        // An unrelated unload may have completed while the cache was owned by
        // this worker. Apply it before the cache or its status snapshot returns.
        for evicted in self.evictions.drain(..) {
            state.forget(evicted);
            if let Ok(output) = &mut result {
                output.statuses.retain(|&(position, _)| position != evicted);
            }
        }
        self.generation_state = Some(state);
        self.running_generation = None;
        if self
            .active_generation
            .get(&pos)
            .is_some_and(|(id, _)| *id == request.id)
        {
            self.active_generation.remove(&pos);
        }
        self.generation_dispatcher.release(pos, false, None);
        self.generation_dispatcher.complete_batch();
        cleanup?;
        if request.cancelled.load(Ordering::Relaxed) {
            return Ok(());
        }
        let _ = request.reply.send(result);
        Ok(())
    }
    /// Pause dispatch while save captures canonical data. No worker owns live
    /// chunks; pending requests resume at the next normal integration poll.
    pub async fn finish_generation(&mut self) -> Result<()> {
        if let Some(work) = self.generation_work.take() {
            self.complete_generation(work.await?)?;
        }
        Ok(())
    }
    pub fn proto_saves_complete(&self) -> bool {
        self.proto_saves.is_empty()
    }
    pub fn forget_proto_holder(&mut self, pos: ChunkPos) {
        self.generation_statuses.remove(&pos);
        if let Some(state) = self.generation_state.as_mut() {
            state.forget(pos);
        } else {
            self.evictions.push(pos);
        }
    }
    pub fn generation_pins(&self, pos: ChunkPos) -> bool {
        self.active_generation
            .keys()
            .any(|center| center.x.abs_diff(pos.x).max(center.z.abs_diff(pos.z)) <= 11)
    }
    pub fn schedule_unload(
        &mut self,
        level: &mut Level,
        pos: ChunkPos,
        epoch: u64,
        game_time: i64,
    ) -> Result<()> {
        if self.generation_pins(pos) || self.unloading.iter().any(|job| job.pos == pos) {
            return Ok(());
        }
        level
            .try_mark_chunk_saved(pos)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        let snapshot = match self.capture_chunk(level, pos, game_time) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                level
                    .mark_chunk_unsaved(pos)
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                return Err(error);
            }
        };
        let registry = self.registry.clone();
        let io = self.io.clone();
        let saving = self.saving.submit(move || -> Result<()> {
            io.store(pos, Some(snapshot.encode(&registry)?)).wait()?;
            Ok(())
        });
        self.unloading.push_back(ChunkUnload { pos, epoch, saving });
        Ok(())
    }
    pub fn poll_unloads(&mut self) -> Result<Vec<(ChunkPos, u64, Result<()>)>> {
        let mut done = Vec::new();
        while let Some(job) = self.unloading.front_mut() {
            match job.saving.try_recv() {
                Ok(result) => {
                    let job = self.unloading.pop_front().unwrap();
                    done.push((job.pos, job.epoch, result));
                }
                Err(oneshot::error::TryRecvError::Empty) => break,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(done)
    }
    pub fn finish_unload(&mut self, pos: ChunkPos) -> Result<()> {
        self.ready.remove(&pos);
        self.light_ready.remove(&pos);
        self.saved.remove(&pos.pack());
        self.lights.remove(&pos.pack());
        self.generation_statuses.remove(&pos);
        if let Some(state) = self.generation_state.as_mut() {
            state.forget(pos);
        } else {
            self.evictions.push(pos);
        }
        self.light_tx.send(LightJob::Unload { pos })?;
        Ok(())
    }
    pub async fn close_chunk_storage(&mut self) -> Result<()> {
        self.finish_generation().await?;
        while let Some(job) = self.unloading.pop_front() {
            job.saving.await??;
        }
        Arc::get_mut(&mut self.io)
            .context("chunk IO still owned by generation")?
            .shutdown()
            .await?;
        Ok(())
    }
    pub fn refresh_generation_snapshots(&mut self, level: &Level) {
        if !self.pending.is_empty()
            && let Some(state) = &mut self.generation_state
        {
            state.refresh_full_chunks(level);
        }
    }
    /// Poll at ServerChunkCache's integration point, strictly by request ordinal.
    pub fn poll_terrain(&mut self) -> Result<Vec<(u64, Terrain)>> {
        if let Some(work) = &mut self.generation_work {
            match work.try_recv() {
                Ok(output) => {
                    self.generation_work = None;
                    self.complete_generation(output)?;
                }
                Err(oneshot::error::TryRecvError::Empty) => {}
                Err(error) => return Err(error.into()),
            }
        }
        let mut context = TaskContext::from_waker(Waker::noop());
        while let Some(task) = self.proto_saves.front_mut() {
            let Poll::Ready(result) = Pin::new(task).poll(&mut context) else {
                break;
            };
            self.proto_saves.pop_front();
            result?;
        }
        let mut done = Vec::new();
        while let Some((&request, _)) = self.pending.first_key_value() {
            let (position, answer) = self.pending.get_mut(&request).unwrap();
            let position = *position;
            match answer.try_recv() {
                Ok(result) => {
                    self.pending.remove(&request);
                    if result.is_err() {
                        self.failed_generation.push((position, request));
                    }
                    let output = result?;
                    if output.initial_mob_spawning_pending {
                        self.pending_initial_spawns.insert(output.terrain.pos);
                    }
                    self.generation_skips.extend(output.skips);
                    self.generation_statuses.extend(output.statuses);
                    for (pos, structures) in output.full_structures {
                        if let Some(disk) = self.saved.get_mut(&pos.pack()) {
                            disk.structures = Some(structures);
                        }
                    }
                    for (pos, disk) in output.neighbors {
                        self.proto_saves
                            .push_back(self.io.store(pos, Some(disk.write()?)));
                    }
                    done.push((request, output.terrain));
                }
                Err(oneshot::error::TryRecvError::Empty) => break,
                Err(error) => return Err(error.into()),
            }
        }
        // Integrate completed results before capturing the next job's live
        // imposter snapshots, including post-processing writes and heightmaps.
        if done.is_empty() {
            self.start_generation()?;
        }
        Ok(done)
    }
    pub fn queue_block_light_changes(
        &self,
        level: &Level,
        effects: Vec<ferraloom_game::level::effects::LevelEffect>,
    ) -> Result<()> {
        use ferraloom_game::level::effects::LevelEffect;
        if effects.is_empty() {
            return Ok(());
        }
        let mut positions = std::collections::HashSet::new();
        let mut chunks = Vec::new();
        let mut changes = Vec::new();
        for effect in effects {
            let (pos, change) = match effect {
                LevelEffect::CheckLight(pos) => {
                    (ChunkPos::containing(pos), LightMutation::CheckBlock(pos))
                }
                LevelEffect::SectionEmptiness {
                    chunk,
                    section_y,
                    empty,
                } => (
                    chunk,
                    LightMutation::SectionStatus(
                        SectionPos::new(chunk.x, section_y, chunk.z),
                        empty,
                    ),
                ),
                _ => unreachable!("only light effects enter the light worker"),
            };
            if positions.insert(pos) {
                let chunk = level.chunk(pos).context("light change in missing chunk")?;
                chunks.push((pos, chunk.sections().cloned().collect()));
            }
            changes.push(change);
        }
        self.light_tx.send(LightJob::Changed { chunks, changes })?;
        Ok(())
    }
    pub fn integrate(&mut self, request: u64, terrain: Terrain) -> Result<LevelChunk> {
        ensure!(
            terrain
                .disk
                .status
                .as_ref()
                .is_some_and(|s| s.to_string_lossy() == "minecraft:full"),
            "promotion before FULL"
        );
        self.light_tx.send(LightJob::Initialize {
            request,
            pos: terrain.pos,
            sections: terrain.sections.clone(),
            saved_light: Some(Box::new(terrain.disk.clone())),
            neighbors: terrain.light_neighbors,
        })?;
        let chunk = ferraloom_game::level::generation::promote(
            terrain.pos,
            terrain.sections,
            &terrain.disk,
        )
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
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
        chunk.save_pending_generation_data(&mut disk);
        disk.inhabited_time = Some(chunk.inhabited_time());
        write_runtime_heightmaps(chunk, &mut disk)?;
        Ok(ChunkSaveSnapshot {
            pos,
            sections: chunk.sections().cloned().collect(),
            disk,
            light: self.lights.get(&pos.pack()).cloned(),
            game_time,
            light_correct: self.light_ready.contains(&pos),
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
                light_correct: terrain.disk.is_light_on == Some(true),
                disk: terrain.disk,
                light: None,
                game_time,
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
fn write_runtime_heightmaps(chunk: &LevelChunk, disk: &mut SerializableChunkData) -> Result<()> {
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
            LongArray(heightmap_in_dimension(
                chunk
                    .heightmap(map)
                    .context("full chunk heightmaps not primed")?,
                chunk.min_y(),
                chunk.max_y() - chunk.min_y(),
            )),
        );
    }
    Ok(())
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
        let min_section = self.disk.y_pos.context("saved chunk minimum section")?;
        let max_section = min_section + self.sections.len() as i32;
        let mut sections = Vec::new();
        for (index, runtime) in self.sections.iter().enumerate() {
            let y = min_section + index as i32;
            let mut section = existing.remove(&(y as i8)).unwrap_or_default();
            section.y = Some(y as i8);
            section.update_from_runtime(runtime, registry)?;
            if let Some(light) = &self.light {
                section.sky_light = light
                    .sky
                    .get(&y)
                    .map(|v| ByteArray(v.iter().map(|v| *v as i8).collect()));
                section.block_light = light
                    .block
                    .get(&y)
                    .map(|v| ByteArray(v.iter().map(|v| *v as i8).collect()));
            }
            sections.push(section);
        }
        for y in [min_section - 1, max_section] {
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
    surface_heights_in_dimension(sections, -64)
}
pub(crate) fn surface_heights_in_dimension(
    sections: &[LevelChunkSection],
    min_y: i32,
) -> [i32; 256] {
    std::array::from_fn(|index| {
        let x = (index & 15) as u8;
        let z = (index >> 4) as u8;
        for y in (min_y..min_y + sections.len() as i32 * 16).rev() {
            if !matches!(
                sections[((y - min_y) / 16) as usize]
                    .get_block_state(x, (y & 15) as u8, z)
                    .block(),
                Block::AIR | Block::CAVE_AIR | Block::VOID_AIR
            ) {
                return y + 1;
            }
        }
        min_y
    })
}
/// Port of Heightmap's padded SimpleBitStorage for an overworld height.
pub fn heightmap(heights: &[i32; 256]) -> Vec<i64> {
    heightmap_in_dimension(heights, -64, 384)
}
fn heightmap_in_dimension(heights: &[i32; 256], min_y: i32, height: i32) -> Vec<i64> {
    let bits = 32 - (height as u32).leading_zeros();
    let per_word = 64 / bits as usize;
    let mut words = vec![0u64; 256usize.div_ceil(per_word)];
    for (index, &y) in heights.iter().enumerate() {
        words[index / per_word] |= ((y - min_y) as u64) << ((index % per_word) * bits as usize);
    }
    words.into_iter().map(|word| word as i64).collect()
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

#[cfg(test)]
mod light_worker_tests {
    use super::*;

    #[test]
    fn block_light_deltas_update_real_sections_without_completing_generation() {
        let (send, receive) = mpsc::channel();
        let (reply, mut results) = tokio::sync::mpsc::unbounded_channel();
        let worker = std::thread::spawn(move || run_light_worker(receive, reply, -4, 20, true));
        let pos = ChunkPos::new(0, 0);
        let biome = BiomeId(0);
        let mut sections: Vec<_> = (0..24)
            .map(|_| {
                LevelChunkSection::new(
                    Strategy::block_states(),
                    Strategy::biomes(Arc::new(IdMap::new(vec![biome]).unwrap())),
                    biome,
                )
            })
            .collect();
        send.send(LightJob::Initialize {
            request: 7,
            pos,
            sections: sections.clone(),
            saved_light: None,
            neighbors: Vec::new(),
        })
        .unwrap();
        assert_eq!(results.blocking_recv().unwrap().initialized, Some((7, pos)));
        let source = BlockPos::new(8, 64, 8);
        sections[8].set_block_state(8, 0, 8, Block::GLOWSTONE.default_state());
        send.send(LightJob::Changed {
            chunks: vec![(pos, sections.clone())],
            changes: vec![
                LightMutation::SectionStatus(SectionPos::new(0, 4, 0), false),
                LightMutation::CheckBlock(source),
            ],
        })
        .unwrap();
        let lit = results.blocking_recv().unwrap();
        assert_eq!(lit.initialized, None);
        let light = &lit.updates[&pos.pack()];
        assert_eq!(light.block[&4][68] & 15, 15);
        sections[8].set_block_state(8, 0, 8, Block::AIR.default_state());
        send.send(LightJob::Changed {
            chunks: vec![(pos, sections)],
            changes: vec![
                LightMutation::SectionStatus(SectionPos::new(0, 4, 0), true),
                LightMutation::CheckBlock(source),
            ],
        })
        .unwrap();
        let dark = results.blocking_recv().unwrap();
        assert_eq!(dark.initialized, None);
        assert_eq!(
            dark.updates[&pos.pack()]
                .block
                .get(&4)
                .map_or(0, |data| data[68] & 15),
            0
        );
        drop(send);
        worker.join().unwrap();
    }
}

#[cfg(test)]
mod generation_order_tests {
    use super::*;

    #[test]
    fn saving_uses_dimension_sections_and_retains_generation_light_until_live_attachment() {
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder vanilla jar");
        let resources = ResourceManager::new(vec![
            ferraloom_data::datapack::Pack::vanilla_jar(Path::new(&jar)).unwrap(),
        ]);
        let registry = RegistryAdapter::new(&RegistrySet::load(&resources).unwrap()).unwrap();
        let biome = BiomeId(0);
        let section = LevelChunkSection::new(
            Strategy::block_states(),
            Strategy::biomes(Arc::new(IdMap::new(vec![biome]).unwrap())),
            biome,
        );
        let disk = SerializableChunkData {
            y_pos: Some(2),
            status: Some("minecraft:full".into()),
            sections: Some(vec![storage::ChunkSection {
                y: Some(2),
                sky_light: Some(ByteArray(vec![-1; 2048])),
                ..Default::default()
            }]),
            ..Default::default()
        };
        for attached in [false, true] {
            let snapshot = ChunkSaveSnapshot {
                pos: ChunkPos::new(0, 0),
                sections: vec![section.clone(); 2],
                disk: disk.clone(),
                light: attached.then(|| ChunkLight {
                    sky: BTreeMap::from([(1, vec![255; 2048]), (4, vec![255; 2048])]),
                    ..Default::default()
                }),
                game_time: 7,
                light_correct: true,
            };
            let saved = SerializableChunkData::parse(snapshot.encode(&registry).unwrap()).unwrap();
            let sections = saved.sections.unwrap();
            let ys = sections.iter().map(|s| s.y.unwrap()).collect::<Vec<_>>();
            assert_eq!(
                ys,
                if attached {
                    vec![1, 2, 3, 4]
                } else {
                    vec![2, 3]
                }
            );
            assert_eq!(saved.is_light_on, Some(true));
            assert_eq!(
                sections
                    .iter()
                    .find(|s| s.y == Some(2))
                    .unwrap()
                    .sky_light
                    .is_some(),
                !attached
            );
        }
        let heights = heightmap_in_dimension(&[64; 256], 32, 32);
        let packed = ferraloom_chunk::SimpleBitStorage::from_raw(
            6,
            256,
            heights.into_iter().map(|word| word as u64).collect(),
        )
        .unwrap();
        assert_eq!(packed.get(255), 32);
    }

    #[tokio::test]
    async fn priority_completion_keeps_statuses_skips_and_chunks_in_request_order() {
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder vanilla jar");
        let resources = ResourceManager::new(vec![
            ferraloom_data::datapack::Pack::vanilla_jar(Path::new(&jar)).unwrap(),
        ]);
        let registries = RegistrySet::load(&resources).unwrap();
        let mut properties = ServerProperties::from_values(Default::default());
        properties.set("level-type", "minecraft:debug_all_block_states");
        let directory = tempfile::tempdir().unwrap();
        let mut world = World::open(
            directory.path(),
            &properties,
            &resources,
            &registries,
            ferraloom_game::block::UnportedBehavior::Strict,
        )
        .unwrap();
        let output = |pos, unit: &str| GenerationOutput {
            terrain: Terrain {
                pos,
                sections: Vec::new(),
                surface: [0; 256],
                disk: SerializableChunkData::default(),
                light_neighbors: Vec::new(),
            },
            neighbors: Vec::new(),
            skips: vec![(unit.into(), "pending callback".into())],
            statuses: vec![(pos, ChunkStatus::Full)],
            full_structures: Vec::new(),
            initial_mob_spawning_pending: true,
        };
        let first = ChunkPos::new(0, 0);
        let second = ChunkPos::new(1, 0);
        let (first_reply, first_result) = oneshot::channel();
        let (second_reply, second_result) = oneshot::channel();
        world.pending.insert(0, (first, first_result));
        world.pending.insert(1, (second, second_result));
        assert!(second_reply.send(Ok(output(second, "second"))).is_ok());
        assert!(world.poll_terrain().unwrap().is_empty());
        assert!(world.generation_statuses.is_empty());
        assert!(world.generation_skips.is_empty());
        assert!(world.pending_initial_spawns.is_empty());
        assert!(first_reply.send(Ok(output(first, "first"))).is_ok());
        let chunks = world.poll_terrain().unwrap();
        assert_eq!(chunks.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [0, 1]);
        assert_eq!(world.generation_skips[0].0, "first");
        assert_eq!(world.generation_skips[1].0, "second");
        assert_eq!(world.generation_statuses[&second], ChunkStatus::Full);
        assert!(world.pending_initial_spawns.contains(&second));
        world.close_chunk_storage().await.unwrap();
    }
}
