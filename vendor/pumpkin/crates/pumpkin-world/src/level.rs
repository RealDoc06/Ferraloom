use crate::chunk::format::linear::LinearV2File;
use crate::chunk::format::pump::PumpFile;
use crate::chunk_system::{ChunkListener, ChunkLoading, GenerationSchedule, LevelChannel};
use crate::generation::generator::WorldGenerator;
use crate::lighting::DynamicLightEngine;
use crate::{
    chunk::{
        ChunkData, ChunkEntityData, ChunkReadingError,
        format::anvil::AnvilChunkFile,
        io::{
            Dirtiable, FileIO, LoadedData,
            file_manager::{ChunkFileManager, LevelFileIO},
        },
        palette::has_random_ticking_fluid,
    },
    generation::get_world_gen_with_all_settings,
    tick::{OrderedTick, ScheduledTick, TickPriority},
    world::WorldPortalExt,
};
use arc_swap::ArcSwap;
use crossbeam::queue::SegQueue;
use dashmap::{DashMap, Entry};
use pumpkin_config::{chunk::ChunkConfig, lighting::LightingEngineConfig, world::LevelConfig};
use pumpkin_data::biome::Biome;
use pumpkin_data::dimension::Dimension;
use pumpkin_data::{Block, BlockStateId, block_properties::has_random_ticks, fluid::Fluid};
use pumpkin_util::math::{position::BlockPos, vector2::Vector2};
use pumpkin_util::world_seed::Seed;
use rustc_hash::FxHashSet;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    thread,
};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, trace, warn};
// use tokio::runtime::Handle;
use tokio::{
    select,
    sync::{
        mpsc::{self, Receiver},
        oneshot,
    },
    task::JoinHandle,
};
use tokio_util::task::TaskTracker;

pub use crate::chunk_system::activation::ChunkActivationDemand;

pub type SyncChunk = Arc<ChunkData>;
pub type SyncEntityChunk = Arc<ChunkEntityData>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadedChunkChange {
    Loaded(Vector2<i32>),
    Unloaded(Vector2<i32>),
}

pub type ChunkSaver =
    LevelFileIO<LinearV2File<ChunkData>, AnvilChunkFile<ChunkData>, PumpFile<ChunkData>>;

pub type EntitySaver = LevelFileIO<
    LinearV2File<ChunkEntityData>,
    AnvilChunkFile<ChunkEntityData>,
    PumpFile<ChunkEntityData>,
>;

/// The `Level` module provides functionality for working with chunks within or outside a Minecraft world.
///
/// Key features include:
///
/// - **Chunk Loading:** Efficiently loads chunks from disk.
/// - **Chunk Caching:** Stores accessed chunks in memory for faster access.
/// - **Chunk Generation:** Generates new chunks on-demand using a specified `WorldGenerator`.
///
/// For more details on world generation, refer to the `WorldGenerator` module.
pub struct Level {
    pub seed: Seed,
    pub world_portal: ArcSwap<Option<Arc<dyn WorldPortalExt>>>,
    pub level_folder: Arc<LevelFolder>,
    pub lighting_config: LightingEngineConfig,

    /// Counts the number of ticks that have been scheduled for this world
    schedule_tick_counts: AtomicU64,

    // Chunks that are paired with chunk watchers. When a chunk is no longer watched, it is removed
    // from the loaded chunks map and sent to the underlying ChunkIO
    pub loaded_chunks: Arc<DashMap<Vector2<i32>, SyncChunk>>,
    pub(crate) loaded_chunk_changes: Arc<SegQueue<LoadedChunkChange>>,
    pub(crate) chunk_activation: crate::chunk_system::activation::ChunkActivation,
    pub(crate) chunk_lifecycle: Arc<std::sync::RwLock<()>>,
    pub chunk_unload: Arc<crate::chunk_system::unload::ChunkUnloadCoordinator>,
    pub chunk_activation_changed: Arc<tokio::sync::Notify>,
    pub(crate) chunk_activation_revision: Arc<AtomicU64>,
    loaded_entity_chunks: Arc<DashMap<Vector2<i32>, SyncEntityChunk>>,
    pending_entity_saves: DashMap<Vector2<i32>, SyncEntityChunk>,
    entity_save_scheduled: AtomicBool,
    entity_lifecycle: Mutex<()>,
    save_failure: Mutex<Option<String>>,
    pub(crate) fatal_save_failure: Arc<Mutex<Option<String>>>,
    pub chunks_with_scheduled_ticks: Arc<dashmap::DashSet<Vector2<i32>>>,
    pub chunk_loading: Mutex<ChunkLoading>,

    chunk_watchers: Arc<DashMap<Vector2<i32>, usize>>,

    pub chunk_saver: Arc<ChunkSaver>,
    entity_saver: Arc<EntitySaver>,

    pub world_gen: ArcSwap<WorldGenerator>,

    /// Handles runtime lighting updates
    pub light_engine: DynamicLightEngine,

    /// Tracks tasks associated with this world instance
    tasks: TaskTracker,
    runtime: tokio::runtime::Handle,
    pub chunk_system_tasks: TaskTracker,
    /// Notification that interrupts tasks for shutdown
    pub cancel_token: CancellationToken,

    pub shut_down_chunk_system: AtomicBool,
    pub should_save: AtomicBool,
    pub should_unload: AtomicBool,
    /// Whether periodic autosaving is enabled. Toggled by `/save-off` and `/save-on`;
    /// a manual `/save-all` still saves while this is `false`.
    pub save_enabled: AtomicBool,
    /// Number of ticks between autosave checks. If 0, autosave is disabled.
    pub autosave_ticks: u64,

    pending_entity_generations: Arc<DashMap<Vector2<i32>, Vec<oneshot::Sender<SyncEntityChunk>>>>,

    pub level_channel: Arc<LevelChannel>,
    pub thread_tracker: Mutex<Vec<thread::JoinHandle<()>>>,
    pub chunk_listener: Arc<ChunkListener>,
}

pub struct TickData {
    pub block_ticks: Vec<OrderedTick<&'static Block>>,
    pub fluid_ticks: Vec<OrderedTick<&'static Fluid>>,
    pub random_ticks: Vec<RandomTickSample>,
}

#[derive(Clone, Copy)]
pub struct RandomTickSample {
    pub position: BlockPos,
    pub tick_block: bool,
    pub tick_fluid: bool,
}

pub struct LevelFolder {
    pub root_folder: PathBuf,
    pub dim_folder: PathBuf,
    pub region_folder: PathBuf,
    pub entities_folder: PathBuf,
    pub poi_folder: PathBuf,
}

