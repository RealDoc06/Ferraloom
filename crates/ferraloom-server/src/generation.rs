//! Owned chunk-generation state crosses the off-tick pool as one value. Within
//! overlapping neighborhoods vanilla's X-major layers are serialized. Vanilla
//! itself permits order-dependent feature writes; worker completion order never
//! chooses which write wins here. Disjoint dimensions can use separate workers.
pub(crate) mod runtime;

use crate::jobs::JobPool;
use crate::world::{
    GenerationLightNeighbor, RegistryAdapter, Terrain, surface_heights_in_dimension,
};
use anyhow::{Context, Result, ensure};
use ferraloom_game::level::generation::{
    ChunkGenerationTask, ChunkStatus, ChunkStatusTask, GenerationChunkHolder, ProtoChunk,
    WorldGenContext,
};
use ferraloom_light::{LevelLightEngine, LightLayer, SectionSnapshots};
use ferraloom_storage::{self as storage, IoWorker, SerializableChunkData};
use ferraloom_util::{ChunkPos, SectionPos};
use ferraloom_worldgen::structure::{check::StructureCheck, pipeline::ChunkStructures};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// Execution order is separate from FULL request/integration order. Shared
/// StructureStart mutations must survive an older owned promotion snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct GenerationRevision(pub u64);

#[derive(Clone)]
pub(crate) struct StructureSnapshot {
    pub pos: ChunkPos,
    pub revision: GenerationRevision,
    pub data: storage::StructureData,
    pub structures: ChunkStructures,
    /// An unattached FULL imposter still needs a save when its shared starts
    /// change. Attached chunks instead save their canonical live payload.
    pub unattached_disk: Option<SerializableChunkData>,
}

