//! World snapshots cross worker boundaries; only tick integration mutates Level.
use crate::{
    generation::{
        GenerationCompletion, GenerationIdentity, GenerationOutput, GenerationRequest,
        GenerationRevision, GenerationState, ProtoStructureSnapshot, ReadyGeneration,
        StructureSnapshot,
    },
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
use ferraloom_protocol::play::Difficulty;
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
    structure::pipeline::ChunkStructures,
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

mod root;
use root::StartupSpawn;
pub use root::{WorldRoot, WorldServices};

impl std::ops::Deref for World {
    type Target = WorldServices;
    fn deref(&self) -> &Self::Target {
        &self.services
    }
}

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
    pub(crate) revision: GenerationRevision,
    pub sections: Vec<LevelChunkSection>,
    pub disk: SerializableChunkData,
}
pub struct Terrain {
    pub pos: ChunkPos,
    pub(crate) revision: GenerationRevision,
    pub sections: Vec<LevelChunkSection>,
    pub disk: SerializableChunkData,
    /// Already reconstructed at disk load; never decode monuments again at FULL.
    pub structures: ChunkStructures,
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
    pub initialized: Option<GenerationLightInitialization>,
    pub updates: BTreeMap<i64, ChunkLight>,
    pub dirty: BTreeMap<i64, LightDirtySections>,
}
/// The actor may already hold a newer neighbor import than the center request.
/// Report the installed generation revision, not the incoming request revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenerationLightInitialization {
    pub request: u64,
    pub pos: ChunkPos,
    pub(crate) revision: GenerationRevision,
}
enum LightJob {
    Initialize {
        request: u64,
        revision: GenerationRevision,
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
#[derive(Clone, Copy)]
struct ImportedGenerationLight {
    revision: GenerationRevision,
    lighted: bool,
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
    let mut imported = BTreeMap::<i64, ImportedGenerationLight>::new();
    while let Ok(job) = light_in.recv() {
        let context = match &job {
            LightJob::Initialize { request, pos, .. } => Some((*request, *pos)),
            _ => None,
        };
        let _timing = crate::generation::profile::Span::new("live_light", context);
        let (initialized, changes) = match job {
            LightJob::Initialize {
                request,
                revision,
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
                    .filter(|neighbor| {
                        !full_chunks.contains(&neighbor.pos)
                            && imported
                                .get(&neighbor.pos.pack())
                                .is_none_or(|known| known.revision < neighbor.revision)
                    })
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
                    let lighted = neighbor.disk.is_light_on == Some(true);
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
                            .initialize_light(&snapshots, neighbor.pos, lighted)
                            .sections,
                    );
                    imported.insert(
                        neighbor.pos.pack(),
                        ImportedGenerationLight {
                            revision: neighbor.revision,
                            lighted,
                        },
                    );
                }
                let known = imported.get(&pos.pack()).copied();
                let import = known.is_none_or(|known| known.revision < revision);
                let lighted = if import {
                    saved_light
                        .as_ref()
                        .is_some_and(|disk| disk.is_light_on == Some(true))
                } else {
                    known.unwrap().lighted
                };
                if import && let Some(disk) = saved_light {
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
                imported.insert(
                    pos.pack(),
                    ImportedGenerationLight {
                        revision: known.map_or(revision, |known| known.revision.max(revision)),
                        lighted: true,
                    },
                );
                (
                    Some(GenerationLightInitialization {
                        request,
                        pos,
                        revision: known.map_or(revision, |known| known.revision.max(revision)),
                    }),
                    changes,
                )
            }
            LightJob::Unload { pos } => {
                imported.remove(&pos.pack());
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
        if let Some(GenerationLightInitialization { pos, .. }) = initialized {
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
        if self.chunks.contains_key(&pos.to_long()) {
            return Ok(());
        }
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
        for snapshot in generated.full_structures {
            if let Some(terrain) = self.chunks.get_mut(&snapshot.pos.pack())
                && terrain.revision < snapshot.revision
            {
                terrain.disk.structures = Some(snapshot.data);
                terrain.structures = snapshot.structures;
                terrain.revision = snapshot.revision;
            }
        }
        self.skips.extend(generated.skips);
        self.chunks.insert(pos.to_long(), generated.terrain);
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
    request_id: u64,
    saving: oneshot::Receiver<Result<()>>,
}
/// Dimension-owned chunk/generation state; global metadata belongs to WorldRoot.
pub struct World {
    services: Arc<WorldServices>,
    pub dimension_key: ferraloom_protocol::Identifier,
    initial_difficulty: Difficulty,
    pub io: Arc<IoWorker>,
    entity_io: Option<EntityIo>,
    generator: Arc<ChunkGenerator>,
    dimension: DimensionType,
    initial_chunks: BTreeMap<i64, Terrain>,
    structure_snapshots: BTreeMap<i64, StructureSnapshot>,
    pending_structure_publications: VecDeque<ChunkPos>,
    generation_save_revisions: BTreeMap<i64, GenerationRevision>,
    proto_biomes: BTreeMap<i64, crate::generation::ProtoBiomeSnapshot>,
    proto_structures: BTreeMap<i64, ProtoStructureSnapshot>,
    generation: Arc<JobPool>,
    generation_state: Option<GenerationState>,
    generation_work: Option<oneshot::Receiver<GenerationCompletion>>,
    progress_waker: Waker,
    generation_dispatcher: ThrottlingChunkTaskDispatcher<GenerationRequest>,
    active_generation: BTreeMap<i64, GenerationActivity>,
    running_generation: Option<(u64, ChunkPos)>,
    settled_generation: VecDeque<u64>,
    retained_generation: BTreeSet<i64>,
    pub failed_generation: Vec<(ChunkPos, u64)>,
    proto_saves: VecDeque<storage::IoTask<()>>,
    cancelled_outputs: Vec<GenerationOutput>,
    unloading: VecDeque<ChunkUnload>,
    evictions: Vec<ChunkPos>,
    saving: Arc<JobPool>,
    pending: BTreeMap<u64, (ChunkPos, oneshot::Receiver<Result<GenerationOutput>>)>,
    completed_generation: VecDeque<ReadyGeneration>,
    pub(crate) exclusive_ready_tasks: VecDeque<GenerationIdentity>,
    pub(crate) full_promotions:
        BTreeMap<u64, crate::server::generation_completion::PromotionContinuation>,
    light_tx: mpsc::Sender<LightJob>,
    pub light_rx: tokio::sync::mpsc::UnboundedReceiver<LitChunk>,
    pub saved: BTreeMap<i64, SerializableChunkData>,
    pub lights: BTreeMap<i64, ChunkLight>,
    pub ready: std::collections::HashSet<ChunkPos>,
    pub light_ready: std::collections::HashSet<ChunkPos>,
    pub generation_skips: Vec<(String, String)>,
    pub pending_initial_spawns: BTreeSet<i64>,
    pub generation_statuses: BTreeMap<i64, ChunkStatus>,
    pub pending_light_updates: std::collections::HashMap<ChunkPos, LightDirtySections>,
    pub light_chunks_to_broadcast: ferraloom_util::collections::LongLinkedOpenHashSet,
}
struct GenerationActivity {
    task: u64,
    cancelled: Arc<AtomicBool>,
    radius: i32,
}
pub(crate) struct BlockLightChanges(LightJob);
pub(crate) struct ChunkStorageDrain {
    pub proto: Vec<storage::IoTask<()>>,
    pub chunks: Vec<oneshot::Receiver<Result<()>>>,
}
/// The entity region worker is distinct from block-chunk storage. Completion
/// polling returns owned responses; the game integrates its request ordinals.
struct EntityIo {
    worker: IoWorker,
    loads: VecDeque<(EntityLoadRequestId, storage::IoTask<Option<CompoundTag>>)>,
    stores: VecDeque<storage::IoTask<()>>,
}
/// Owned IO result popped before any entity decode, registration or callback.
pub(crate) enum EntityIoCompletion {
    Load(EntityLoadRequestId, storage::Result<Option<CompoundTag>>),
    Store(storage::Result<()>),
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
        while let Some(completion) = self.next_completion(&mut context) {
            match completion {
                EntityIoCompletion::Load(id, result) => {
                    level.complete_entity_chunk_load(id, result)?;
                }
                EntityIoCompletion::Store(result) => result?,
            }
        }
        Ok(())
    }
    fn next_completion(&mut self, context: &mut TaskContext<'_>) -> Option<EntityIoCompletion> {
        if let Some((id, task)) = self.loads.front_mut()
            && let Poll::Ready(result) = Pin::new(task).poll(&mut *context)
        {
            let id = *id;
            self.loads.pop_front();
            return Some(EntityIoCompletion::Load(id, result));
        }
        if let Some(task) = self.stores.front_mut()
            && let Poll::Ready(result) = Pin::new(task).poll(context)
        {
            self.stores.pop_front();
            return Some(EntityIoCompletion::Store(result));
        }
        None
    }
}
impl World {
    /// Validate createLevels' dimension/service binding before a runtime
    /// installs callbacks or attaches any progress queues.
    pub(crate) fn validate_dimension_binding(
        &self,
        dimension: &ferraloom_protocol::Identifier,
    ) -> Result<()> {
        ensure!(
            dimension == &self.dimension_key,
            "generation runtime dimension differs from its World"
        );
        let stem = self.services.stem(dimension)?;
        ensure!(
            Arc::ptr_eq(&self.generator, &stem.generator),
            "World generator differs from its registered stem"
        );
        let properties = |value: DimensionType| {
            (
                value.min_y,
                value.height,
                value.logical_height,
                value.has_ceiling,
                value.has_skylight,
                value.fast_lava,
                value.coordinate_scale.to_bits(),
            )
        };
        ensure!(
            properties(self.dimension) == properties(stem.dimension),
            "World dimension properties differ from its registered stem"
        );
        ensure!(
            Arc::ptr_eq(&self.generation, &self.services.generation)
                && Arc::ptr_eq(&self.saving, &self.services.saving),
            "World worker services differ from its root services"
        );
        Ok(())
    }
    /// Construct one dimension's owned runtime, returning bootstrap spawn data
    /// for the sole PrimaryLevelData owner before the live level is published.
    fn open_dimension(
        services: Arc<WorldServices>,
        dimension_key: ferraloom_protocol::Identifier,
        properties: &ServerProperties,
        initialize_spawn: bool,
        initial_difficulty: Difficulty,
    ) -> Result<(Self, Option<StartupSpawn>)> {
        let stem = services.stem(&dimension_key)?;
        let dimension = stem.dimension;
        let generator = stem.generator.clone();
        let access = services.access.clone();
        let seed = services.seed;
        let mut startup_spawn = None;
        let version = match properties.get("region-file-compression") {
            "none" => storage::RegionFileVersion::None,
            "lz4" => storage::RegionFileVersion::Lz4,
            _ => storage::RegionFileVersion::Deflate,
        };
        let io = IoWorker::new(
            access.dimension(dimension_key.as_str())?.region,
            version,
            properties.boolean("sync-chunk-writes"),
        )?;
        let registry = services.registry.clone();
        let generation = services.generation.clone();
        let light_pool = services.light_pool.clone();
        let mut context = ferraloom_game::level::generation::WorldGenContext::load(
            &services.resources,
            generator.clone(),
            dimension,
            seed,
            services.generate_structures,
            services.dimensions.environment.clone(),
            stem.environment.clone(),
        )?;
        context.unported_behavior = services.unported_behavior;
        let mut generation_state = GenerationState::new(Arc::new(context), light_pool);
        let mut initial_chunks = BTreeMap::new();
        let mut generation_skips = Vec::new();
        let io = if initialize_spawn {
            let (reply, result) = mpsc::channel();
            let spawn_generator = generator.clone();
            let bonus_chest = services.bonus_chest;
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
            startup_spawn = Some(StartupSpawn {
                position: BlockPos::new(position[0], position[1], position[2]),
            });
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
        let proto_structures = generation_state
            .structure_versions(&BTreeSet::new())
            .into_iter()
            .map(|snapshot| (snapshot.pos.pack(), snapshot))
            .collect();
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
        let saving = services.saving.clone();
        Ok((
            Self {
                services,
                dimension_key,
                initial_difficulty,
                io: Arc::new(io),
                entity_io: None,
                generator: generator.clone(),
                dimension,
                structure_snapshots: initial_chunks
                    .iter()
                    .filter_map(|(&key, terrain)| {
                        terrain.disk.structures.clone().map(|data| {
                            (
                                key,
                                StructureSnapshot {
                                    pos: terrain.pos,
                                    revision: terrain.revision,
                                    data,
                                    structures: terrain.structures.clone(),
                                    unattached_disk: None,
                                },
                            )
                        })
                    })
                    .collect(),
                initial_chunks,
                pending_structure_publications: VecDeque::new(),
                generation_save_revisions: BTreeMap::new(),
                proto_biomes: BTreeMap::new(),
                proto_structures,
                generation,
                generation_state: Some(generation_state),
                generation_work: None,
                progress_waker: Waker::noop().clone(),
                generation_dispatcher: ThrottlingChunkTaskDispatcher::new(4),
                active_generation: BTreeMap::new(),
                running_generation: None,
                settled_generation: VecDeque::new(),
                retained_generation: BTreeSet::new(),
                failed_generation: Vec::new(),
                proto_saves: VecDeque::new(),
                cancelled_outputs: Vec::new(),
                unloading: VecDeque::new(),
                evictions: Vec::new(),
                saving,
                pending: BTreeMap::new(),
                completed_generation: VecDeque::new(),
                exclusive_ready_tasks: VecDeque::new(),
                full_promotions: BTreeMap::new(),
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
            },
            startup_spawn,
        ))
    }
    pub fn configure_live_rules(&self, level: &mut Level) {
        level.set_has_skylight(self.dimension.has_skylight);
        let data = self.generator.data();
        level.set_item_tags(data.item_tags.clone());
        level.set_weather_environment(ferraloom_game::level::weather::WeatherEnvironment {
            temperatures: data.temperatures.clone(),
            biome_names: Arc::new(data.biome_names.clone()),
            biome_zoom_seed: ferraloom_worldgen::biome_manager::obfuscate_seed(self.seed),
            sea_level: self.generator.sea_level(),
        });
        let rules = level.rules_mut();
        rules.debug = self.is_debug_world();
        rules.fast_lava = self.dimension.fast_lava;
        rules.can_have_weather = self.dimension.has_skylight && !self.dimension.has_ceiling;
    }
    /// Capture the dimension's actual generator, configured feature registry and
    /// WorldOptions seed before constructing the concrete live level.
    pub fn level_context(
        &self,
        resources: &ResourceManager,
    ) -> Result<Arc<ferraloom_game::level::LevelContext>> {
        let features = ferraloom_worldgen::feature::FeatureLoader::new(resources)?.load_all()?;
        Ok(Arc::new(
            ferraloom_game::level::LevelContext::new(
                self.seed,
                self.generator.sea_level(),
                self.generator.clone(),
                Arc::new(features),
                Arc::new(ferraloom_game::level::StructureServices::load(
                    resources,
                    self.generator.data(),
                )?),
                Arc::new(ferraloom_game::entity::damage::DamageTypeRegistry::load(
                    resources,
                )?),
                self.initial_difficulty,
            )
            .with_block_transformers(Arc::new(
                ferraloom_game::item::block_transformer::BlockTransformerRegistry::load(resources)?,
            ))
            .with_enchantments(Arc::new(
                ferraloom_game::item::enchantment::EnchantmentRegistry::load(resources)?,
            ))
            .with_mob_variants(Arc::new(
                ferraloom_game::entity::variant::MobVariants::load(resources)?,
            ))
            .with_game_event_tags(Arc::new(ferraloom_data::datapack::TagSet::load(resources)?)),
        ))
    }
    /// Attach immutable dimension layers after the sole server clock manager
    /// loads. Callers publish its actual snapshot; no local day counter exists.
    pub fn install_live_environment(
        &self,
        level: &mut Level,
        clocks: ferraloom_worldgen::environment::ClockSnapshot,
    ) -> Result<()> {
        let stem = self.services.stem(&self.dimension_key)?;
        level
            .set_environment_attributes(
                self.dimensions.environment.clone(),
                &stem.environment,
                stem.name.clone(),
                &self.generator.data().biome_names,
                clocks,
            )
            .map_err(|error| anyhow::anyhow!("{error:?}"))
    }
    /// Capture level-owned inputs before constructing the IO future. Canonical
    /// simulation state is Send, and its single-owner lazy caches need not be Sync.
    pub fn save_live_level_context(
        &mut self,
        level: &Level,
    ) -> impl std::future::Future<Output = Result<()>> + Send + use<> {
        let task = LiveLevelSaveContext::capture(level)
            .and_then(|captured| self.begin_save_live_level_context(captured));
        async move {
            task?.await??;
            Ok(())
        }
    }
    pub(crate) fn begin_save_live_level_context(
        &mut self,
        captured: LiveLevelSaveContext,
    ) -> Result<oneshot::Receiver<Result<()>>> {
        // Startup's generated spawn-search chunks may not have entered Level.
        // Prime their actual predicates without creating another simulation RNG.
        for terrain in self.initial_chunks.values_mut() {
            let mut chunk = LevelChunk::from_sections(
                terrain.pos,
                terrain.disk.y_pos.context("saved chunk minimum section")?,
                terrain.sections.clone(),
            );
            chunk.prime_heightmaps(&captured.tags);
            write_runtime_heightmaps(&chunk, &mut terrain.disk)?;
        }
        let border = captured.border;
        let path = self.access.dimension(self.dimension_key.as_str())?.data;
        Ok(self.saving.submit(move || -> Result<()> {
            let saved = SavedDataStorage::new(path);
            saved.save(
                ferraloom_game::level::border::SAVED_DATA_ID,
                &mut SavedData::new(border),
            )?;
            Ok(())
        }))
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
                    self.access.dimension(self.dimension_key.as_str())?.entities,
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
    pub(crate) fn take_entity_io_completion(
        &mut self,
        properties: &ServerProperties,
    ) -> Result<Option<EntityIoCompletion>> {
        let waker = self.progress_waker.clone();
        let mut context = TaskContext::from_waker(&waker);
        Ok(self.entity_io(properties)?.next_completion(&mut context))
    }

    /// Returned futures own their operation and never borrow the runtime owner.
    pub(crate) fn synchronize_entities(
        &mut self,
        properties: &ServerProperties,
        flush: bool,
    ) -> Result<storage::IoTask<()>> {
        Ok(self.entity_io(properties)?.worker.synchronize(flush))
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
    pub(crate) fn begin_close_entity_storage(&mut self) -> Option<storage::IoTask<()>> {
        self.entity_io.as_mut().map(|io| io.worker.shutdown())
    }
    pub fn update_generation_priority(&mut self, pos: ChunkPos, level: i32) {
        self.generation_dispatcher
            .on_level_change(pos, level.clamp(0, 45));
        if level <= 44 {
            self.retained_generation.insert(pos.pack());
        } else {
            self.retained_generation.remove(&pos.pack());
        }
    }
    pub fn cancel_generation(&mut self, pos: ChunkPos) -> Option<u64> {
        if let Some(active) = self.active_generation.get(&pos.pack()) {
            let request = active.task;
            if !active.cancelled.swap(true, Ordering::Relaxed) {
                if let Some((_, mut answer)) = self.pending.remove(&request)
                    && let Ok(Ok(output)) = answer.try_recv()
                {
                    self.cancelled_outputs.push(output);
                }
                if let Some(index) = self
                    .completed_generation
                    .iter()
                    .position(|completion| completion.request == request)
                    && let Some(completion) = self.completed_generation.remove(index)
                    && let Ok(output) = completion.result
                {
                    self.cancelled_outputs.push(output);
                }
                self.failed_generation.push((pos, request));
                self.generation_dispatcher.release(pos, true, None);
            }
            if self.running_generation != Some((request, pos)) {
                self.active_generation.remove(&pos.pack());
                self.settled_generation.push_back(request);
            }
            return Some(request);
        }
        None
    }
    /// Task identity, rather than position, guards replacement cancellation.
    /// Claims stay held until the actual worker has returned its owned cache.
    pub(crate) fn cancel_generation_task(&mut self, pos: ChunkPos, task: u64) -> bool {
        if self
            .active_generation
            .get(&pos.pack())
            .is_none_or(|current| current.task != task)
        {
            return false;
        }
        self.cancel_generation(pos) == Some(task)
    }
    pub(crate) fn take_settled_generation(&mut self) -> Vec<u64> {
        self.settled_generation.drain(..).collect()
    }
    pub fn request(&mut self, request: u64, pos: ChunkPos) {
        self.request_status(request, pos, ChunkStatus::Full);
    }
    /// Status acquisition also uses the canonical owned worker and request order.
    pub fn request_status(&mut self, request: u64, pos: ChunkPos, status: ChunkStatus) {
        let (reply, answer) = oneshot::channel();
        self.pending.insert(request, (pos, answer));
        let cancelled = Arc::new(AtomicBool::new(false));
        let radius = ferraloom_game::level::generation::ChunkPyramid::generation()
            .step_to(status)
            .accumulated_radius_of(ChunkStatus::Empty)
            .unwrap_or(0) as i32;
        self.active_generation.insert(
            pos.pack(),
            GenerationActivity {
                task: request,
                cancelled: cancelled.clone(),
                radius,
            },
        );
        self.generation_dispatcher.submit(
            pos,
            self.generation_dispatcher.queue_level(pos),
            GenerationRequest {
                id: request,
                pos,
                status,
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
                ChunkTaskDispatch::Batch { pos, mut tasks } => {
                    // Release messages precede pending submissions in vanilla's
                    // mailbox. A canceled submission can therefore still join
                    // the retried position's batch; its owned token rejects it.
                    tasks.retain(|task| !task.cancelled.load(Ordering::Relaxed));
                    if tasks.is_empty() {
                        self.generation_dispatcher.release(pos, false, None);
                        self.generation_dispatcher.complete_batch();
                        continue;
                    }
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
        for (&key, active) in &self.active_generation {
            let center = ChunkPos::unpack(key);
            for x in center.x - active.radius..=center.x + active.radius {
                for z in center.z - active.radius..=center.z + active.radius {
                    retained.insert(ChunkPos::new(x, z).pack());
                }
            }
        }
        self.running_generation = Some((request.id, request.pos));
        self.generation_work = Some(self.generation.submit(move || {
            let _timing = crate::generation::profile::Span::new(
                "generation_job",
                Some((request.id, request.pos)),
            )
            .status(request.status);
            let mut result = state.generate(
                request.id,
                request.pos,
                request.status,
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
        self.settled_generation.push_back(request.id);
        self.generation_dispatcher.release(pos, false, None);
        self.generation_dispatcher.complete_batch();
        cleanup?;
        if request.cancelled.load(Ordering::Relaxed) {
            if let Ok(output) = result {
                self.cancelled_outputs.push(output);
            }
            if self
                .active_generation
                .get(&pos.pack())
                .is_some_and(|active| active.task == request.id)
            {
                self.active_generation.remove(&pos.pack());
            }
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
        self.proto_saves.is_empty() && self.cancelled_outputs.is_empty()
    }
    pub fn forget_proto_holder(&mut self, pos: ChunkPos) {
        self.proto_biomes.remove(&pos.pack());
        self.proto_structures.remove(&pos.pack());
        self.pending_structure_publications
            .retain(|&queued| queued != pos);
        self.full_promotions
            .retain(|_, continuation| continuation.identity.pos != pos);
        self.generation_statuses.remove(&pos.pack());
        self.structure_snapshots.remove(&pos.pack());
        self.generation_save_revisions.remove(&pos.pack());
        if let Some(state) = self.generation_state.as_mut() {
            state.forget(pos);
        } else {
            self.evictions.push(pos);
        }
    }
    /// Port of ChunkHolder#isReadyForSaving: generation claims and the FULL,
    /// ticking and entity range futures must all have released their inputs.
    pub fn chunk_ready_for_saving(
        &self,
        holder: &ferraloom_game::level::chunk_holder::ChunkHolder,
    ) -> bool {
        !self.generation_pins(holder.position)
            && holder.generation.generation_references_released()
            && holder.pending_full_status().is_none()
            && (holder.generation.latest_completed_status() != Some(ChunkStatus::Full)
                || self.light_ready.contains(&holder.position))
            && (holder.requested_full_status()
                < ferraloom_game::level::distance_manager::FullChunkStatus::BlockTicking
                || self.ready.contains(&holder.position))
    }
    pub fn generation_pins(&self, pos: ChunkPos) -> bool {
        self.active_generation.iter().any(|(&key, active)| {
            let center = ChunkPos::unpack(key);
            center.x.abs_diff(pos.x).max(center.z.abs_diff(pos.z)) <= active.radius as u32
        })
    }
    pub fn schedule_unload(
        &mut self,
        level: &mut Level,
        pos: ChunkPos,
        request_id: u64,
        game_time: i64,
    ) -> Result<()> {
        if self.generation_pins(pos) || self.unloading.iter().any(|job| job.pos == pos) {
            return Ok(());
        }
        let snapshot = self.capture_chunk_for_save(level, pos, game_time)?;
        self.schedule_unload_snapshot(pos, request_id, snapshot)
    }
    pub(crate) fn unload_pending(&self, pos: ChunkPos) -> bool {
        self.unloading.iter().any(|job| job.pos == pos)
    }
    pub(crate) fn schedule_unload_snapshot(
        &mut self,
        pos: ChunkPos,
        holder_epoch: u64,
        snapshot: ChunkSaveSnapshot,
    ) -> Result<()> {
        let registry = self.registry.clone();
        let io = self.io.clone();
        let saving = self.saving.submit(move || -> Result<()> {
            io.store(pos, Some(snapshot.encode(&registry)?)).wait()?;
            Ok(())
        });
        self.unloading.push_back(ChunkUnload {
            pos,
            request_id: holder_epoch,
            saving,
        });
        Ok(())
    }
    pub fn poll_unloads(&mut self) -> Result<Vec<(ChunkPos, u64, Result<()>)>> {
        let mut done = Vec::new();
        while let Some(job) = self.unloading.front_mut() {
            match job.saving.try_recv() {
                Ok(result) => {
                    let job = self.unloading.pop_front().unwrap();
                    done.push((job.pos, job.request_id, result));
                }
                Err(oneshot::error::TryRecvError::Empty) => break,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(done)
    }
    pub fn finish_unload(&mut self, pos: ChunkPos) -> Result<()> {
        self.proto_biomes.remove(&pos.pack());
        self.proto_structures.remove(&pos.pack());
        self.pending_structure_publications
            .retain(|&queued| queued != pos);
        self.full_promotions
            .retain(|_, continuation| continuation.identity.pos != pos);
        self.ready.remove(&pos);
        self.light_ready.remove(&pos);
        self.saved.remove(&pos.pack());
        self.structure_snapshots.remove(&pos.pack());
        self.generation_save_revisions.remove(&pos.pack());
        self.lights.remove(&pos.pack());
        self.generation_statuses.remove(&pos.pack());
        if let Some(state) = self.generation_state.as_mut() {
            state.forget(pos);
        } else {
            self.evictions.push(pos);
        }
        self.light_tx.send(LightJob::Unload { pos })?;
        Ok(())
    }
    fn persist_cancelled_outputs(&mut self) -> Result<()> {
        // A cancellation can arrive after the worker finished but before its
        // ordered response was consumed. Persist that owned result before the
        // holder/cache may be released, just as for an in-flight cancellation.
        for output in std::mem::take(&mut self.cancelled_outputs) {
            for snapshot in output.full_structures {
                self.integrate_structure_snapshot(snapshot)?;
            }
            let revision = output.terrain.revision;
            for (pos, disk) in output
                .neighbors
                .into_iter()
                .chain(std::iter::once((output.terrain.pos, output.terrain.disk)))
            {
                self.store_generation_chunk(pos, revision, disk)?;
            }
        }
        Ok(())
    }
    pub async fn close_chunk_storage(&mut self) -> Result<()> {
        self.finish_generation().await?;
        let drain = self.prepare_close_chunk_storage()?;
        for save in drain.proto {
            save.await?;
        }
        for save in drain.chunks {
            save.await??;
        }
        self.begin_close_chunk_storage()?.await?;
        Ok(())
    }
    pub(crate) fn prepare_close_chunk_storage(&mut self) -> Result<ChunkStorageDrain> {
        ensure!(
            !self.generation_running(),
            "chunk storage closed with owned generation in flight"
        );
        for continuation in std::mem::take(&mut self.full_promotions).into_values() {
            if let Some(mut input) = continuation.input {
                if let Some(failure) = continuation.publication_failure {
                    input.disk.entities = Some(failure.prepared.worldgen_entities);
                }
                self.store_generation_chunk(input.pos, input.revision, input.disk)?;
            }
        }
        for (_, (_, mut answer)) in std::mem::take(&mut self.pending) {
            if let Ok(Ok(output)) = answer.try_recv() {
                self.cancelled_outputs.push(output);
            }
        }
        for completion in self.completed_generation.drain(..) {
            if let Ok(output) = completion.result {
                self.cancelled_outputs.push(output);
            }
        }
        self.persist_cancelled_outputs()?;
        Ok(ChunkStorageDrain {
            proto: self.proto_saves.drain(..).collect(),
            chunks: self.unloading.drain(..).map(|job| job.saving).collect(),
        })
    }
    pub(crate) fn begin_close_chunk_storage(&mut self) -> Result<storage::IoTask<()>> {
        Ok(Arc::get_mut(&mut self.io)
            .context("chunk IO still owned by generation")?
            .shutdown())
    }
    pub fn refresh_generation_snapshots(&mut self, level: &Level) {
        if !self.pending.is_empty()
            && let Some(state) = &mut self.generation_state
        {
            state.refresh_full_chunks(level);
        }
    }
    /// Receive owned results in request order without taking main-thread tasks
    /// off the canonical queue. No Level borrow or promotion callback runs here.
    pub fn poll_generation_progress(&mut self) -> Result<bool> {
        let mut progressed = self.poll_running_generation()?;
        let waker = self.progress_waker.clone();
        let mut context = TaskContext::from_waker(&waker);
        progressed |= !self.cancelled_outputs.is_empty();
        self.persist_cancelled_outputs()?;
        while let Some(task) = self.proto_saves.front_mut() {
            let Poll::Ready(result) = Pin::new(task).poll(&mut context) else {
                break;
            };
            self.proto_saves.pop_front();
            result?;
            progressed = true;
        }
        while let Some((&request, _)) = self.pending.first_key_value() {
            let (position, answer) = self.pending.get_mut(&request).unwrap();
            let position = *position;
            let result = match answer.try_recv() {
                Ok(result) => result,
                Err(oneshot::error::TryRecvError::Empty) => break,
                Err(error) => Err(error.into()),
            };
            self.pending.remove(&request);
            self.completed_generation.push_back(ReadyGeneration {
                request,
                pos: position,
                result,
            });
            progressed = true;
        }
        // Integrate completed results before capturing the next job's live
        // imposter snapshots, including post-processing writes and heightmaps.
        if self.completed_generation.is_empty() {
            let running = self.generation_work.is_some();
            self.start_generation()?;
            progressed |= !running && self.generation_work.is_some();
        }
        Ok(progressed)
    }
    pub(crate) fn generation_running(&self) -> bool {
        self.generation_work.is_some()
    }
    /// Settle the submitted layer without dispatching a new one. Receivers and
    /// later completions stay reachable in the canonical backend during waits.
    pub(crate) fn poll_running_generation(&mut self) -> Result<bool> {
        let waker = self.progress_waker.clone();
        let mut context = TaskContext::from_waker(&waker);
        if let Some(work) = &mut self.generation_work {
            match Pin::new(work).poll(&mut context) {
                Poll::Ready(Ok(output)) => {
                    self.generation_work = None;
                    self.complete_generation(output)?;
                    return Ok(true);
                }
                Poll::Pending => {}
                Poll::Ready(Err(error)) => return Err(error.into()),
            }
        }
        Ok(false)
    }
    pub(crate) fn set_progress_waker(&mut self, waker: Waker) {
        self.progress_waker = waker;
    }
    pub(crate) fn take_light_completion(&mut self) -> Result<Option<LitChunk>> {
        let mut context = TaskContext::from_waker(&self.progress_waker);
        match self.light_rx.poll_recv(&mut context) {
            Poll::Ready(Some(light)) => Ok(Some(light)),
            Poll::Pending => Ok(None),
            Poll::Ready(None) => anyhow::bail!("generation light worker terminated"),
        }
    }
    /// Begin just one ordered integration. A nested managed-block pump can
    /// still reach every later completion, including an ordered failure.
    pub fn take_generation_completion(&mut self) -> Result<Option<(u64, Terrain)>> {
        let Some(completion) = self.completed_generation.pop_front() else {
            return Ok(None);
        };
        let ReadyGeneration {
            request,
            pos,
            result,
        } = completion;
        let _timing =
            crate::generation::profile::Span::new("integrate_output", Some((request, pos)));
        if self
            .active_generation
            .get(&pos.pack())
            .is_some_and(|active| active.task == request)
        {
            self.active_generation.remove(&pos.pack());
        }
        let output = match result {
            Ok(output) => output,
            Err(error) => {
                self.failed_generation.push((pos, request));
                return Err(error);
            }
        };
        if output.initial_mob_spawning_pending {
            self.pending_initial_spawns
                .insert(output.terrain.pos.pack());
        }
        self.generation_skips.extend(output.skips);
        for snapshot in output.proto_biomes {
            if self
                .proto_biomes
                .get(&snapshot.pos.pack())
                .is_none_or(|old| old.revision < snapshot.revision)
            {
                self.proto_biomes.insert(snapshot.pos.pack(), snapshot);
            }
        }
        for snapshot in output.proto_structures {
            self.integrate_proto_structures(snapshot)?;
        }
        for (pos, status) in output.statuses {
            self.generation_statuses
                .entry(pos.pack())
                .and_modify(|known| *known = (*known).max(status))
                .or_insert(status);
        }
        for snapshot in output.full_structures {
            self.integrate_structure_snapshot(snapshot)?;
        }
        for (pos, disk) in output.neighbors {
            self.store_generation_chunk(pos, output.terrain.revision, disk)?;
        }
        Ok(Some((request, output.terrain)))
    }
    fn integrate_proto_structures(&mut self, snapshot: ProtoStructureSnapshot) -> Result<()> {
        ensure!(
            snapshot.structures.position == [snapshot.pos.x, snapshot.pos.z],
            "proto structures belong to another chunk"
        );
        ensure!(
            snapshot.status >= ChunkStatus::StructureStarts,
            "proto structures precede STRUCTURE_STARTS"
        );
        let key = snapshot.pos.pack();
        if self
            .proto_structures
            .get(&key)
            .is_none_or(|old| old.revision < snapshot.revision)
        {
            self.proto_structures.insert(key, snapshot);
        }
        Ok(())
    }
    pub(crate) fn proto_chunk_structures(&self, pos: ChunkPos) -> Option<Arc<ChunkStructures>> {
        self.proto_structures
            .get(&pos.pack())
            .map(|snapshot| Arc::clone(&snapshot.structures))
    }
    pub(crate) fn proto_noise_biome(
        &self,
        pos: ChunkPos,
        quart: [i32; 3],
    ) -> Option<ferraloom_chunk::BiomeId> {
        self.proto_biomes
            .get(&pos.pack())
            .map(|snapshot| snapshot.biome(quart))
    }
    pub(crate) fn generator(&self) -> Arc<ChunkGenerator> {
        self.generator.clone()
    }
    pub(crate) fn build_height(&self) -> std::ops::Range<i32> {
        self.dimension.min_y..self.dimension.min_y + self.dimension.height
    }
    #[cfg(test)]
    fn poll_terrain(&mut self) -> Result<Vec<(u64, Terrain)>> {
        self.poll_generation_progress()?;
        let mut completed = Vec::new();
        while let Some(terrain) = self.take_generation_completion()? {
            completed.push(terrain);
        }
        Ok(completed)
    }
    pub fn queue_block_light_changes(
        &self,
        level: &Level,
        effects: Vec<ferraloom_game::level::effects::LevelEffect>,
    ) -> Result<()> {
        if let Some(changes) = Self::capture_block_light_changes(level, effects)? {
            self.submit_block_light_changes(changes)?;
        }
        Ok(())
    }
    pub(crate) fn capture_block_light_changes(
        level: &Level,
        effects: Vec<ferraloom_game::level::effects::LevelEffect>,
    ) -> Result<Option<BlockLightChanges>> {
        use ferraloom_game::level::effects::LevelEffect;
        if effects.is_empty() {
            return Ok(None);
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
        Ok(Some(BlockLightChanges(LightJob::Changed {
            chunks,
            changes,
        })))
    }
    pub(crate) fn submit_block_light_changes(&self, changes: BlockLightChanges) -> Result<()> {
        self.light_tx.send(changes.0)?;
        Ok(())
    }
    fn integrate_structure_snapshot(&mut self, mut snapshot: StructureSnapshot) -> Result<()> {
        ensure!(
            snapshot.structures.position == [snapshot.pos.x, snapshot.pos.z],
            "structure snapshot belongs to another chunk"
        );
        let key = snapshot.pos.pack();
        if self
            .structure_snapshots
            .get(&key)
            .is_some_and(|known| known.revision >= snapshot.revision)
        {
            return Ok(());
        }
        let unattached_disk = snapshot.unattached_disk.take();
        let attached = self.saved.contains_key(&key) || self.initial_chunks.contains_key(&key);
        if let Some(disk) = self.saved.get_mut(&key) {
            disk.structures = Some(snapshot.data.clone());
        }
        if let Some(terrain) = self.initial_chunks.get_mut(&key)
            && terrain.revision < snapshot.revision
        {
            terrain.disk.structures = Some(snapshot.data.clone());
            terrain.structures = snapshot.structures.clone();
            terrain.revision = snapshot.revision;
        }
        if !attached && let Some(mut disk) = unattached_disk {
            disk.structures = Some(snapshot.data.clone());
            self.store_generation_chunk(snapshot.pos, snapshot.revision, disk)?;
        }
        for continuation in self.full_promotions.values_mut() {
            if let Some(input) = &mut continuation.input
                && input.pos == snapshot.pos
                && input.revision < snapshot.revision
            {
                input.disk.structures = Some(snapshot.data.clone());
                input.structures = snapshot.structures.clone();
                input.revision = snapshot.revision;
            }
        }
        self.structure_snapshots.insert(key, snapshot);
        let pos = ChunkPos::unpack(key);
        if !self.pending_structure_publications.contains(&pos) {
            self.pending_structure_publications.push_back(pos);
        }
        Ok(())
    }
    /// Plan one owned publication without holding World or holder borrows over
    /// Level's shared insertion point. Unattached chunks retain their payload.
    pub(crate) fn next_structure_publication(
        &self,
        level: &Level,
    ) -> Option<(ChunkPos, GenerationRevision, ChunkStructures)> {
        self.pending_structure_publications.iter().find_map(|&pos| {
            level.chunk(pos)?;
            let snapshot = self.structure_snapshots.get(&pos.pack())?;
            Some((pos, snapshot.revision, snapshot.structures.clone()))
        })
    }
    pub(crate) fn finish_structure_publication(
        &mut self,
        pos: ChunkPos,
        revision: GenerationRevision,
    ) {
        // A nested publication may have delivered a newer version meanwhile.
        if self
            .structure_snapshots
            .get(&pos.pack())
            .is_some_and(|snapshot| snapshot.revision == revision)
        {
            self.pending_structure_publications
                .retain(|&queued| queued != pos);
        }
    }
    pub(crate) fn refresh_promotion_structures(&mut self, request: u64) -> Result<()> {
        let terrain = self
            .full_promotions
            .get_mut(&request)
            .context("missing FULL continuation")?
            .input
            .as_mut()
            .context("missing FULL terrain")?;
        if let Some(snapshot) = self.structure_snapshots.get(&terrain.pos.pack())
            && snapshot.revision > terrain.revision
        {
            terrain.disk.structures = Some(snapshot.data.clone());
            terrain.structures = snapshot.structures.clone();
            terrain.revision = snapshot.revision;
        }
        Ok(())
    }
    pub(crate) fn store_generation_chunk(
        &mut self,
        pos: ChunkPos,
        revision: GenerationRevision,
        mut disk: SerializableChunkData,
    ) -> Result<()> {
        let key = pos.pack();
        if self
            .generation_save_revisions
            .get(&key)
            .is_some_and(|&known| known > revision)
        {
            return Ok(());
        }
        let status = disk
            .status
            .as_ref()
            .and_then(|s| ferraloom_game::level::generation::parse_status(&s.to_string_lossy()))
            .context("unknown generation save status")?;
        if self
            .generation_statuses
            .get(&key)
            .is_some_and(|&known| known > status)
        {
            return Ok(());
        }
        if let Some(snapshot) = self.structure_snapshots.get(&key)
            && snapshot.revision > revision
        {
            disk.structures = Some(snapshot.data.clone());
        }
        self.proto_saves
            .push_back(self.io.store(pos, Some(disk.write()?)));
        self.generation_save_revisions.insert(key, revision);
        Ok(())
    }
    pub(crate) fn integrate(
        &mut self,
        request: u64,
        light_revision: GenerationRevision,
        terrain: &mut Terrain,
    ) -> Result<LevelChunk> {
        let _timing =
            crate::generation::profile::Span::new("prepare_full", Some((request, terrain.pos)));
        ensure!(
            terrain.structures.position == [terrain.pos.x, terrain.pos.z],
            "promotion structures belong to another chunk"
        );
        if let Some(snapshot) = self.structure_snapshots.get(&terrain.pos.pack())
            && snapshot.revision > terrain.revision
        {
            terrain.disk.structures = Some(snapshot.data.clone());
            terrain.structures = snapshot.structures.clone();
            terrain.revision = snapshot.revision;
        } else if let Some(data) = terrain.disk.structures.clone() {
            self.integrate_structure_snapshot(StructureSnapshot {
                pos: terrain.pos,
                revision: terrain.revision,
                data,
                structures: terrain.structures.clone(),
                unattached_disk: None,
            })?;
        }

        ensure!(
            terrain
                .disk
                .status
                .as_ref()
                .is_some_and(|s| s.to_string_lossy() == "minecraft:full"),
            "promotion before FULL"
        );
        let chunk = ferraloom_game::level::generation::promote(
            terrain.pos,
            terrain.sections.clone(),
            &terrain.disk,
            terrain.structures.clone(),
            terrain.revision.0,
        )
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let light = LightJob::Initialize {
            request,
            revision: light_revision,
            pos: terrain.pos,
            sections: terrain.sections.clone(),
            saved_light: Some(Box::new(terrain.disk.clone())),
            neighbors: std::mem::take(&mut terrain.light_neighbors),
        };
        if let Err(error) = self.light_tx.send(light) {
            if let LightJob::Initialize { neighbors, .. } = error.0 {
                terrain.light_neighbors = neighbors;
            }
            anyhow::bail!("light worker stopped during FULL preparation");
        }
        self.saved.insert(terrain.pos.pack(), terrain.disk.clone());
        Ok(chunk)
    }
    /// Capture owned sections here; palette/NBT encoding runs in the save pool.
    /// Clear the marker before capture so a later mutation remains unsaved.
    pub fn capture_chunk_for_save(
        &self,
        level: &mut Level,
        pos: ChunkPos,
        game_time: i64,
    ) -> Result<ChunkSaveSnapshot> {
        level
            .try_mark_chunk_saved(pos)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        match self.capture_chunk(level, pos, game_time) {
            Ok(snapshot) => Ok(snapshot),
            Err(error) => {
                level
                    .mark_chunk_unsaved(pos)
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                Err(error)
            }
        }
    }
    pub fn capture_chunk(
        &self,
        level: &Level,
        pos: ChunkPos,
        game_time: i64,
    ) -> Result<ChunkSaveSnapshot> {
        self.chunk_save_plan(pos, game_time)?.capture(level)
    }
    /// Capture cold storage/light inputs under the backend's short guard. The
    /// returned plan enters Level only after that guard has ended.
    pub(crate) fn chunk_save_plan(&self, pos: ChunkPos, game_time: i64) -> Result<ChunkSavePlan> {
        Ok(ChunkSavePlan {
            pos,
            disk: self
                .saved
                .get(&pos.pack())
                .context("save of unknown full chunk")?
                .clone(),
            light: self.lights.get(&pos.pack()).cloned(),
            game_time,
            light_correct: self.light_ready.contains(&pos),
        })
    }
    pub(crate) fn begin_encode_saves(
        &self,
        snapshots: Vec<ChunkSaveSnapshot>,
    ) -> oneshot::Receiver<Result<Vec<(ChunkPos, CompoundTag)>>> {
        let registry = self.registry.clone();
        self.saving.submit(move || {
            snapshots
                .into_iter()
                .map(|snapshot| {
                    let pos = snapshot.pos;
                    Ok((pos, snapshot.encode(registry.as_ref())?))
                })
                .collect()
        })
    }
    pub async fn encode_saves(
        &mut self,
        snapshots: Vec<ChunkSaveSnapshot>,
    ) -> Result<Vec<(ChunkPos, CompoundTag)>> {
        self.begin_encode_saves(snapshots).await?
    }
    pub(crate) fn take_bootstrap_save_snapshots(
        &mut self,
        game_time: i64,
    ) -> Vec<ChunkSaveSnapshot> {
        std::mem::take(&mut self.initial_chunks)
            .into_values()
            .map(|terrain| ChunkSaveSnapshot {
                pos: terrain.pos,
                sections: terrain.sections,
                light_correct: terrain.disk.is_light_on == Some(true),
                disk: terrain.disk,
                light: None,
                game_time,
            })
            .collect()
    }
}

/// Owned cold portion of SerializableChunkData#copyOf. Gameplay/BE serializers
/// execute from capture after the runtime borrow has ended.
pub(crate) struct ChunkSavePlan {
    pos: ChunkPos,
    disk: SerializableChunkData,
    light: Option<ChunkLight>,
    game_time: i64,
    light_correct: bool,
}
pub(crate) struct LiveLevelSaveContext {
    tags: ferraloom_data::datapack::TagSet<Block>,
    border: ferraloom_game::level::border::WorldBorder,
}
impl LiveLevelSaveContext {
    pub(crate) fn capture(level: &Level) -> Result<Self> {
        Ok(Self {
            tags: level
                .block_tags()
                .map_err(|error| anyhow::anyhow!("{error:?}"))?
                .clone(),
            border: level.entity_world_border().clone(),
        })
    }
}
impl ChunkSavePlan {
    pub(crate) fn capture(self, level: &Level) -> Result<ChunkSaveSnapshot> {
        let pos = self.pos;
        let chunk = level.chunk(pos).context("save of missing full chunk")?;
        let mut disk = self.disk;
        level
            .save_chunk_ticks(pos, &mut disk)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        chunk.save_pending_generation_data(&mut disk);
        disk.structures = Some(storage::StructureData::from_nbt(
            chunk.structures().to_nbt()?,
        )?);
        disk.inhabited_time = Some(chunk.inhabited_time());
        disk.block_entities = Some(
            level
                .block_entity_tags_for_saving(pos)
                .map_err(|error| anyhow::anyhow!("{error:?}"))?,
        );
        write_runtime_heightmaps(chunk, &mut disk)?;
        Ok(ChunkSaveSnapshot {
            pos,
            sections: chunk.sections().cloned().collect(),
            disk,
            light: self.light,
            game_time: self.game_time,
            light_correct: self.light_correct,
        })
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
/// Port of LevelSettings#parse and DifficultySettings.CODEC. A malformed record
/// uses the native complete-record default, rather than mixing individual fields.
fn read_difficulty_settings(data: &CompoundTag) -> (Difficulty, bool, bool) {
    let decoded = (|| {
        let settings = data.get_compound("difficulty_settings")?;
        Some((
            match settings
                .get_string("difficulty")?
                .to_string_checked()
                .ok()?
                .as_str()
            {
                "peaceful" => Difficulty::Peaceful,
                "easy" => Difficulty::Easy,
                "normal" => Difficulty::Normal,
                "hard" => Difficulty::Hard,
                _ => return None,
            },
            // NbtOps#getBooleanValue compares the number as double, unlike
            // CompoundTag#getBoolean's narrowing byte conversion.
            settings.get("hardcore")?.as_double()? != 0.0,
            settings.get("locked")?.as_double()? != 0.0,
        ))
    })();
    decoded.unwrap_or((Difficulty::Normal, false, false))
}

/// Port of DedicatedServer#forceDifficulty and MinecraftServer#setDifficulty.
fn force_initial_difficulty(data: &mut CompoundTag, requested: &str) {
    let (_, hardcore, locked) = read_difficulty_settings(data);
    // ServerProperties has already normalized the requested difficulty name.
    let difficulty = if hardcore { "hard" } else { requested };
    let mut settings = data
        .get_compound("difficulty_settings")
        .cloned()
        .unwrap_or_default();
    settings.insert("difficulty", Tag::String(difficulty.into()));
    settings.insert("hardcore", Tag::Byte(i8::from(hardcore)));
    settings.insert("locked", Tag::Byte(i8::from(locked)));
    data.insert("difficulty_settings", Tag::Compound(settings));
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
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            ferraloom_game::level::LevelContext::fixture(0, 63),
        );
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
    fn older_promotion_preserves_newer_neighbor_light_and_later_deltas() {
        let (send, receive) = mpsc::channel();
        let (reply, mut results) = tokio::sync::mpsc::unbounded_channel();
        let worker = std::thread::spawn(move || run_light_worker(receive, reply, 0, 1, false));
        let first = ChunkPos::new(1, 0);
        let second = ChunkPos::new(0, 0);
        let biome = BiomeId(0);
        let empty = LevelChunkSection::new(
            Strategy::block_states(),
            Strategy::biomes(Arc::new(IdMap::new(vec![biome]).unwrap())),
            biome,
        );
        let mut source = empty.clone();
        source.set_block_state(8, 0, 8, Block::GLOWSTONE.default_state());
        let disk = |light| {
            let mut bytes = vec![0; 2048];
            bytes[68] = light;
            SerializableChunkData {
                is_light_on: Some(true),
                sections: Some(vec![storage::ChunkSection {
                    y: Some(0),
                    block_light: Some(ByteArray(bytes)),
                    ..Default::default()
                }]),
                ..Default::default()
            }
        };
        // B executed at revision 1; A then imported its updated light at 2.
        send.send(LightJob::Initialize {
            request: 0,
            revision: GenerationRevision(2),
            pos: first,
            sections: vec![empty.clone()],
            saved_light: None,
            neighbors: vec![GenerationLightNeighbor {
                pos: second,
                revision: GenerationRevision(2),
                sections: vec![source.clone()],
                disk: disk(15),
            }],
        })
        .unwrap();
        let first_result = results.blocking_recv().unwrap();
        assert_eq!(
            first_result.initialized,
            Some(GenerationLightInitialization {
                request: 0,
                pos: first,
                revision: GenerationRevision(2)
            })
        );
        assert_eq!(first_result.updates[&second.pack()].block[&0][68] & 15, 15);
        send.send(LightJob::Initialize {
            request: 1,
            revision: GenerationRevision(1),
            pos: second,
            sections: vec![source],
            saved_light: Some(Box::new(disk(0))),
            neighbors: Vec::new(),
        })
        .unwrap();
        let promoted = results.blocking_recv().unwrap();
        assert_eq!(
            promoted.initialized,
            Some(GenerationLightInitialization {
                request: 1,
                pos: second,
                revision: GenerationRevision(2)
            })
        );
        assert_eq!(promoted.updates[&second.pack()].block[&0][68] & 15, 15);
        send.send(LightJob::Changed {
            chunks: vec![(second, vec![empty])],
            changes: vec![
                LightMutation::SectionStatus(SectionPos::new(0, 0, 0), true),
                LightMutation::CheckBlock(BlockPos::new(8, 0, 8)),
            ],
        })
        .unwrap();
        let delta = results.blocking_recv().unwrap();
        assert_eq!(delta.initialized, None);
        assert_eq!(
            delta.updates[&second.pack()]
                .block
                .get(&0)
                .map_or(0, |data| data[68] & 15),
            0
        );
        drop(send);
        worker.join().unwrap();
    }

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
            revision: GenerationRevision(1),
            pos,
            sections: sections.clone(),
            saved_light: None,
            neighbors: Vec::new(),
        })
        .unwrap();
        assert_eq!(
            results.blocking_recv().unwrap().initialized,
            Some(GenerationLightInitialization {
                request: 7,
                pos,
                revision: GenerationRevision(1)
            })
        );
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

    #[tokio::test]
    async fn saved_world_structure_option_overrides_properties_and_defaults_to_true() {
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder vanilla jar");
        let resources = ResourceManager::new(vec![
            ferraloom_data::datapack::Pack::vanilla_jar(Path::new(&jar)).unwrap(),
        ]);
        let registries = RegistrySet::load(&resources).unwrap();
        for generate_structures in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let mut properties = ServerProperties::from_values(Default::default());
            properties.set("level-type", "minecraft:debug_all_block_states");
            properties.set("level-seed", 8675309);
            properties.set("generate-structures", generate_structures);
            let mut world_root = WorldRoot::open(
                directory.path(),
                &properties,
                &resources,
                &registries,
                ferraloom_game::block::UnportedBehavior::Strict,
            )
            .unwrap();
            let mut world = world_root
                .open_dimension("minecraft:overworld".parse().unwrap(), &properties)
                .unwrap();
            let saved_data = SavedDataStorage::new(world.access.path().join("data"));
            let saved = saved_data
                .load::<CompoundTag>("minecraft:world_gen_settings")
                .unwrap()
                .unwrap();
            assert_eq!(
                saved.value().get_boolean("generate_structures"),
                Some(generate_structures)
            );
            assert!(!saved.value().contains_key("generate_features"));
            world.close_chunk_storage().await.unwrap();
            drop(world);
            drop(world_root);

            properties.set("generate-structures", !generate_structures);
            properties.set("level-seed", 0);
            properties.set("level-type", "minecraft:flat");
            let mut world_root = WorldRoot::open(
                directory.path(),
                &properties,
                &resources,
                &registries,
                ferraloom_game::block::UnportedBehavior::Strict,
            )
            .unwrap();
            let mut world = world_root
                .open_dimension("minecraft:overworld".parse().unwrap(), &properties)
                .unwrap();
            assert_eq!(
                world
                    .generation_state
                    .as_ref()
                    .unwrap()
                    .context
                    .generate_structures,
                generate_structures
            );
            assert_eq!(world.seed, 8675309);
            assert!(matches!(world.generator.as_ref(), ChunkGenerator::Debug(_)));
            world.close_chunk_storage().await.unwrap();
            drop(world);
            drop(world_root);

            if !generate_structures {
                // The old, unrelated key does not override the codec default.
                let mut settings = saved.value().clone();
                settings.remove("generate_structures");
                settings.insert("generate_features", Tag::Byte(0));
                saved_data
                    .save(
                        "minecraft:world_gen_settings",
                        &mut SavedData::new(settings),
                    )
                    .unwrap();
                properties.set("generate-structures", false);
                let mut world_root = WorldRoot::open(
                    directory.path(),
                    &properties,
                    &resources,
                    &registries,
                    ferraloom_game::block::UnportedBehavior::Strict,
                )
                .unwrap();
                let mut world = world_root
                    .open_dimension("minecraft:overworld".parse().unwrap(), &properties)
                    .unwrap();
                assert!(
                    world
                        .generation_state
                        .as_ref()
                        .unwrap()
                        .context
                        .generate_structures
                );
                world.close_chunk_storage().await.unwrap();
            }
        }
    }

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
            data_version: Some(storage::DATA_VERSION),
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
        use ferraloom_game::{Behaviors, level::Region, level::server_level::ServerLevel};
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder vanilla jar");
        let resources = ResourceManager::new(vec![
            ferraloom_data::datapack::Pack::vanilla_jar(Path::new(&jar)).unwrap(),
        ]);
        let registries = RegistrySet::load(&resources).unwrap();
        let mut properties = ServerProperties::from_values(Default::default());
        properties.set("level-type", "minecraft:debug_all_block_states");
        let directory = tempfile::tempdir().unwrap();
        let mut world_root = WorldRoot::open(
            directory.path(),
            &properties,
            &resources,
            &registries,
            ferraloom_game::block::UnportedBehavior::Strict,
        )
        .unwrap();
        let mut world = world_root
            .open_dimension("minecraft:overworld".parse().unwrap(), &properties)
            .unwrap();
        let mut level = ServerLevel::new(
            Region::new(&BEHAVIORS),
            "minecraft:overworld",
            world.level_context(&resources).unwrap(),
        );
        level
            .level
            .set_block_tags(world.generator.data().block_tags.clone());
        level
            .level
            .set_unported_behavior(ferraloom_game::block::UnportedBehavior::Warn);
        let empty_sections = world
            .generation_state
            .as_ref()
            .unwrap()
            .context
            .empty_chunk([0, 0])
            .unwrap()
            .chunk
            .sections;
        let structures = |pos: ChunkPos, placed| {
            use ferraloom_worldgen::structure::{
                TerrainAdjustment,
                references::StructureReferences,
                scattered_piece::ScatteredFeaturePiece,
                start::{StructurePiece, StructureStart},
                structures::jungle_temple::JungleTemplePiece,
            };
            let id = "minecraft:jungle_pyramid".parse().unwrap();
            let mut structures = ChunkStructures::new([pos.x, pos.z]);
            structures.starts.insert(
                id,
                StructureStart::new(
                    "minecraft:jungle_pyramid".parse().unwrap(),
                    ferraloom_data::registries::worldgen_structure_type::Id::from_name(
                        "minecraft:jungle_temple",
                    )
                    .unwrap(),
                    [pos.x, pos.z],
                    0,
                    TerrainAdjustment::None,
                    vec![StructurePiece::JungleTemple(JungleTemplePiece {
                        scattered: ScatteredFeaturePiece::new(
                            [pos.x * 16, 64, pos.z * 16],
                            [12, 10, 15],
                            ferraloom_util::Direction::North,
                        ),
                        placed_main_chest: placed,
                        placed_hidden_chest: false,
                        placed_traps: [false; 2],
                    })],
                ),
            );
            structures.references.insert(
                "minecraft:jungle_pyramid".parse().unwrap(),
                StructureReferences::from_array(&[
                    pos.pack(),
                    ChunkPos::new(pos.x + 1, pos.z).pack(),
                ]),
            );
            structures
        };
        let structure_data = |pos, placed| {
            storage::StructureData::from_nbt(structures(pos, placed).to_nbt().unwrap()).unwrap()
        };
        let output = |pos, unit: &str| GenerationOutput {
            terrain: Terrain {
                pos,
                revision: GenerationRevision::default(),
                sections: empty_sections.clone(),
                surface: [0; 256],
                disk: SerializableChunkData {
                    data_version: Some(storage::DATA_VERSION),
                    x_pos: Some(pos.x),
                    z_pos: Some(pos.z),
                    y_pos: Some(-4),
                    status: Some("minecraft:full".into()),
                    ..Default::default()
                },
                structures: ChunkStructures::new([pos.x, pos.z]),
                light_neighbors: Vec::new(),
            },
            neighbors: Vec::new(),
            skips: vec![(unit.into(), "pending callback".into())],
            statuses: vec![(pos, ChunkStatus::Full)],
            proto_biomes: Vec::new(),
            proto_structures: Vec::new(),
            full_structures: Vec::new(),
            initial_mob_spawning_pending: true,
        };
        let first = ChunkPos::new(0, 0);
        let second = ChunkPos::new(1, 0);
        // The forceload command publishes the ticket before awaiting generation.
        assert!(level.set_chunk_forced(second, true));
        for change in level
            .level
            .run_distance_manager_updates(&mut level.distance_manager, &mut level.tickets)
        {
            level.chunks.ensure_holder(change.pos).ticket_level = change.level;
        }
        for (pos, task) in [(first, 0), (second, 1)] {
            level.chunks.update_highest_allowed_status(pos);
            assert_eq!(
                level
                    .chunks
                    .schedule_generation_task(pos, ChunkStatus::Full)
                    .task
                    .schedule
                    .unwrap()
                    .id,
                task
            );
        }
        assert!(
            level.chunks.holders[&second.pack()].requested_full_status()
                >= ferraloom_game::level::distance_manager::FullChunkStatus::BlockTicking
        );
        let (first_reply, first_result) = oneshot::channel();
        let (second_reply, second_result) = oneshot::channel();
        world.pending.insert(0, (first, first_result));
        world.pending.insert(1, (second, second_result));
        // Priority executes B first (revision 1). A then mutates B's shared
        // StructureStart before A/B are integrated in their original order.
        let mut second_output = output(second, "second");
        second_output.terrain.revision = GenerationRevision(1);
        second_output.terrain.disk.structures = Some(structure_data(second, false));
        second_output.terrain.structures = structures(second, false);
        let auxiliary = ChunkPos::new(5, 0);
        let proto = ChunkPos::new(6, 0);
        let proto_disk = |status, marker| {
            let mut disk = output(proto, "proto").terrain.disk;
            disk.status = Some(ferraloom_game::level::generation::status_name(status).into());
            disk.extra.insert("marker", Tag::Int(marker));
            disk
        };
        second_output
            .neighbors
            .push((proto, proto_disk(ChunkStatus::Terrain, 1)));
        second_output.statuses.push((proto, ChunkStatus::Terrain));
        second_output.full_structures.push(StructureSnapshot {
            pos: auxiliary,
            revision: GenerationRevision(1),
            data: structure_data(auxiliary, false),
            structures: structures(auxiliary, false),
            unattached_disk: Some(output(auxiliary, "auxiliary").terrain.disk),
        });
        assert!(second_reply.send(Ok(second_output)).is_ok());
        assert!(world.poll_terrain().unwrap().is_empty());
        assert!(world.generation_statuses.is_empty());
        assert!(world.generation_skips.is_empty());
        assert!(world.pending_initial_spawns.is_empty());
        assert!(world.structure_snapshots.is_empty());
        assert_eq!(level.level.unported_behavior_stats().skipped_calls, 0);
        assert!(level.level.chunk(second).is_none());
        let mut first_output = output(first, "first");
        first_output.terrain.revision = GenerationRevision(2);
        first_output
            .neighbors
            .push((proto, proto_disk(ChunkStatus::Features, 2)));
        first_output.statuses.push((proto, ChunkStatus::Features));
        first_output.full_structures = [second, auxiliary]
            .into_iter()
            .map(|pos| StructureSnapshot {
                pos,
                revision: GenerationRevision(2),
                data: structure_data(pos, true),
                structures: structures(pos, true),
                unattached_disk: Some(output(pos, "unattached").terrain.disk),
            })
            .collect();
        assert!(first_reply.send(Ok(first_output)).is_ok());
        let chunks = world.poll_terrain().unwrap();
        assert_eq!(chunks.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [0, 1]);
        assert_eq!(world.generation_skips[0].0, "first");
        assert_eq!(world.generation_skips[1].0, "second");
        assert_eq!(world.generation_statuses[&second.pack()], ChunkStatus::Full);
        assert!(world.pending_initial_spawns.contains(&second.pack()));
        assert_eq!(
            world.structure_snapshots[&auxiliary.pack()].revision,
            GenerationRevision(2)
        );
        let auxiliary_disk =
            SerializableChunkData::parse(world.io.load(auxiliary).await.unwrap().unwrap()).unwrap();
        assert_eq!(
            auxiliary_disk.structures,
            Some(structure_data(auxiliary, true))
        );
        let proto_disk =
            SerializableChunkData::parse(world.io.load(proto).await.unwrap().unwrap()).unwrap();
        assert_eq!(proto_disk.extra.get_int("marker"), Some(2));
        assert_eq!(
            world.generation_statuses[&proto.pack()],
            ChunkStatus::Features
        );
        let promotion_skips = std::mem::take(&mut world.generation_skips);
        // Cancel a completed later result while it is still held behind an
        // earlier request. It must save its payload without promoting a holder.
        let cancelled = ChunkPos::new(2, 0);
        let (blocker_reply, blocker_result) = oneshot::channel();
        let (cancelled_reply, cancelled_result) = oneshot::channel();
        world.pending.insert(2, (first, blocker_result));
        world.pending.insert(3, (cancelled, cancelled_result));
        world.active_generation.insert(
            cancelled.pack(),
            GenerationActivity {
                task: 3,
                cancelled: Arc::new(AtomicBool::new(false)),
                radius: 11,
            },
        );
        let mut cancelled_output = output(cancelled, "cancelled");
        cancelled_output.terrain.revision = GenerationRevision(3);
        cancelled_output.full_structures.push(StructureSnapshot {
            pos: auxiliary,
            revision: GenerationRevision(3),
            data: structure_data(auxiliary, false),
            structures: structures(auxiliary, false),
            unattached_disk: Some(output(auxiliary, "unattached").terrain.disk),
        });
        assert!(cancelled_reply.send(Ok(cancelled_output)).is_ok());
        assert!(world.generation_pins(cancelled));
        assert_eq!(world.cancel_generation(cancelled), Some(3));
        assert!(world.poll_terrain().unwrap().is_empty());
        assert!(!world.generation_statuses.contains_key(&cancelled.pack()));
        assert!(
            !world
                .generation_skips
                .iter()
                .any(|(unit, _)| unit == "cancelled")
        );
        assert!(world.io.load(cancelled).await.unwrap().is_some());
        let auxiliary_disk =
            SerializableChunkData::parse(world.io.load(auxiliary).await.unwrap().unwrap()).unwrap();
        assert_eq!(
            auxiliary_disk.structures,
            Some(structure_data(auxiliary, false))
        );
        assert!(blocker_reply.send(Ok(output(first, "blocker"))).is_ok());
        assert_eq!(world.poll_terrain().unwrap()[0].0, 2);
        let retry = ChunkPos::new(128, 128);
        world.request(4, retry);
        assert_eq!(world.cancel_generation(retry), Some(4));
        world.request(5, retry);
        let completed = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                let done = world.poll_terrain().unwrap();
                if !done.is_empty() {
                    break done;
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!((completed[0].0, completed[0].1.pos), (5, retry));
        // Taking A's task must not strand B in an outer local Vec while A's
        // callback is parked. A later failed request must not discard either.
        let (a_reply, a_result) = oneshot::channel();
        let (b_reply, b_result) = oneshot::channel();
        let (failed_reply, failed_result) = oneshot::channel();
        world.pending.insert(6, (first, a_result));
        world.pending.insert(7, (second, b_result));
        world.pending.insert(8, (cancelled, failed_result));
        assert!(a_reply.send(Ok(output(first, "outer"))).is_ok());
        assert!(b_reply.send(Ok(output(second, "nested"))).is_ok());
        assert!(
            failed_reply
                .send(Err(anyhow::anyhow!("owned failure")))
                .is_ok()
        );
        assert!(world.poll_generation_progress().unwrap());
        assert_eq!(world.take_generation_completion().unwrap().unwrap().0, 6);
        world.poll_generation_progress().unwrap();
        assert_eq!(world.take_generation_completion().unwrap().unwrap().0, 7);
        assert_eq!(
            world
                .take_generation_completion()
                .err()
                .unwrap()
                .to_string(),
            "owned failure"
        );
        assert!(world.failed_generation.contains(&(cancelled, 8)));
        assert!(world.take_generation_completion().unwrap().is_none());
        // The transport exercise above simulated completed task IDs. The live
        // continuation test below retains its own real scheduled holder tasks.
        world.settled_generation.clear();
        world.failed_generation.clear();
        world.generation_skips = promotion_skips;
        let mut live =
            crate::generation::runtime::LiveLevel::install(world, level, Arc::new(properties))
                .unwrap();
        for (task, terrain) in chunks {
            live.generation()
                .accept_test_terrain(&live.level, task, terrain)
                .unwrap();
        }
        live.level.get_full_chunk(first).unwrap();
        live.level.get_full_chunk(second).unwrap();
        assert_eq!(live.level.unported_behavior_stats().skipped_calls, 2);
        assert!(live.level.chunk(second).unwrap().is_unsaved());
        assert!(
            !live
                .generation()
                .with_world(|world| world.ready.contains(&second))
        );
        assert_eq!(
            live.generation()
                .with_world(|world| world.saved[&second.pack()].structures.clone()),
            Some(structure_data(second, true))
        );
        assert_eq!(live.level.chunk(second).unwrap().structures_revision(), 2);
        assert_eq!(
            live.level
                .chunk(second)
                .unwrap()
                .structures()
                .to_nbt()
                .unwrap(),
            structures(second, true).to_nbt().unwrap()
        );
        let structure = "minecraft:jungle_pyramid".parse().unwrap();
        assert!(
            live.level
                .structure_manager()
                .get_start_for_structure(second, &structure)
                .unwrap()
                .unwrap()
                .is_valid()
        );
        assert_eq!(
            live.level
                .structure_manager()
                .get_references_for_structure(second, &structure)
                .unwrap()
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            structures(second, true).references[&structure]
                .iter()
                .collect::<Vec<_>>()
        );
        let plan = live
            .generation()
            .with_world(|world| world.chunk_save_plan(second, 0))
            .unwrap();
        assert_eq!(
            plan.capture(&live.level).unwrap().disk.structures,
            Some(structure_data(second, true))
        );
        live.level.try_mark_chunk_saved(second).unwrap();
        let old_structures = live.level.chunk(second).unwrap().structures();
        live.generation()
            .with_world_mut(|world| {
                world.integrate_structure_snapshot(StructureSnapshot {
                    pos: second,
                    revision: GenerationRevision(3),
                    data: structure_data(second, false),
                    structures: structures(second, false),
                    unattached_disk: None,
                })
            })
            .unwrap();
        live.generation()
            .publish_structure_updates(&live.level)
            .unwrap();
        assert_eq!(
            old_structures.to_nbt().unwrap(),
            structures(second, true).to_nbt().unwrap()
        );
        assert_eq!(live.level.chunk(second).unwrap().structures_revision(), 3);
        assert!(live.level.chunk(second).unwrap().is_unsaved());
        live.generation().with_world_mut(|world| {
            world.saved.get_mut(&second.pack()).unwrap().structures =
                Some(structure_data(second, true))
        });
        let plan = live
            .generation()
            .with_world(|world| world.chunk_save_plan(second, 0))
            .unwrap();
        assert_eq!(
            plan.capture(&live.level).unwrap().disk.structures,
            Some(structure_data(second, false))
        );
        // The captured range continuation has an older holder incarnation. Its
        // exclusive callback must not prepare the current holder at that position.
        let identity = live
            .generation()
            .with_chunks(|chunks| GenerationIdentity::of(&chunks.holders[&second.pack()]));
        live.generation().with_world_mut(|world| {
            world.exclusive_ready_tasks.push_back(GenerationIdentity {
                holder_epoch: 0,
                ..identity
            })
        });
        live.finish_promotions().unwrap();
        assert!(
            !live
                .generation()
                .with_world(|world| world.ready.contains(&second))
        );
        assert!(
            live.generation()
                .with_world(|world| world.exclusive_ready_tasks.is_empty())
        );
        let drain = live
            .generation()
            .with_world_mut(World::prepare_close_chunk_storage)
            .unwrap();
        for task in drain.proto {
            task.await.unwrap();
        }
        for task in drain.chunks {
            task.await.unwrap().unwrap();
        }
        let close = live
            .generation()
            .with_world_mut(World::begin_close_chunk_storage)
            .unwrap();
        close.await.unwrap();
    }
}

#[cfg(test)]
mod difficulty_tests {
    use super::*;

    fn name(value: Difficulty) -> &'static str {
        match value {
            Difficulty::Peaceful => "peaceful",
            Difficulty::Easy => "easy",
            Difficulty::Normal => "normal",
            Difficulty::Hard => "hard",
        }
    }

    #[test]
    fn initial_difficulty_uses_native_record_defaults_hardcore_and_lock_handling() {
        let mut counts = (0, 0);
        for row in include_str!("../../ferraloom-game/tests/java/entity/vectors.tsv").lines() {
            let fields: Vec<_> = row.split('\t').collect();
            match fields[0] {
                "difficulty-read" => {
                    let data = ferraloom_nbt::parse_compound(fields[1]).unwrap();
                    let (difficulty, hardcore, locked) = read_difficulty_settings(&data);
                    assert_eq!(name(difficulty), fields[2], "{row}");
                    assert_eq!(hardcore, fields[3] == "true", "{row}");
                    assert_eq!(locked, fields[4] == "true", "{row}");
                    counts.0 += 1;
                }
                "difficulty-initial" => {
                    let mut data = ferraloom_nbt::parse_compound(&format!(
                        "{{difficulty_settings:{{difficulty:normal,hardcore:{}b,locked:{}b,future:42}}}}",
                        i8::from(fields[1] == "true"),
                        i8::from(fields[2] == "true"),
                    ))
                    .unwrap();
                    force_initial_difficulty(&mut data, fields[3]);
                    let (difficulty, hardcore, locked) = read_difficulty_settings(&data);
                    assert_eq!(name(difficulty), fields[4], "{row}");
                    assert_eq!(hardcore, fields[1] == "true", "{row}");
                    assert_eq!(locked, fields[5] == "true", "{row}");
                    assert_eq!(
                        data.get_compound("difficulty_settings")
                            .unwrap()
                            .get_int("future"),
                        Some(42)
                    );
                    counts.1 += 1;
                }
                _ => (),
            }
        }
        assert_eq!(counts, (16, 16));
    }
}