impl Level {
    #[must_use]
    #[expect(clippy::too_many_lines)]
    pub fn from_root_folder(
        level_config: &LevelConfig,
        root_folder: PathBuf,
        seed: i64,
        dimension: Dimension,
    ) -> Arc<Self> {
        let (namespace, name) = match dimension.minecraft_name.split_once(':') {
            Some((ns, n)) => (ns, n),
            None => ("minecraft", dimension.minecraft_name),
        };

        // 26.2 canonical layout: root_folder/dimensions/<namespace>/<name>
        let canonical_dim_folder = root_folder.join("dimensions").join(namespace).join(name);

        // Check if canonical 26.2 folder exists, or fall back to pre-26.2 legacy folders
        let dim_folder = if canonical_dim_folder.exists() {
            canonical_dim_folder
        } else if dimension.minecraft_name == Dimension::OVERWORLD.minecraft_name
            && root_folder.join("region").exists()
        {
            root_folder.clone()
        } else if dimension.minecraft_name == Dimension::THE_NETHER.minecraft_name
            && root_folder.join("DIM-1").join("region").exists()
        {
            root_folder.join("DIM-1")
        } else if dimension.minecraft_name == Dimension::THE_END.minecraft_name
            && root_folder.join("DIM1").join("region").exists()
        {
            root_folder.join("DIM1")
        } else {
            canonical_dim_folder
        };

        let region_folder = dim_folder.join("region");
        let entities_folder = dim_folder.join("entities");
        let poi_folder = dim_folder.join("poi");

        let _ = std::fs::create_dir_all(&region_folder);
        let _ = std::fs::create_dir_all(&entities_folder);
        let _ = std::fs::create_dir_all(&poi_folder);

        let level_folder = Arc::new(LevelFolder {
            root_folder,
            dim_folder,
            region_folder,
            entities_folder,
            poi_folder,
        });

        let main_folder = &level_folder.root_folder;

        let mut is_flat = false;
        let mut flat_layers = Vec::new();
        let mut flat_biome = "minecraft:plains".to_string();
        let mut generator_settings_name: Option<String> = None;
        let mut biome_source: Option<crate::world_info::BiomeSource> = None;
        let mut structure_overrides: Option<Vec<String>> = None;

        if let Some(wgs) = crate::world_info::data_files::read_world_gen_settings(main_folder)
            && let Some(dim_settings) = wgs.dimensions.get(dimension.minecraft_name)
        {
            biome_source.clone_from(&dim_settings.generator.biome_source);

            if dim_settings.generator.generator_type == "minecraft:flat" {
                is_flat = true;
                let flat_settings = dim_settings
                    .generator
                    .settings
                    .as_ref()
                    .and_then(crate::world_info::GeneratorSettings::as_flat_settings)
                    .or_else(|| {
                        crate::world_info::FlatLevelGeneratorPreset::from_name("classic_flat")
                            .map(|p| p.settings)
                    });
                if let Some(flat_settings) = flat_settings {
                    flat_layers = flat_settings.to_flat_layers();
                    structure_overrides = flat_settings.structure_overrides_vec();
                    flat_biome = flat_settings.biome;
                }
            } else if let Some(crate::world_info::GeneratorSettings::Reference(s)) =
                &dim_settings.generator.settings
            {
                generator_settings_name = Some(s.clone());
            }
        }

        let dim_min_y = dimension.min_y;
        let dim_height = dimension.height;
        let seed = Seed(seed as u64);
        let world_gen: Arc<WorldGenerator> = Arc::from(get_world_gen_with_all_settings(
            seed,
            dimension,
            is_flat,
            flat_layers,
            flat_biome,
            generator_settings_name.as_deref(),
            biome_source.as_ref(),
            structure_overrides.as_deref(),
        ));

        let chunk_saver = match &level_config.chunk {
            ChunkConfig::Linear => Arc::new(ChunkSaver::Linear(ChunkFileManager::new(()))),
            ChunkConfig::Anvil(config) => {
                Arc::new(ChunkSaver::Anvil(ChunkFileManager::new(config.clone())))
            }
            ChunkConfig::Pump => Arc::new(ChunkSaver::Pump(ChunkFileManager::new(()))),
        };
        let entity_saver = match &level_config.chunk {
            ChunkConfig::Linear => Arc::new(EntitySaver::Linear(ChunkFileManager::new(()))),
            ChunkConfig::Anvil(config) => {
                Arc::new(EntitySaver::Anvil(ChunkFileManager::new(config.clone())))
            }
            ChunkConfig::Pump => Arc::new(EntitySaver::Pump(ChunkFileManager::new(()))),
        };

        let pending_entity_generations = Arc::new(DashMap::new());
        let level_channel = Arc::new(LevelChannel::new());
        let thread_tracker = Mutex::new(Vec::new());
        let listener = Arc::new(ChunkListener::new());

        let level_ref = Arc::new(Self {
            seed,
            world_portal: ArcSwap::new(Arc::new(None)),
            world_gen: ArcSwap::new(world_gen),
            level_folder,
            lighting_config: level_config.lighting,
            light_engine: DynamicLightEngine::new(dim_min_y, dim_min_y + dim_height),
            chunk_saver,
            entity_saver,
            schedule_tick_counts: AtomicU64::new(0),
            loaded_chunks: Arc::new(DashMap::new()),
            loaded_chunk_changes: Arc::new(SegQueue::new()),
            chunk_activation: crate::chunk_system::activation::ChunkActivation::default(),
            chunk_lifecycle: Arc::new(std::sync::RwLock::new(())),
            chunk_unload: Arc::new(crate::chunk_system::unload::ChunkUnloadCoordinator::default()),
            chunk_activation_changed: Arc::new(tokio::sync::Notify::new()),
            chunk_activation_revision: Arc::new(AtomicU64::new(0)),
            loaded_entity_chunks: Arc::new(DashMap::new()),
            pending_entity_saves: DashMap::new(),
            entity_save_scheduled: AtomicBool::new(false),
            entity_lifecycle: Mutex::new(()),
            save_failure: Mutex::new(None),
            fatal_save_failure: Arc::new(Mutex::new(None)),
            chunks_with_scheduled_ticks: Arc::new(dashmap::DashSet::new()),
            chunk_loading: Mutex::new(ChunkLoading::new(level_channel.clone())),
            chunk_watchers: Arc::new(DashMap::new()),
            tasks: TaskTracker::new(),
            runtime: tokio::runtime::Handle::current(),
            chunk_system_tasks: TaskTracker::new(),
            cancel_token: CancellationToken::new(),
            shut_down_chunk_system: AtomicBool::new(false),
            should_save: AtomicBool::new(false),
            should_unload: AtomicBool::new(false),
            save_enabled: AtomicBool::new(true),
            autosave_ticks: level_config.autosave_ticks,
            pending_entity_generations,
            level_channel: level_channel.clone(),
            thread_tracker,
            chunk_listener: listener.clone(),
        });

        GenerationSchedule::create(
            4,
            level_ref.clone(),
            level_channel,
            listener,
            level_ref
                .thread_tracker
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_mut(),
        );

        level_ref
    }

    pub fn set_world_gen(&self, generator: Arc<WorldGenerator>) {
        self.world_gen.store(generator);
    }

    #[must_use]
    pub fn world_gen(&self) -> Arc<WorldGenerator> {
        self.world_gen.load_full()
    }

    pub fn spawn_entity_generation(self: &Arc<Self>, pos: Vector2<i32>) {
        let level = self.clone();
        rayon::spawn(move || {
            let arc_chunk = Arc::new(ChunkEntityData {
                x: pos.x,
                z: pos.y,
                data: std::sync::Mutex::new(Vec::new()),
                live: AtomicBool::new(false),
                dirty: AtomicBool::new(false),
            });

            let arc_chunk = level.publish_entity_chunk(arc_chunk);

            if let Some((_, waiters)) = level.pending_entity_generations.remove(&pos) {
                for tx in waiters {
                    let _ = tx.send(arc_chunk.clone());
                }
            }
        });
    }

    /// Spawns a task associated with this world. All tasks spawned with this method are awaited
    /// when the client. This means tasks should complete in a reasonable (no looping) amount of time.
    pub fn spawn_task<F>(&self, task: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.tasks.spawn_on(task, &self.runtime)
    }

    pub async fn shutdown(&self) -> Result<(), String> {
        let world_id = self.level_folder.root_folder.display();
        info!("Saving level ({})...", world_id);
        self.cancel_token.cancel();
        self.shut_down_chunk_system.store(true, Ordering::Relaxed);
        self.level_channel.notify();

        self.tasks.close();
        self.chunk_system_tasks.close();

        let handles = {
            let mut lock = self
                .thread_tracker
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            lock.drain(..).collect::<Vec<_>>()
        };

        let handle_count = handles.len();
        info!("Joining {} threads for {}...", handle_count, world_id);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let _ = std::thread::Builder::new()
            .name("Thread-Joiner".into())
            .spawn(move || {
                let mut failed_count = 0;
                for handle in handles {
                    if handle.join().is_err() {
                        failed_count += 1;
                    }
                }
                let _ = tx.send(failed_count);
            });

        match timeout(Duration::from_secs(3), rx).await {
            Ok(Ok(failed_count)) => {
                if failed_count > 0 {
                    warn!(
                        "{} threads failed to join properly for {}.",
                        failed_count, world_id
                    );
                }
            }
            Ok(Err(_)) => {
                warn!("Thread join task panicked for {}.", world_id);
            }
            Err(_) => {
                warn!("Timed out waiting for threads to join for {}.", world_id);
            }
        }

        self.tasks.wait().await;
        self.chunk_system_tasks.wait().await;

        info!("Flushing chunk data to disk for {}...", world_id);
        self.chunk_saver.block_and_await_ongoing_tasks().await;
        info!("Flushing entity data to disk for {}...", world_id);
        self.entity_saver.block_and_await_ongoing_tasks().await;

        // Watchers must survive until all cached snapshots reach disk.
        let flushed = self.flush_chunk_files().await;
        if let Err(error) = &flushed {
            error!("Failed flushing level at shutdown: {error}");
        } else {
            self.loaded_entity_chunks.clear();
            self.entity_saver.clear_watched_chunks().await;
        }
        flushed
    }