pub(crate) struct GenerationRequest {
    pub id: u64,
    pub pos: ChunkPos,
    pub status: ChunkStatus,
    pub reply: tokio::sync::oneshot::Sender<Result<GenerationOutput>>,
    pub cancelled: Arc<AtomicBool>,
}
/// Mailbox completion retained until its ordered main-thread integration starts.
/// A parked callback must leave subsequent results reachable by a nested pump.
pub(crate) struct ReadyGeneration {
    pub request: u64,
    pub pos: ChunkPos,
    pub result: Result<GenerationOutput>,
}
/// Identity captured by a main-executor FULL continuation. The status future
/// survives replacement tasks; epoch rejects a removed/recreated holder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GenerationIdentity {
    pub pos: ChunkPos,
    pub request: u64,
    pub holder_epoch: u64,
}
impl GenerationIdentity {
    pub fn of(holder: &ferraloom_game::level::chunk_holder::ChunkHolder) -> Self {
        Self {
            pos: holder.position,
            request: holder
                .generation
                .future_id(ChunkStatus::Full)
                .expect("FULL continuation requires its status future")
                .0,
            holder_epoch: holder.epoch,
        }
    }
    pub fn matches(&self, holder: &ferraloom_game::level::chunk_holder::ChunkHolder) -> bool {
        self.pos == holder.position
            && self.holder_epoch == holder.epoch
            && holder
                .generation
                .future_id(ChunkStatus::Full)
                .is_some_and(|future| future.0 == self.request)
    }
}
pub(crate) struct GenerationCompletion {
    pub state: GenerationState,
    pub result: Result<GenerationOutput>,
    pub request: GenerationRequest,
    pub cleanup: Result<()>,
}
pub struct GenerationOutput {
    pub terrain: Terrain,
    pub neighbors: Vec<(ChunkPos, SerializableChunkData)>,
    pub skips: Vec<(String, String)>,
    pub statuses: Vec<(ChunkPos, ChunkStatus)>,
    pub(crate) proto_biomes: Vec<ProtoBiomeSnapshot>,
    pub(crate) full_structures: Vec<StructureSnapshot>,
    pub initial_mob_spawning_pending: bool,
}
/// Immutable owned biome result for live BIOMES acquisitions. Writable proto
/// neighborhoods remain exclusively in the generation worker's owned cache.
pub(crate) struct ProtoBiomeSnapshot {
    pub pos: ChunkPos,
    pub revision: GenerationRevision,
    pub min_section: i32,
    pub biomes: Vec<ferraloom_chunk::PalettedContainer<ferraloom_chunk::BiomeId>>,
}
impl ProtoBiomeSnapshot {
    pub fn biome(&self, quart: [i32; 3]) -> ferraloom_chunk::BiomeId {
        let min_quart = self.min_section * 4;
        let y = quart[1].clamp(min_quart, min_quart + self.biomes.len() as i32 * 4 - 1);
        self.biomes[((y - min_quart) / 4) as usize].get(
            (quart[0] & 3) as u8,
            (y & 3) as u8,
            (quart[2] & 3) as u8,
        )
    }
}
/// Port of ChunkMap's generation cache. FULL entries are read-only imposter
/// protos. No references into this cache escape an owned generation job.
pub struct GenerationState {
    pub context: Arc<WorldGenContext>,
    holders: BTreeMap<i64, GenerationChunkHolder<ProtoChunk>>,
    failed_loads: BTreeMap<i64, ferraloom_game::level::generation::GenerationError>,
    light: LevelLightEngine,
    light_pool: Arc<JobPool>,
    snapshots: SectionSnapshots<'static>,
    structure_check: StructureCheck,
    revision: GenerationRevision,
    placeholder: ferraloom_worldgen::region::RegionChunk,
}
impl GenerationState {
    pub fn new(context: Arc<WorldGenContext>, light_pool: Arc<JobPool>) -> Self {
        let placeholder = context
            .empty_chunk([0, 0])
            .expect("validated generation context");
        Self {
            placeholder,
            light_pool,
            light: LevelLightEngine::new(true, context.dimension.has_skylight),
            snapshots: SectionSnapshots::new(
                context.dimension.min_y >> 4,
                (context.dimension.min_y + context.dimension.height) >> 4,
            ),
            context,
            holders: BTreeMap::new(),
            failed_loads: BTreeMap::new(),
            structure_check: StructureCheck::default(),
            revision: GenerationRevision::default(),
        }
    }
    pub fn completed_statuses(&self) -> Vec<(ChunkPos, ChunkStatus)> {
        self.holders
            .iter()
            .map(|(&key, holder)| (ChunkPos::unpack(key), holder.completed))
            .collect()
    }
    pub fn pending_initial_spawns(&self) -> BTreeSet<i64> {
        self.holders
            .iter()
            .filter_map(|(&key, holder)| holder.chunk.initial_mob_spawning_pending.then_some(key))
            .collect()
    }
    pub fn refresh_full_chunks(&mut self, level: &ferraloom_game::level::Level) {
        for (&key, holder) in &mut self.holders {
            if holder.chunk.region.status != ChunkStatus::Full {
                continue;
            }
            let Some(chunk) = level.chunk(ChunkPos::unpack(key)) else {
                continue;
            };
            let mut generated = holder.chunk.region.chunk.clone();
            generated.sections = chunk.sections().cloned().collect();
            generated.world_surface = *chunk
                .heightmap(ferraloom_game::level::heightmap::Heightmap::WorldSurface)
                .expect("attached full heightmap");
            generated.ocean_floor = *chunk
                .heightmap(ferraloom_game::level::heightmap::Heightmap::OceanFloor)
                .expect("attached full heightmap");
            let mut updated =
                ferraloom_worldgen::region::RegionChunk::new(generated, ChunkStatus::Full);
            std::mem::swap(&mut updated.structures, &mut holder.chunk.region.structures);
            updated.block_entities = holder.chunk.region.block_entities.clone();
            holder.chunk.region = updated;
        }
    }
    pub fn save_proto_chunks(&self, registry: &RegistryAdapter, io: &IoWorker) -> Result<()> {
        for (&key, holder) in &self.holders {
            let proto = &holder.chunk;
            if proto.region.status > ChunkStatus::Empty && proto.region.status < ChunkStatus::Full {
                let disk = proto
                    .capture(&self.context, registry)
                    .map_err(anyhow::Error::msg)?;
                io.store(ChunkPos::unpack(key), Some(disk.write()?))
                    .wait()?;
            }
        }
        Ok(())
    }
    pub fn save_unclaimed_protos(
        &mut self,
        retained: &std::collections::BTreeSet<i64>,
        registry: &RegistryAdapter,
        io: &IoWorker,
    ) -> Result<()> {
        let drop = self
            .holders
            .iter()
            .filter_map(|(&key, holder)| {
                let pos = ChunkPos::unpack(key);
                (holder.chunk.region.status < ChunkStatus::Full && !retained.contains(&pos.pack()))
                    .then_some(pos)
            })
            .collect::<Vec<_>>();
        for pos in drop {
            let proto = &self.holders[&pos.pack()].chunk;
            if proto.region.status > ChunkStatus::Empty {
                let disk = proto
                    .capture(&self.context, registry)
                    .map_err(anyhow::Error::msg)?;
                io.store(pos, Some(disk.write()?)).wait()?;
            }
            self.forget(pos);
        }
        Ok(())
    }
    pub fn forget(&mut self, pos: ChunkPos) {
        self.holders.remove(&pos.pack());
        self.failed_loads.remove(&pos.pack());
        self.snapshots.remove_chunk(pos);
        self.light.forget_chunk_sources(pos);
    }
    /// Runtime may request any saved status. A missing chunk is never fabricated
    /// at FULL, and every per-step result is retained even when a later step fails.
    pub fn generate(
        &mut self,
        request: u64,
        pos: ChunkPos,
        target: ChunkStatus,
        registry: &RegistryAdapter,
        io: &IoWorker,
        cancelled: Option<&AtomicBool>,
    ) -> Result<GenerationOutput> {
        self.revision = GenerationRevision(
            self.revision
                .0
                .checked_add(1)
                .context("generation revision exhausted")?,
        );
        let mut task = ChunkGenerationTask::new(request, pos, target);
        let mut touched = BTreeSet::new();
        let mut skips = Vec::new();
        loop {
            if cancelled.is_some_and(|token| token.load(Ordering::Relaxed)) {
                task.cancel();
            }
            let Some(layer) = task
                .next_layer(|p| self.holders.get(&p.pack()).map(|h| h.persisted))
                .map_err(|e| anyhow::anyhow!("{e:?}"))?
            else {
                break;
            };
            for position in layer.positions {
                let key = position.pack();
                if layer.status == ChunkStatus::Empty {
                    if let Some(error) = self.failed_loads.get(&key) {
                        anyhow::bail!("generation dependency failed: {error:?}");
                    }
                    if let std::collections::btree_map::Entry::Vacant(entry) =
                        self.holders.entry(key)
                    {
                        let loaded = (|| -> Result<ProtoChunk> {
                            if let Some(tag) = io.load(position).wait()? {
                                ProtoChunk::load(
                                    &self.context,
                                    registry,
                                    position,
                                    SerializableChunkData::parse(tag)?,
                                )
                                .map_err(anyhow::Error::msg)
                            } else {
                                Ok(ProtoChunk::empty(&self.context, position)?)
                            }
                        })();
                        let proto = match loaded {
                            Ok(proto) => proto,
                            Err(error) => {
                                self.failed_loads.insert(
                                    key,
                                    ferraloom_game::level::generation::GenerationError::Failed {
                                        position,
                                        status: ChunkStatus::Empty,
                                        message: error.to_string(),
                                    },
                                );
                                return Err(error);
                            }
                        };
                        let persisted = proto.region.status;
                        entry.insert(GenerationChunkHolder::new(proto, persisted));
                    }
                    continue;
                }
                let holder = self
                    .holders
                    .get(&key)
                    .context("generation holder missing")?;
                if holder.completed >= layer.status {
                    continue;
                }
                let step = holder.step_to(layer.status).clone();
                if let Some(error) = &holder.failure {
                    anyhow::bail!("generation dependency failed: {error:?}");
                }
                let result = (|| -> Result<()> {
                    match step.task {
                        ChunkStatusTask::InitializeLight | ChunkStatusTask::Light => {
                            self.apply_light(position, step.target)?
                        }
                        ChunkStatusTask::Full => {
                            let proto = &mut self.holders.get_mut(&key).unwrap().chunk;
                            proto.disk = proto
                                .capture(&self.context, registry)
                                .map_err(anyhow::Error::msg)?;
                            proto.region.status = ChunkStatus::Full;
                        }
                        ChunkStatusTask::GenerateSpawn => {
                            // Explicit hook: initial mob spawning awaits NaturalSpawner.
                            // It does not gate block generation or invent entity NBT.
                            let proto = &mut self.holders.get_mut(&key).unwrap().chunk;
                            proto.initial_mob_spawning_pending = matches!(
                                self.context.generator.as_ref(),
                                ferraloom_worldgen::generator::ChunkGenerator::Noise(noise) if !noise.settings.disable_mob_generation
                            );
                            proto.region.status = proto.region.status.max(ChunkStatus::Spawn);
                        }
                        _ => {
                            let radius = step.direct.radius().unwrap_or(0) as i32;
                            let mut neighborhood = Vec::new();
                            for x in position.x - radius..=position.x + radius {
                                for z in position.z - radius..=position.z + radius {
                                    let neighbor = ChunkPos::new(x, z);
                                    let distance =
                                        x.abs_diff(position.x).max(z.abs_diff(position.z)) as usize;
                                    let holder = self
                                        .holders
                                        .get(&neighbor.pack())
                                        .context("generation dependency missing")?;
                                    if let Some(required) = step.direct.get(distance) {
                                        ensure!(
                                            holder.completed >= required,
                                            "dependency {neighbor:?} incomplete at {required:?}"
                                        );
                                    }
                                    neighborhood.push(neighbor);
                                }
                            }
                            // Move the actual neighborhood out, never clone writable
                            // chunks. Restore every neighbor before propagating errors.
                            let mut owned = neighborhood
                                .iter()
                                .map(|p| {
                                    self.holders.remove(&p.pack()).expect("checked dependency")
                                })
                                .collect::<Vec<_>>();
                            let mut regions = owned
                                .iter_mut()
                                .map(|h| {
                                    std::mem::replace(&mut h.chunk.region, self.placeholder.clone())
                                })
                                .collect::<Vec<_>>();
                            let result =
                                self.context
                                    .apply(&mut regions, [position.x, position.z], &step);
                            for ((neighbor, mut holder), region) in
                                neighborhood.into_iter().zip(owned.drain(..)).zip(regions)
                            {
                                holder.chunk.region = region;
                                self.holders.insert(neighbor.pack(), holder);
                                touched.insert(neighbor.pack());
                            }
                            let stage_skips = result?;
                            skips.extend(stage_skips);
                            if matches!(
                                step.task,
                                ChunkStatusTask::GenerateStructureStarts
                                    | ChunkStatusTask::LoadStructureStarts
                            ) {
                                self.structure_check.on_structure_load(
                                    [position.x, position.z],
                                    self.holders[&key]
                                        .chunk
                                        .region
                                        .structures
                                        .known_valid_starts(),
                                );
                            }
                        }
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    self.holders.get_mut(&key).unwrap().failure =
                        Some(ferraloom_game::level::generation::GenerationError::Failed {
                            position,
                            status: step.target,
                            message: error.to_string(),
                        });
                    return Err(error);
                }
                self.holders.get_mut(&key).unwrap().complete(layer.status);
                touched.insert(key);
            }
        }
        let mut neighbors = Vec::new();
        let mut full_structures = Vec::new();
        for key in touched {
            let proto = &self.holders[&key].chunk;
            if key != pos.pack() && proto.region.status == ChunkStatus::Full {
                full_structures.push(StructureSnapshot {
                    pos: ChunkPos::unpack(key),
                    revision: self.revision,
                    data: storage::StructureData::from_nbt(proto.region.structures.to_nbt()?)?,
                    structures: proto.region.structures.clone(),
                    unattached_disk: Some(
                        proto
                            .capture(&self.context, registry)
                            .map_err(anyhow::Error::msg)?,
                    ),
                });
            }
            if key != pos.pack()
                && proto.region.status < ChunkStatus::Full
                && proto.region.status > ChunkStatus::Empty
            {
                let disk = proto
                    .capture(&self.context, registry)
                    .map_err(anyhow::Error::msg)?;
                neighbors.push((ChunkPos::unpack(key), disk));
            }
        }
        let proto = &self.holders[&pos.pack()].chunk;
        let disk = proto
            .capture(&self.context, registry)
            .map_err(anyhow::Error::msg)?;
        let sections = proto.region.chunk.sections.clone();
        let light_neighbors = self
            .holders
            .iter()
            .filter_map(|(&key, holder)| {
                let neighbor = ChunkPos::unpack(key);
                if neighbor != pos
                    && neighbor.x.abs_diff(pos.x).max(neighbor.z.abs_diff(pos.z)) <= 1
                    && holder.completed >= ChunkStatus::InitializeLight
                {
                    Some(GenerationLightNeighbor {
                        pos: neighbor,
                        revision: self.revision,
                        sections: holder.chunk.region.chunk.sections.clone(),
                        disk: holder.chunk.disk.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();
        let terrain = Terrain {
            pos,
            revision: self.revision,
            light_neighbors,
            surface: surface_heights_in_dimension(&sections, self.context.dimension.min_y),
            sections,
            disk,
            structures: proto.region.structures.clone(),
        };
        Ok(GenerationOutput {
            terrain,
            neighbors,
            skips,
            full_structures,
            initial_mob_spawning_pending: proto.initial_mob_spawning_pending,
            statuses: self.completed_statuses(),
            proto_biomes: self
                .holders
                .iter()
                .filter(|(_, holder)| holder.completed >= ChunkStatus::Biomes)
                .map(|(&key, holder)| ProtoBiomeSnapshot {
                    pos: ChunkPos::unpack(key),
                    revision: self.revision,
                    min_section: self.context.dimension.min_y >> 4,
                    biomes: holder
                        .chunk
                        .region
                        .chunk
                        .sections
                        .iter()
                        .map(|section| section.biomes().clone())
                        .collect(),
                })
                .collect(),
        })
    }
    fn apply_light(&mut self, pos: ChunkPos, target: ChunkStatus) -> Result<()> {
        // The light executor reads owned snapshots only; its status dependencies
        // have completed before source propagation or disk-layer initialization.
        for (&key, holder) in &self.holders {
            if holder.chunk.region.status >= ChunkStatus::Terrain {
                self.snapshots.insert_chunk(
                    ChunkPos::unpack(key),
                    holder
                        .chunk
                        .region
                        .chunk
                        .sections
                        .iter()
                        .cloned()
                        .map(|s| Some(Cow::Owned(s)))
                        .collect(),
                );
            }
        }
        let holder = self.holders.get(&pos.pack()).unwrap();
        let lighted = holder.chunk.light_correct;
        let disk_sections = holder.chunk.disk.sections.clone().unwrap_or_default();
        let mut engine = std::mem::replace(
            &mut self.light,
            LevelLightEngine::new(true, self.context.dimension.has_skylight),
        );
        let snapshots = std::mem::replace(
            &mut self.snapshots,
            SectionSnapshots::new(
                self.context.dimension.min_y >> 4,
                (self.context.dimension.min_y + self.context.dimension.height) >> 4,
            ),
        );
        let work = self.light_pool.submit(move || -> Result<_> {
            let updates = if target == ChunkStatus::InitializeLight {
                for section in &disk_sections {
                    if let Some(y) = section.y {
                        for (layer, data) in [
                            (LightLayer::Sky, &section.sky_light),
                            (LightLayer::Block, &section.block_light),
                        ] {
                            if let Some(bytes) = data {
                                let bytes = bytes
                                    .0
                                    .iter()
                                    .map(|&b| b as u8)
                                    .collect::<Vec<_>>()
                                    .into_boxed_slice()
                                    .try_into()
                                    .map_err(|_| anyhow::anyhow!("invalid saved light length"))?;
                                engine.queue_section_data(
                                    layer,
                                    SectionPos::new(pos.x, i32::from(y), pos.z),
                                    Some(ferraloom_light::DataLayer::from_bytes(bytes)),
                                );
                            }
                        }
                    }
                }
                engine.initialize_light(&snapshots, pos, lighted)
            } else {
                engine.light_chunk(&snapshots, pos, lighted)
            };
            Ok((engine, snapshots, updates))
        });
        let (engine, snapshots, updates) = work
            .blocking_recv()
            .context("generation light worker terminated")??;
        self.light = engine;
        self.snapshots = snapshots;
        for update in updates.sections {
            let Some(holder) = self.holders.get_mut(&update.section.chunk().pack()) else {
                continue;
            };
            let sections = holder.chunk.disk.sections.get_or_insert_default();
            let y = update.section.y as i8;
            let index = if let Some(index) = sections.iter().position(|s| s.y == Some(y)) {
                index
            } else {
                sections.push(storage::ChunkSection {
                    y: Some(y),
                    ..Default::default()
                });
                sections.len() - 1
            };
            let data = update
                .data
                .as_ref()
                .map(|d| storage::ByteArray(d.to_bytes().iter().map(|&b| b as i8).collect()));
            match update.layer {
                LightLayer::Sky => sections[index].sky_light = data,
                LightLayer::Block => sections[index].block_light = data,
            }
        }
        let proto = &mut self.holders.get_mut(&pos.pack()).unwrap().chunk;
        proto.region.status = proto.region.status.max(target);
        if target == ChunkStatus::Light {
            proto.light_correct = true;
        }
        Ok(())
    }
}