    pub fn loaded_chunk_count(&self) -> usize {
        self.loaded_chunks.len()
    }

    pub fn list_cached(&self) {
        for entry in self.loaded_chunks.iter() {
            debug!("In map: {:?}", entry.key());
        }
    }

    /// Marks chunks as "watched" by a unique player. When no players are watching a chunk,
    /// it is removed from memory. Should only be called on chunks the player was not watching
    /// before
    pub async fn mark_chunks_as_newly_watched(&self, chunks: &[Vector2<i32>]) {
        for chunk in chunks {
            self.chunk_watchers
                .entry(*chunk)
                .and_modify(|count| *count = count.saturating_add(1))
                .or_insert(1);
        }

        self.entity_saver
            .watch_chunks(&self.level_folder, chunks)
            .await;
    }

    /// Marks chunks no longer "watched" by a unique player. When no players are watching a chunk,
    /// it is removed from memory. Should only be called on chunks the player was watching before
    pub async fn mark_chunks_as_not_watched(
        &self,
        chunks: impl IntoIterator<Item = impl std::borrow::Borrow<Vector2<i32>>>,
    ) -> Vec<Vector2<i32>> {
        let chunks_vec: Vec<Vector2<i32>> = chunks.into_iter().map(|c| *c.borrow()).collect();
        let chunks_to_clean = self.release_chunk_watchers(&chunks_vec);
        self.finish_entity_chunk_unwatch(&chunks_vec).await;
        chunks_to_clean
    }

    /// Decrement watcher counts before the caller snapshots live chunk entities.
    pub fn release_chunk_watchers(&self, chunks: &[Vector2<i32>]) -> Vec<Vector2<i32>> {
        let mut chunks_to_clean = Vec::new();
        for chunk in chunks {
            if let Entry::Occupied(mut entry) = self.chunk_watchers.entry(*chunk) {
                *entry.get_mut() = entry.get().saturating_sub(1);
                if *entry.get() == 0 {
                    entry.remove();
                    chunks_to_clean.push(*chunk);
                }
            }
        }

        chunks_to_clean
    }

    /// Release serializer references after the caller has snapshotted and queued
    /// cleanup. Pass the original removed watcher references, including chunks
    /// that remain watched by somebody else.
    pub async fn finish_entity_chunk_unwatch(&self, chunks: &[Vector2<i32>]) {
        self.entity_saver
            .unwatch_chunks(&self.level_folder, chunks)
            .await;
    }

    /// Returns whether the chunk should be removed from memory
    #[inline]
    pub async fn mark_chunk_as_not_watched(&self, chunk: Vector2<i32>) -> bool {
        !self.mark_chunks_as_not_watched([chunk]).await.is_empty()
    }

    // In Level::clean_entity_chunks()
    pub fn clean_entity_chunks(
        self: &Arc<Self>,
        chunks: impl IntoIterator<Item = impl std::borrow::Borrow<Vector2<i32>>>,
    ) {
        let _guard = self
            .entity_lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut queued = false;
        for pos in chunks {
            let pos = pos.borrow();
            if self.is_chunk_watched(pos) {
                continue;
            }
            if let Some((_, chunk)) = self.loaded_entity_chunks.remove(pos) {
                let snapshot = Self::entity_snapshot(&chunk);
                chunk.live.store(false, Ordering::Relaxed);
                self.pending_entity_saves.insert(*pos, snapshot);
                queued = true;
            }
        }

        // One outstanding marker drains the latest retained snapshot for each
        // position. Retries do not create duplicate queued chunks.
        if queued && !self.entity_save_scheduled.swap(true, Ordering::Relaxed) {
            self.level_channel.request_entity_save();
        }
    }

    fn entity_snapshot(chunk: &ChunkEntityData) -> SyncEntityChunk {
        Arc::new(ChunkEntityData {
            x: chunk.x,
            z: chunk.z,
            data: Mutex::new(
                chunk
                    .data
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone(),
            ),
            live: AtomicBool::new(false),
            dirty: AtomicBool::new(true),
        })
    }

    fn cached_entity_chunk(&self, position: Vector2<i32>) -> Option<SyncEntityChunk> {
        let _guard = self
            .entity_lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(chunk) = self.loaded_entity_chunks.get(&position) {
            return Some(chunk.clone());
        }
        let snapshot = self.pending_entity_saves.get(&position)?.clone();
        // Activation consumes NBT, so reads must not share the immutable snapshot
        // whose write is still queued (or waiting for a failed IO retry).
        let chunk = Self::entity_snapshot(&snapshot);
        chunk.mark_dirty(false);
        self.loaded_entity_chunks.insert(position, chunk.clone());
        Some(chunk)
    }

    fn publish_entity_chunk(&self, chunk: SyncEntityChunk) -> SyncEntityChunk {
        let _guard = self
            .entity_lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let position = Vector2::new(chunk.x, chunk.z);
        if let Some(existing) = self.loaded_entity_chunks.get(&position) {
            return existing.clone();
        }
        let chunk = self
            .pending_entity_saves
            .get(&position)
            .map_or(chunk, |snapshot| {
                let restored = Self::entity_snapshot(&snapshot);
                restored.mark_dirty(false);
                restored
            });
        self.loaded_entity_chunks.insert(position, chunk.clone());
        chunk
    }

    pub fn get_tick_data(
        &self,
        active_chunks: &FxHashSet<Vector2<i32>>,
        block_ticking_chunks: &FxHashSet<Vector2<i32>>,
        random_tick_speed: i64,
    ) -> TickData {
        let samples_per_section = random_tick_speed.max(0);

        let mut ticks = TickData {
            block_ticks: Vec::new(),
            fluid_ticks: Vec::new(),
            random_ticks: Vec::with_capacity(active_chunks.len() * 3),
        };

        // 1. Process active chunks (random ticks, block entities)
        for pos in active_chunks {
            if let Some(chunk) = self.loaded_chunks.get(pos) {
                let chunk = chunk.value();
                if !chunk.is_post_processing_ready() {
                    continue;
                }
                let chunk_x_base = chunk.x * 16;
                let chunk_z_base = chunk.z * 16;
                let section_count = chunk.section.count;

                // Use the bitmask to skip sections
                let mask = chunk.section.randomly_ticking_mask.load(Ordering::Relaxed);
                if mask != 0 {
                    let sections = chunk
                        .section
                        .block_sections
                        .read()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    let min_y = chunk.section.min_y;

                    for i in 0..section_count {
                        if (mask & (1 << i)) == 0 {
                            continue;
                        }
                        let y_base = min_y + (i as i32 * 16);
                        for _ in 0..samples_per_section {
                            let r = rand::random::<u32>();
                            let x_offset = (r & 0xF) as usize;
                            let z_offset = (r >> 8 & 0xF) as usize;
                            let y_in_section = ((r >> 4) & 0xF) as usize;

                            let block_state_id = sections[i].get(x_offset, y_in_section, z_offset);
                            let tick_block = has_random_ticks(block_state_id);
                            let tick_fluid = has_random_ticking_fluid(block_state_id);
                            if tick_block || tick_fluid {
                                ticks.random_ticks.push(RandomTickSample {
                                    position: BlockPos::new(
                                        chunk_x_base + x_offset as i32,
                                        y_base + y_in_section as i32,
                                        chunk_z_base + z_offset as i32,
                                    ),
                                    tick_block,
                                    tick_fluid,
                                });
                            }
                        }
                    }
                }
            }
        }

        // 2. Process chunks with scheduled ticks
        // We collect keys first to avoid holding DashSet shard lock while accessing loaded_chunks (deadlock risk)
        let scheduled_chunk_pos: Vec<_> = self
            .chunks_with_scheduled_ticks
            .iter()
            .map(|p| *p)
            .collect();
        for pos in scheduled_chunk_pos {
            if !block_ticking_chunks.contains(&pos) {
                continue;
            }
            if let Some(chunk) = self.loaded_chunks.get(&pos) {
                let chunk = chunk.value();
                if !chunk.is_post_processing_ready() {
                    continue;
                }
                ticks.block_ticks.append(&mut chunk.block_ticks.step_tick());
                ticks.fluid_ticks.append(&mut chunk.fluid_ticks.step_tick());

                // Remove from set if it no longer has ticks
                if !chunk.block_ticks.has_ticks() && !chunk.fluid_ticks.has_ticks() {
                    self.chunks_with_scheduled_ticks.remove(&pos);
                }
            } else {
                self.chunks_with_scheduled_ticks.remove(&pos); // Chunk unloaded
            }
        }

        ticks.block_ticks.sort_unstable();
        ticks.fluid_ticks.sort_unstable();

        ticks
    }

    pub fn clean_entity_chunk(self: &Arc<Self>, chunk: &Vector2<i32>) {
        self.clean_entity_chunks([*chunk]);
    }

    pub fn is_chunk_watched(&self, chunk: &Vector2<i32>) -> bool {
        self.chunk_watchers.get(chunk).is_some()
    }

    pub fn clean_memory(self: &Arc<Self>) -> Vec<Vector2<i32>> {
        self.chunk_watchers.retain(|_, watcher| *watcher != 0);

        let entity_chunks_to_remove: Vec<_> = self
            .loaded_entity_chunks
            .iter()
            .filter(|entry| !self.chunk_watchers.contains_key(entry.key()))
            .map(|entry| *entry.key())
            .collect();

        // We do not clean them here because we want the caller to save any active entities in them first.

        // if the difference is too big, we can shrink the loaded chunks
        // (1024 chunks is the equivalent to a 32x32 chunks area)
        if self.chunk_watchers.capacity() - self.chunk_watchers.len() >= 4096 {
            self.chunk_watchers.shrink_to_fit();
        }

        if self.loaded_chunks.capacity() - self.loaded_chunks.len() >= 4096 {
            self.loaded_chunks.shrink_to_fit();
        }

        if self.loaded_entity_chunks.capacity() - self.loaded_entity_chunks.len() >= 4096 {
            self.loaded_entity_chunks.shrink_to_fit();
        }
        entity_chunks_to_remove
    }

    pub async fn get_or_fetch_chunk<R, F: Fn(&SyncChunk) -> R>(
        self: &Arc<Self>,
        pos: Vector2<i32>,
        f: F,
    ) -> R {
        // Check if already in memory
        if let Some(res) = self.read_chunk_sync(&pos, &f) {
            return res;
        }
        let chunk = self.fetch_chunk(pos).await;
        f(&chunk)
    }

    pub fn loaded_chunk_changes(&self) -> impl Iterator<Item = LoadedChunkChange> + '_ {
        std::iter::from_fn(|| self.loaded_chunk_changes.pop())
    }

    async fn fetch_chunk(self: &Arc<Self>, pos: Vector2<i32>) -> SyncChunk {
        let recv = self.chunk_listener.add_single_chunk_listener(pos);

        {
            let mut lock = self
                .chunk_loading
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            lock.add_ticket(pos, ChunkLoading::FULL_CHUNK_LEVEL);
            lock.send_change();
        };

        let chunk = recv
            .await
            .unwrap_or_else(|_| ChunkData::empty_sync(pos.x, pos.y));

        {
            let mut lock = self
                .chunk_loading
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            lock.remove_ticket(pos, ChunkLoading::FULL_CHUNK_LEVEL);
            lock.send_change();
        };

        chunk
    }

    async fn load_single_entity_chunk(
        &self,
        pos: Vector2<i32>,
    ) -> Result<(SyncEntityChunk, bool), ChunkReadingError> {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        self.entity_saver
            .fetch_chunks(&self.level_folder, &[pos], tx)
            .await;

        match rx.recv().await {
            Some(LoadedData::Loaded(chunk)) => Ok((chunk, false)),
            Some(LoadedData::Error((_, err))) => Err(err),
            _ => Err(ChunkReadingError::ChunkNotExist),
        }
    }

    pub fn receive_entity_chunks(
        self: &Arc<Self>,
        chunks: Vec<Vector2<i32>>,
    ) -> Receiver<(Weak<ChunkEntityData>, bool)> {
        let (sender, receiver) = mpsc::channel(64);
        let level = self.clone();

        self.spawn_task(async move {
            let cancel_notifier = level.cancel_token.cancelled();

            let fetch_task = async {
                let to_fetch: Vec<_> = chunks
                    .iter()
                    .filter(|pos| {
                        level.cached_entity_chunk(**pos).is_none_or(|chunk| {
                            let first_load = !chunk.live.load(Ordering::Relaxed);
                            let _ = sender.try_send((Arc::downgrade(&chunk), first_load));
                            false // Don't fetch
                        })
                    })
                    .copied()
                    .collect();

                if !to_fetch.is_empty() {
                    let (tx, mut rx) = tokio::sync::mpsc::channel::<
                        LoadedData<SyncEntityChunk, ChunkReadingError>,
                    >(to_fetch.len());

                    level
                        .entity_saver
                        .fetch_chunks(&level.level_folder, &to_fetch, tx)
                        .await;

                    while let Some(data) = rx.recv().await {
                        match data {
                            LoadedData::Loaded(chunk) => {
                                let chunk = level.publish_entity_chunk(chunk);
                                let first_load = !chunk.live.load(Ordering::Relaxed);
                                let _ = sender.send((Arc::downgrade(&chunk), first_load)).await;
                            }
                            LoadedData::Missing(pos) => {
                                let (tx, rx) = oneshot::channel();
                                match level.pending_entity_generations.entry(pos) {
                                    dashmap::mapref::entry::Entry::Occupied(mut entry) => {
                                        entry.get_mut().push(tx);
                                    }
                                    dashmap::mapref::entry::Entry::Vacant(entry) => {
                                        entry.insert(vec![tx]);
                                        level.spawn_entity_generation(pos);
                                    }
                                }
                                let sender_clone = sender.clone();
                                tokio::spawn(async move {
                                    if let Ok(chunk) = rx.await {
                                        let _ =
                                            sender_clone.send((Arc::downgrade(&chunk), true)).await;
                                    }
                                });
                            }
                            LoadedData::Error((pos, error)) => {
                                let message =
                                    format!("Could not load entity chunk {pos:?}: {error}");
                                error!("{message}");
                                level.record_fatal_save_failure(message);
                            }
                        }
                    }
                }
            };

            select! {
                () = cancel_notifier => {},
                () = fetch_task => {}
            }
        });

        receiver
    }

    pub async fn get_entity_chunk(
        self: &Arc<Self>,
        pos: Vector2<i32>,
    ) -> Result<SyncEntityChunk, ChunkReadingError> {
        if let Some(chunk) = self.cached_entity_chunk(pos) {
            return Ok(chunk);
        }

        match self.load_single_entity_chunk(pos).await {
            Ok((chunk, _)) => Ok(self.publish_entity_chunk(chunk)),
            Err(ChunkReadingError::ChunkNotExist) => {
                let (tx, rx) = oneshot::channel();
                match self.pending_entity_generations.entry(pos) {
                    dashmap::mapref::entry::Entry::Occupied(mut entry) => {
                        entry.get_mut().push(tx);
                    }
                    dashmap::mapref::entry::Entry::Vacant(entry) => {
                        entry.insert(vec![tx]);
                        self.spawn_entity_generation(pos);
                    }
                }
                rx.await.map_err(|_| {
                    ChunkReadingError::IoError(std::io::Error::other(
                        "Entity chunk generation stopped",
                    ))
                })
            }
            Err(error) => {
                self.record_fatal_save_failure(format!(
                    "Could not read entity chunk {pos:?}: {error}"
                ));
                Err(error)
            }
        }
    }

    pub fn get_block_state(&self, position: &BlockPos) -> BlockStateId {
        let (chunk_coordinate, relative) = position.chunk_and_chunk_relative_position();
        let id = self
            .read_chunk_sync(&chunk_coordinate, |chunk| {
                chunk.section.get_block_absolute_y(
                    relative.x as usize,
                    relative.y,
                    relative.z as usize,
                )
            })
            .flatten();

        id.unwrap_or(Block::VOID_AIR.default_state.id)
    }

    pub fn set_block_state(
        &self,
        position: &BlockPos,
        block_state_id: BlockStateId,
    ) -> BlockStateId {
        let (chunk_coordinate, relative) = position.chunk_and_chunk_relative_position();
        self.read_chunk_sync(&chunk_coordinate, |chunk| {
            let replaced_block_state_id = chunk.set_block_absolute_y(
                relative.x as usize,
                relative.y,
                relative.z as usize,
                block_state_id,
            );
            if replaced_block_state_id != block_state_id {
                chunk.mark_dirty(true);
            }
            replaced_block_state_id
        })
        .unwrap_or(Block::VOID_AIR.default_state.id)
    }

    pub async fn write_chunks(&self, chunks_to_write: Vec<(Vector2<i32>, SyncChunk)>) {
        if chunks_to_write.is_empty() {
            return;
        }

        let chunk_saver = self.chunk_saver.clone();
        let level_folder = self.level_folder.clone();

        trace!("Sending chunks to ChunkIO {:}", chunks_to_write.len());
        if let Err(error) = chunk_saver
            .save_chunks(&level_folder, chunks_to_write)
            .await
        {
            error!("Failed writing Chunk to disk {error}");
            self.record_save_failure(error.to_string());
        }
    }

    pub(crate) fn record_save_failure(&self, error: String) {
        self.save_failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_or_insert(error);
    }

    pub(crate) fn record_fatal_save_failure(&self, error: String) {
        self.fatal_save_failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_or_insert(error);
    }

    /// Save through the scheduler's FIFO writer and await physical region writes.
    pub async fn save_and_flush(&self) -> Result<(), String> {
        let completed = self.level_channel.request_flush();
        tokio::select! {
            result = completed => result.map_err(|_| "Chunk writer stopped before completing the save".to_string())?,
            () = self.cancel_token.cancelled() => Err("Level stopped before completing the save".to_string()),
        }
    }

    pub(crate) async fn flush_chunk_files(&self) -> Result<(), String> {
        let terrain = self
            .chunk_saver
            .flush()
            .await
            .map_err(|error| error.to_string());
        {
            let _guard = self
                .entity_lifecycle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for chunk in self
                .loaded_entity_chunks
                .iter()
                .filter(|entry| entry.value().is_dirty())
            {
                self.pending_entity_saves
                    .insert(*chunk.key(), Self::entity_snapshot(chunk.value()));
            }
        }
        let saved_entities = self.save_pending_entity_chunks().await;
        let flushed_entities = self
            .entity_saver
            .flush()
            .await
            .map_err(|error| error.to_string());
        // Finish all independent writes before returning the first failure.
        let written = terrain.and(saved_entities).and(flushed_entities);
        let previous = self
            .save_failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        let fatal = self
            .fatal_save_failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        fatal.or(previous).map_or(written, Err)
    }

    pub(crate) async fn save_pending_entity_chunks(&self) -> Result<(), String> {
        self.entity_save_scheduled.store(false, Ordering::Relaxed);
        let snapshots: Vec<_> = self
            .pending_entity_saves
            .iter()
            .map(|entry| (*entry.key(), entry.value().clone()))
            .collect();
        self.save_entity_chunks(snapshots.clone()).await?;
        for (position, saved) in snapshots {
            self.pending_entity_saves
                .remove_if(&position, |_, current| Arc::ptr_eq(current, &saved));
        }
        Ok(())
    }

    pub(crate) async fn save_entity_chunks(
        &self,
        chunks_to_write: Vec<(Vector2<i32>, SyncEntityChunk)>,
    ) -> Result<(), String> {
        if chunks_to_write.is_empty() {
            return Ok(());
        }

        let chunk_saver = self.entity_saver.clone();
        let level_folder = self.level_folder.clone();

        trace!("Sending chunks to ChunkIO {:}", chunks_to_write.len());
        chunk_saver
            .save_chunks(&level_folder, chunks_to_write)
            .await
            .map_err(|error| error.to_string())
    }

    pub fn is_chunk_loaded(&self, coordinates: &Vector2<i32>) -> bool {
        self.loaded_chunks.contains_key(coordinates)
    }

    pub fn read_chunk_sync<R, F: Fn(&SyncChunk) -> R>(
        &self,
        coordinates: &Vector2<i32>,
        f: F,
    ) -> Option<R> {
        self.loaded_chunks.get(coordinates).map(|x| f(x.value()))
    }

    pub fn read_entity_chunk_sync<R, F: Fn(&SyncEntityChunk) -> R>(
        &self,
        coordinates: &Vector2<i32>,
        f: F,
    ) -> Option<R> {
        self.loaded_entity_chunks
            .get(coordinates)
            .map(|x| f(x.value()))
    }

    pub fn get_rough_biome(&self, position: &BlockPos) -> &'static Biome {
        let (chunk_coordinate, relative) = position.chunk_and_chunk_relative_position();
        let id = self.read_chunk_sync(&chunk_coordinate, |chunk| {
            chunk.section.get_rough_biome_absolute_y(
                relative.x as usize,
                relative.y,
                relative.z as usize,
            )
        });
        Biome::from_id(id.flatten().unwrap_or(0)).unwrap_or(&Biome::THE_VOID)
    }

    pub fn get_entity_chunk_sync(&self, pos: &Vector2<i32>) -> Option<SyncEntityChunk> {
        self.loaded_entity_chunks
            .get(pos)
            .map(|x| x.value().clone())
    }

    #[must_use]
    pub fn live_entity_chunk_positions(&self) -> Vec<Vector2<i32>> {
        self.loaded_entity_chunks
            .iter()
            .filter(|entry| entry.value().live.load(Ordering::Relaxed))
            .map(|entry| *entry.key())
            .collect()
    }

    pub async fn get_or_fetch_entity_chunk<R, F: Fn(&SyncEntityChunk) -> R>(
        self: &Arc<Self>,
        pos: Vector2<i32>,
        f: F,
    ) -> Result<R, ChunkReadingError> {
        if let Some(res) = self.read_entity_chunk_sync(&pos, &f) {
            return Ok(res);
        }
        let chunk = self.get_entity_chunk(pos).await?;
        Ok(f(&chunk))
    }

    pub fn try_get_entity_chunk(
        &self,
        coordinates: Vector2<i32>,
    ) -> Option<dashmap::mapref::one::Ref<'_, Vector2<i32>, Arc<ChunkEntityData>>> {
        self.loaded_entity_chunks.try_get(&coordinates).try_unwrap()
    }

    pub fn schedule_block_tick(
        &self,
        block: &Block,
        block_pos: BlockPos,
        delay: u8,
        priority: TickPriority,
    ) {
        let tick_order = self.schedule_tick_counts.fetch_add(1, Ordering::Relaxed);
        let scheduled_tick = ScheduledTick {
            delay,
            position: block_pos,
            priority,
            // SAFETY: `block` is a valid reference that outlives this function call for scheduling.
            value: unsafe { &*std::ptr::from_ref::<Block>(block) },
        };

        let chunk_pos = block_pos.chunk_position();
        if self
            .read_chunk_sync(&chunk_pos, |chunk| {
                chunk.block_ticks.schedule_tick(&scheduled_tick, tick_order);
            })
            .is_some()
        {
            self.chunks_with_scheduled_ticks.insert(chunk_pos);
        }
    }

    pub fn schedule_fluid_tick(
        &self,
        fluid: &Fluid,
        block_pos: BlockPos,
        delay: u8,
        priority: TickPriority,
    ) {
        let tick_order = self.schedule_tick_counts.fetch_add(1, Ordering::Relaxed);
        let scheduled_tick = ScheduledTick {
            delay,
            position: block_pos,
            priority,
            // SAFETY: `fluid` is a valid reference that outlives this function call for scheduling.
            value: unsafe { &*std::ptr::from_ref::<Fluid>(fluid) },
        };

        let chunk_pos = block_pos.chunk_position();
        if self
            .read_chunk_sync(&chunk_pos, |chunk| {
                chunk.fluid_ticks.schedule_tick(&scheduled_tick, tick_order);
            })
            .is_some()
        {
            self.chunks_with_scheduled_ticks.insert(chunk_pos);
        }
    }

    pub fn is_block_tick_scheduled(&self, block_pos: &BlockPos, block: &Block) -> bool {
        self.read_chunk_sync(&block_pos.chunk_position(), |chunk| {
            chunk.block_ticks.is_scheduled(*block_pos, block)
        })
        .unwrap_or(false)
    }

    pub fn is_fluid_tick_scheduled(&self, block_pos: &BlockPos, fluid: &Fluid) -> bool {
        self.read_chunk_sync(&block_pos.chunk_position(), |chunk| {
            chunk.fluid_ticks.is_scheduled(*block_pos, fluid)
        })
        .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{format::anvil::SingleChunkDataSerializer, io::ChunkSerializer};
    use pumpkin_config::{chunk::AnvilChunkConfig, world::LevelConfig};
    use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
    use tempfile::TempDir;

    async fn refused_region_stage<T>(stage: &str, future: impl Future<Output = T>) -> T {
        timeout(Duration::from_secs(10), future)
            .await
            .unwrap_or_else(|_| panic!("refused-region test timed out during {stage}"))
    }

    #[tokio::test]
    #[expect(
        clippy::too_many_lines,
        reason = "Keep refused-region byte preservation and independent healthy-write controls together."
    )]
    async fn refused_container_region_survives_read_flush_and_shutdown_without_publication() {
        let directory = TempDir::new().unwrap();
        let level = Level::from_root_folder(
            &LevelConfig::default(),
            directory.path().to_path_buf(),
            0,
            Dimension::OVERWORLD,
        );
        let position = Vector2::new(0, 0);
        let block_pos = BlockPos::new(1, 64, 2);
        let source = ChunkData::empty_sync(0, 0);
        source.set_block_absolute_y(1, 64, 2, Block::CHEST.default_state.id);
        let mut hidden = NbtCompound::new();
        hidden.put_int("duration", 2400);
        let mut effect = NbtCompound::new();
        effect.put_string("id", "minecraft:speed".into());
        effect.put_int("duration", 1200);
        effect.put_compound("hidden_effect", hidden);
        let mut potion = NbtCompound::new();
        potion.put_list("custom_effects", vec![NbtTag::Compound(effect)]);
        let mut components = NbtCompound::new();
        components.put_compound("minecraft:potion_contents", potion);
        let mut item = NbtCompound::new();
        item.put_string("id", "minecraft:potion".into());
        item.put_byte("Slot", 6);
        item.put_int("count", 1);
        item.put_compound("components", components);
        let mut container = NbtCompound::new();
        container.put_string("id", "minecraft:chest".into());
        container.put_int("x", block_pos.0.x);
        container.put_int("y", block_pos.0.y);
        container.put_int("z", block_pos.0.z);
        container.put_string("preservation_marker", "private-source-marker".into());
        container.put_list("Items", vec![NbtTag::Compound(item)]);
        source
            .pending_block_entities
            .lock()
            .unwrap()
            .insert(block_pos, container.clone());
        let source_tree = || {
            let bytes = source.to_bytes().unwrap();
            pumpkin_nbt::Nbt::read(&mut pumpkin_nbt::deserializer::NbtReadHelperJava::new(
                std::io::Cursor::new(bytes),
            ))
            .unwrap()
            .root_tag
        };
        let source_nbt = source_tree();

        // Seed actual region bytes without invoking the guarded reader. The
        // Level must subsequently use its own file cache, IO worker and scheduler.
        let path = level.level_folder.region_folder.join("r.0.0.mca");
        let mut region = AnvilChunkFile::<ChunkData>::default();
        refused_region_stage(
            "source region encoding",
            region.update_chunk(source.clone(), &AnvilChunkConfig::default()),
        )
        .await
        .unwrap();
        refused_region_stage("source region write", region.write(&path))
            .await
            .unwrap();
        drop(region);
        let before = std::fs::read(&path).unwrap();
        let mut published = level.chunk_listener.add_single_chunk_listener(position);
        {
            let mut loading = level.chunk_loading.lock().unwrap();
            // The weakest ticket requests only this chunk, without a Full halo.
            loading.add_ticket(position, ChunkLoading::MAX_LEVEL - 1);
            loading.send_change();
        };
        let failure = timeout(Duration::from_secs(10), async {
            loop {
                let failure = level.fatal_save_failure.lock().unwrap().clone();
                if let Some(failure) = failure {
                    break failure;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        let failure = match failure {
            Ok(failure) => failure,
            Err(error) => {
                let _ = refused_region_stage("refusal timeout cleanup", level.shutdown()).await;
                panic!("saved container read must report a fatal refusal: {error}");
            }
        };

        // Existing policy still completes independently attempted healthy writes.
        // Use another region so the refused source region can be compared whole.
        let healthy_pos = Vector2::new(32, 0);
        let healthy = ChunkData::empty_sync(32, 0);
        healthy.set_block_absolute_y(3, 64, 4, Block::DIAMOND_BLOCK.default_state.id);
        healthy.mark_dirty(true);
        refused_region_stage(
            "healthy region watch",
            level
                .chunk_saver
                .watch_chunks(&level.level_folder, &[healthy_pos]),
        )
        .await;
        refused_region_stage(
            "healthy region save",
            level
                .chunk_saver
                .save_chunks(&level.level_folder, vec![(healthy_pos, healthy)]),
        )
        .await
        .unwrap();
        let flushed = refused_region_stage("explicit flush", level.save_and_flush()).await;
        let after_flush = std::fs::read(&path).unwrap();
        let healthy_path = level.level_folder.region_folder.join("r.1.0.mca");
        let healthy_after_flush = std::fs::read(&healthy_path);
        let unavailable_after_flush = !level.loaded_chunks.contains_key(&position);
        let shutdown = refused_region_stage("shutdown", level.shutdown()).await;

        assert!(failure.contains("minecraft:chest"));
        assert!(failure.contains("slot Some(6)"));
        assert!(!failure.contains("private-source-marker"));
        assert_eq!(flushed, Err(failure.clone()));
        assert_eq!(shutdown, Err(failure));
        assert!(unavailable_after_flush);
        assert!(!level.loaded_chunks.contains_key(&position));
        assert!(matches!(
            published.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        // Core receives no admitted chunk to materialize, migrate or republish.
        assert!(
            !level
                .loaded_chunk_changes()
                .any(|change| change == LoadedChunkChange::Loaded(position))
        );
        assert_eq!(after_flush, before);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(
            std::fs::read(&healthy_path).unwrap(),
            healthy_after_flush.unwrap()
        );
        // Fresh encodings can reorder compound keys; disk bytes above stay exact.
        assert_eq!(source_tree(), source_nbt);
        assert_eq!(
            source.pending_block_entities.lock().unwrap()[&block_pos],
            container
        );

        let reader =
            ChunkFileManager::<AnvilChunkFile<ChunkData>>::new(AnvilChunkConfig::default());
        let (send, mut receive) = mpsc::channel(1);
        refused_region_stage(
            "healthy region fresh read",
            reader.fetch_chunks(&level.level_folder, &[healthy_pos], send),
        )
        .await;
        let Some(LoadedData::Loaded(saved)) =
            refused_region_stage("healthy region response", receive.recv()).await
        else {
            panic!("independent healthy writes must survive the refused flush and shutdown");
        };
        assert_eq!(
            saved.section.get_block_absolute_y(3, 64, 4),
            Some(Block::DIAMOND_BLOCK.default_state.id)
        );
    }

    fn entity_chunk(position: Vector2<i32>, marker: i32) -> SyncEntityChunk {
        let mut entity = NbtCompound::new();
        entity.put_string("id", "minecraft:pig".to_string());
        entity.put_int("FerraloomProbe", marker);
        Arc::new(ChunkEntityData {
            x: position.x,
            z: position.y,
            data: Mutex::new(vec![entity]),
            live: AtomicBool::new(false),
            dirty: AtomicBool::new(true),
        })
    }

    async fn saved_entity_marker(level: &Level, position: Vector2<i32>) -> i32 {
        let reader = ChunkFileManager::<AnvilChunkFile<ChunkEntityData>>::new(
            pumpkin_config::chunk::AnvilChunkConfig::default(),
        );
        let (send, mut receive) = mpsc::channel(1);
        reader
            .fetch_chunks(&level.level_folder, &[position], send)
            .await;
        let Some(LoadedData::Loaded(saved)) = receive.recv().await else {
            panic!("Saved entity chunk must be readable through a fresh region cache");
        };
        saved.data.lock().unwrap()[0]
            .get_int("FerraloomProbe")
            .unwrap()
    }

    #[tokio::test]
    async fn final_entity_region_unwatch_preserves_other_saved_chunks() {
        let directory = TempDir::new().unwrap();
        let level = Level::from_root_folder(
            &LevelConfig::default(),
            directory.path().to_path_buf(),
            0,
            Dimension::OVERWORLD,
        );
        let a = Vector2::new(0, 0);
        let b = Vector2::new(1, 0);
        level
            .entity_saver
            .watch_chunks(&level.level_folder, &[a, b])
            .await;
        level
            .save_entity_chunks(vec![(a, entity_chunk(a, 37))])
            .await
            .unwrap();
        level
            .entity_saver
            .unwatch_chunks(&level.level_folder, &[a])
            .await;
        level
            .entity_saver
            .unwatch_chunks(&level.level_folder, &[b])
            .await;
        assert_eq!(saved_entity_marker(&level, a).await, 37);
        level.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_preserves_cached_watched_entity_chunks() {
        let directory = TempDir::new().unwrap();
        let level = Level::from_root_folder(
            &LevelConfig::default(),
            directory.path().to_path_buf(),
            0,
            Dimension::OVERWORLD,
        );
        let a = Vector2::new(0, 0);
        let b = Vector2::new(1, 0);
        level
            .entity_saver
            .watch_chunks(&level.level_folder, &[a, b])
            .await;
        level
            .save_entity_chunks(vec![(a, entity_chunk(a, 38))])
            .await
            .unwrap();
        level.shutdown().await.unwrap();
        assert_eq!(saved_entity_marker(&level, a).await, 38);
    }

    #[tokio::test]
    async fn entity_flush_includes_loaded_and_preceding_unload_snapshots() {
        let directory = TempDir::new().unwrap();
        let level = Level::from_root_folder(
            &LevelConfig::default(),
            directory.path().to_path_buf(),
            0,
            Dimension::OVERWORLD,
        );
        let a = Vector2::new(0, 0);
        let chunk = entity_chunk(a, 41);
        level.mark_chunks_as_newly_watched(&[a]).await;
        level.loaded_entity_chunks.insert(a, chunk.clone());
        level.save_and_flush().await.unwrap();
        assert_eq!(saved_entity_marker(&level, a).await, 41);

        chunk.data.lock().unwrap()[0].put_int("FerraloomProbe", 42);
        chunk.mark_dirty(true);
        assert!(level.mark_chunk_as_not_watched(a).await);
        level.clean_entity_chunks([a]);
        level.save_and_flush().await.unwrap();
        assert_eq!(saved_entity_marker(&level, a).await, 42);
        level.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn immediate_entity_rewatch_uses_retained_snapshot_before_writer_runs() {
        let directory = TempDir::new().unwrap();
        let level = Level::from_root_folder(
            &LevelConfig::default(),
            directory.path().to_path_buf(),
            0,
            Dimension::OVERWORLD,
        );
        let a = Vector2::new(0, 0);
        let original = entity_chunk(a, 52);
        original.live.store(true, Ordering::Relaxed);
        level.loaded_entity_chunks.insert(a, original);
        // Hold the writer notification pending so this exercises the exact
        // pre-write window independently of scheduler timing.
        level.entity_save_scheduled.store(true, Ordering::Relaxed);
        level.clean_entity_chunks([a]);
        assert!(
            !level
                .level_folder
                .entities_folder
                .join("r.0.0.mca")
                .exists()
        );
        level.mark_chunks_as_newly_watched(&[a]).await;
        let mut received = level.receive_entity_chunks(vec![a]);
        let (restored, first_load) = received.recv().await.unwrap();
        let restored = restored.upgrade().unwrap();
        assert!(
            first_load,
            "Restored NBT must be activated by the core consumer"
        );
        assert!(!restored.live.load(Ordering::Relaxed));
        assert_eq!(
            restored.data.lock().unwrap()[0].get_int("FerraloomProbe"),
            Some(52)
        );

        // Activation consumes its own NBT, leaving the queued immutable copy
        // intact. A subsequent live snapshot must then supersede the old save.
        restored.data.lock().unwrap().clear();
        assert_eq!(
            level
                .pending_entity_saves
                .get(&a)
                .unwrap()
                .data
                .lock()
                .unwrap()[0]
                .get_int("FerraloomProbe"),
            Some(52)
        );
        let mut latest = NbtCompound::new();
        latest.put_string("id", "minecraft:pig".to_string());
        latest.put_int("FerraloomProbe", 53);
        restored.data.lock().unwrap().push(latest);
        restored.live.store(true, Ordering::Relaxed);
        restored.mark_dirty(true);
        level.save_and_flush().await.unwrap();
        assert_eq!(saved_entity_marker(&level, a).await, 53);
        assert!(level.pending_entity_saves.is_empty());
        level.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn entity_region_init_failure_retains_snapshot_after_error_until_retry() {
        let directory = TempDir::new().unwrap();
        let level = Level::from_root_folder(
            &LevelConfig::default(),
            directory.path().to_path_buf(),
            0,
            Dimension::OVERWORLD,
        );
        let a = Vector2::new(0, 0);
        level.loaded_entity_chunks.insert(a, entity_chunk(a, 57));
        level.entity_save_scheduled.store(true, Ordering::Relaxed);
        level.clean_entity_chunks([a]);
        let region = level.level_folder.entities_folder.join("r.0.0.mca");
        std::fs::create_dir(&region).unwrap();
        // Initialization fails before the generic serializer has accepted the
        // chunk. Reporting that failure must not release the only snapshot.
        assert!(level.save_and_flush().await.is_err());
        assert!(level.save_and_flush().await.is_err());
        assert_eq!(level.pending_entity_saves.len(), 1);
        std::fs::remove_dir(&region).unwrap();
        level.save_and_flush().await.unwrap();
        assert_eq!(saved_entity_marker(&level, a).await, 57);
        assert!(level.pending_entity_saves.is_empty());
        level.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn failed_entity_write_survives_neighbor_fetch_and_retry() {
        let directory = TempDir::new().unwrap();
        let level = Level::from_root_folder(
            &LevelConfig::default(),
            directory.path().to_path_buf(),
            0,
            Dimension::OVERWORLD,
        );
        let a = Vector2::new(0, 0);
        let b = Vector2::new(1, 0);
        level
            .entity_saver
            .watch_chunks(&level.level_folder, &[a])
            .await;
        level
            .save_entity_chunks(vec![(a, entity_chunk(a, 43))])
            .await
            .unwrap();
        let region = level.level_folder.entities_folder.join("r.0.0.mca");
        std::fs::create_dir(&region).unwrap();
        assert!(level.entity_saver.flush().await.is_err());
        level
            .entity_saver
            .unwatch_chunks(&level.level_folder, &[a])
            .await;
        let (send, mut receive) = mpsc::channel(1);
        level
            .entity_saver
            .fetch_chunks(&level.level_folder, &[b], send)
            .await;
        assert!(
            matches!(receive.recv().await, Some(LoadedData::Missing(position)) if position == b)
        );
        std::fs::remove_dir(&region).unwrap();
        level.entity_saver.flush().await.unwrap();
        assert_eq!(saved_entity_marker(&level, a).await, 43);
        level.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn explicit_flush_writes_watched_regions_before_returning() {
        let directory = TempDir::new().unwrap();
        let level = Level::from_root_folder(
            &LevelConfig::default(),
            directory.path().to_path_buf(),
            0,
            Dimension::OVERWORLD,
        );
        let position = Vector2::new(0, 0);
        let chunk = ChunkData::empty_sync(0, 0);
        level
            .chunk_saver
            .watch_chunks(&level.level_folder, &[position])
            .await;
        let region = level.level_folder.region_folder.join("r.0.0.mca");
        let mut previous_file = None;

        for block in [&Block::DIAMOND_BLOCK, &Block::EMERALD_BLOCK] {
            chunk.set_block_absolute_y(3, 200, 11, block.default_state.id);
            chunk.mark_dirty(true);
            level
                .chunk_saver
                .save_chunks(&level.level_folder, vec![(position, chunk.clone())])
                .await
                .unwrap();
            // Ordinary saves remain cached while the chunk is watched, even
            // after an earlier explicit flush: flushing must retain watchers.
            assert_eq!(std::fs::read(&region).ok(), previous_file);
            level.save_and_flush().await.unwrap();
            previous_file = Some(std::fs::read(&region).unwrap());

            let reader = ChunkFileManager::<AnvilChunkFile<ChunkData>>::new(
                pumpkin_config::chunk::AnvilChunkConfig::default(),
            );
            let (send, mut receive) = mpsc::channel(1);
            reader
                .fetch_chunks(&level.level_folder, &[position], send)
                .await;
            let Some(LoadedData::Loaded(saved)) = receive.recv().await else {
                panic!("Flushed chunk must be readable through a fresh region cache");
            };
            assert_eq!(
                saved.section.get_block_absolute_y(3, 200, 11),
                Some(block.default_state.id)
            );
        }
        level.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn explicit_flush_reports_region_write_failure() {
        let directory = TempDir::new().unwrap();
        let level = Level::from_root_folder(
            &LevelConfig::default(),
            directory.path().to_path_buf(),
            0,
            Dimension::OVERWORLD,
        );
        let position = Vector2::new(0, 0);
        let chunk = ChunkData::empty_sync(0, 0);
        chunk.mark_dirty(true);
        level
            .chunk_saver
            .watch_chunks(&level.level_folder, &[position])
            .await;
        level
            .chunk_saver
            .save_chunks(&level.level_folder, vec![(position, chunk)])
            .await
            .unwrap();
        // A directory at the region filename reliably rejects writes, including
        // tests run as root where permission-bit failures would be ineffective.
        let region = level.level_folder.region_folder.join("r.0.0.mca");
        std::fs::create_dir(&region).unwrap();
        assert!(level.save_and_flush().await.is_err());
        assert!(level.shutdown().await.is_err());
        std::fs::remove_dir(&region).unwrap();
        level.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_reports_writer_init_failure_without_a_flush_request() {
        use crate::chunk_system::worker_logic::{WriteRequest, io_write_work};
        let directory = TempDir::new().unwrap();
        let level = Level::from_root_folder(
            &LevelConfig::default(),
            directory.path().to_path_buf(),
            0,
            Dimension::OVERWORLD,
        );
        let position = Vector2::new(0, 0);
        let region = level.level_folder.region_folder.join("r.0.0.mca");
        std::fs::create_dir(&region).unwrap();
        let chunk = ChunkData::empty_sync(0, 0);
        chunk.set_block_absolute_y(3, 200, 11, Block::GOLD_BLOCK.default_state.id);
        chunk.mark_dirty(true);
        let (send, receive) = mpsc::channel(1);
        assert!(
            send.send(WriteRequest::Chunks(vec![(
                position,
                crate::chunk_system::Chunk::Level(chunk),
            )]))
            .await
            .is_ok()
        );
        drop(send);
        let io_lock = Arc::new((
            Mutex::new(std::iter::once((position, 1)).collect()),
            tokio::sync::Notify::new(),
        ));
        io_write_work(receive, level.clone(), io_lock).await;
        // The writer exited without seeing a Flush message. Its failure must
        // still reach shutdown, and the only source Arc must remain retained.
        assert!(level.shutdown().await.is_err());
        std::fs::remove_dir(&region).unwrap();
        level.shutdown().await.unwrap();
        let reader = ChunkFileManager::<AnvilChunkFile<ChunkData>>::new(
            pumpkin_config::chunk::AnvilChunkConfig::default(),
        );
        let (send, mut receive) = mpsc::channel(1);
        reader
            .fetch_chunks(&level.level_folder, &[position], send)
            .await;
        let Some(LoadedData::Loaded(saved)) = receive.recv().await else {
            panic!("Retry must preserve the unloaded source chunk");
        };
        assert_eq!(
            saved.section.get_block_absolute_y(3, 200, 11),
            Some(Block::GOLD_BLOCK.default_state.id)
        );
    }

    #[tokio::test]
    async fn split_entity_unwatch_pairs_original_references_after_snapshots() {
        let directory = TempDir::new().unwrap();
        let level = Level::from_root_folder(
            &LevelConfig::default(),
            directory.path().to_path_buf(),
            0,
            Dimension::OVERWORLD,
        );
        let a = Vector2::new(0, 0);
        let b = Vector2::new(1, 0);
        let chunk = entity_chunk(a, 61);
        level.loaded_entity_chunks.insert(a, chunk.clone());
        level.mark_chunks_as_newly_watched(&[a, b]).await;
        level.mark_chunks_as_newly_watched(&[a]).await;
        level
            .save_entity_chunks(vec![(a, chunk.clone())])
            .await
            .unwrap();
        let removed = [a, b];
        let cleaned = level.release_chunk_watchers(&removed);
        assert_eq!(cleaned, vec![b]);
        level.clean_entity_chunks(&cleaned);
        // Pair both original references, not only the now-unwatched position B.
        level.finish_entity_chunk_unwatch(&removed).await;
        level.entity_saver.flush().await.unwrap();
        assert_eq!(saved_entity_marker(&level, a).await, 61);

        chunk.data.lock().unwrap()[0].put_int("FerraloomProbe", 62);
        chunk.mark_dirty(true);
        assert_eq!(level.release_chunk_watchers(&[a]), vec![a]);
        level.clean_entity_chunks([a]);
        level.save_pending_entity_chunks().await.unwrap();
        level.finish_entity_chunk_unwatch(&[a]).await;
        // No explicit cache flush here: the final original watcher reference
        // must have been released, making its pending snapshot reach disk.
        assert_eq!(saved_entity_marker(&level, a).await, 62);
        level.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn entity_region_read_error_does_not_generate_a_replacement_chunk() {
        let directory = TempDir::new().unwrap();
        let level = Level::from_root_folder(
            &LevelConfig::default(),
            directory.path().to_path_buf(),
            0,
            Dimension::OVERWORLD,
        );
        std::fs::create_dir(level.level_folder.entities_folder.join("r.0.0.mca")).unwrap();
        assert!(level.get_entity_chunk(Vector2::new(0, 0)).await.is_err());
        assert!(level.loaded_entity_chunks.is_empty());
        assert!(level.pending_entity_generations.is_empty());
        assert!(level.shutdown().await.is_err());
    }

    #[tokio::test]
    async fn dimension_paths_26_2() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();
        let config = LevelConfig::default();

        let overworld_level =
            Level::from_root_folder(&config, root.clone(), 0, Dimension::OVERWORLD);
        assert_eq!(
            overworld_level.level_folder.dim_folder,
            root.join("dimensions").join("minecraft").join("overworld")
        );
        assert_eq!(
            overworld_level.level_folder.region_folder,
            root.join("dimensions")
                .join("minecraft")
                .join("overworld")
                .join("region")
        );

        let nether_level = Level::from_root_folder(&config, root.clone(), 0, Dimension::THE_NETHER);
        assert_eq!(
            nether_level.level_folder.dim_folder,
            root.join("dimensions").join("minecraft").join("the_nether")
        );

        let end_level = Level::from_root_folder(&config, root.clone(), 0, Dimension::THE_END);
        assert_eq!(
            end_level.level_folder.dim_folder,
            root.join("dimensions").join("minecraft").join("the_end")
        );
    }

    #[tokio::test]
    async fn legacy_dimension_fallback() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();
        let config = LevelConfig::default();

        // Create legacy directories
        std::fs::create_dir_all(root.join("region")).unwrap();
        std::fs::create_dir_all(root.join("DIM-1").join("region")).unwrap();
        std::fs::create_dir_all(root.join("DIM1").join("region")).unwrap();

        let overworld_level =
            Level::from_root_folder(&config, root.clone(), 0, Dimension::OVERWORLD);
        assert_eq!(overworld_level.level_folder.dim_folder, root);

        let nether_level = Level::from_root_folder(&config, root.clone(), 0, Dimension::THE_NETHER);
        assert_eq!(nether_level.level_folder.dim_folder, root.join("DIM-1"));

        let end_level = Level::from_root_folder(&config, root.clone(), 0, Dimension::THE_END);
        assert_eq!(end_level.level_folder.dim_folder, root.join("DIM1"));
    }
}
