use std::{
    cell::{Cell, RefCell},
    sync::Arc,
};

use ferraloom_data::{Block, BlockState, EntityType};
use ferraloom_util::{BlockPos, ChunkPos};

use crate::{
    Behaviors,
    entity::{Entity, EntityDataView, EntityRef, EntityState},
    ticks::{LevelTicks, ScheduledTick, TickPriority},
};

pub mod acquisition;
mod block_actions;
mod block_entity;
pub mod blockscan;
pub mod border;
mod broadcast;
pub mod chunk;
pub mod chunk_holder;
pub mod chunk_map;
pub mod chunk_task_dispatcher;
pub mod clip;
mod container_runtime;
mod context;
pub use context::LevelContext;
pub mod distance_manager;
pub mod effects;
pub mod emission;
pub mod entity;
pub mod environment;
pub mod events;
mod features;
pub mod game_event;
pub mod generation;
pub mod heightmap;
pub mod interaction;
mod lighting;
pub mod loot_facts;
mod mutation;
pub mod path_navigation_region;
mod post_processing;
pub mod publication;
mod random;
pub mod redstone;
mod saved_ticks;
pub mod server_level;
pub mod simulation;
mod simulation_access;
pub(crate) mod store;
pub mod structure_manager;
mod structure_services;
pub use structure_services::StructureServices;
pub mod tick_rate;
pub mod tickets;
mod ticking;
pub mod weather;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LevelError {
    BlockBreak(crate::entity::player::game_mode::BlockBreakFailure),
    /// Native ArrayList iterator ConcurrentModificationException during listener visitation.
    ConcurrentGameEventListenerModification,
    UnsupportedItemUseOn(ferraloom_data::Item),
    UnsupportedItemCraftedPostProcess(ferraloom_data::Item),
    UnsupportedItemInventoryTick(ferraloom_data::Item),
    UnsupportedItemHurtAndBreak {
        item: ferraloom_data::Item,
        amount: i32,
        slot: ferraloom_protocol::component::EquipmentSlot,
    },
    UnsupportedPortalUse(Block),
    UnsupportedEntityMethod(EntityType, crate::entity::EntityMethod),
    UnsupportedAdvancementCriterion(crate::advancements::criteria::Trigger),
    Inventory(&'static str),
    MissingBlockEntity(crate::block::entity::BlockEntityRef),
    WrongBlockEntityType(crate::block::entity::BlockEntityRef),
    DuplicateBlockEntity(crate::block::entity::BlockEntityRef),
    MissingChunk(ChunkPos),
    ChunkAcquisition(
        ChunkPos,
        chunk_holder::ChunkStatus,
        acquisition::AcquireFailure,
    ),
    UnsupportedChunkEntityPublication(ChunkPos),
    WorldgenMutationFailed {
        pos: BlockPos,
        source: ferraloom_worldgen::Error,
    },
    UnsupportedWorldgenEnvironment,
    UnsupportedWorldgenBehavior,
    OutsideBuildHeight,
    MissingPlacementPlayer,
    UninitializedPlacementDirection,
    MissingEntity(EntityRef),
    WrongEntityType(EntityRef),
    DuplicateEntity(EntityRef),
    DuplicateChunk(ChunkPos),
    UnsupportedEntity(EntityType),
    UnsupportedBlock(Block),
    UnsupportedBlockMethod(Block, crate::block::BlockMethod),
    UnsupportedBlockEntity(Block),
    UnsupportedGameRule(&'static str),
    UnsupportedLightQuery,
    UnsupportedEnvironmentAttribute(ferraloom_data::registries::environment_attribute::Id),
    MissingEnvironmentContext,
    MissingSpawnProtectionContext,
    MissingServerShared,
    MissingBlockTags,
    MissingBlockTag,
    MissingDamageTypeTag,
    MissingFluidTags,
    MissingEntityTypeTags,
    MissingItemTags,
    UnsupportedShape(BlockState),
    UnsupportedFluidRandomTick,
    UnsupportedWeatherQuery,
    UnsupportedPushEntitiesUp,
    UnsupportedPackReload,
    InvalidSavedTick,
    InvalidPendingGenerationData,
    UnsupportedChunkUpgrade,
    WrongChunkHeight(ChunkPos),
    InvalidChunkStructures(ChunkPos),
    InvalidStructureHolderSet,
    PathNavigation(path_navigation_region::PathNavigationError),
    UnsupportedPathNavigationQuery(path_navigation_region::PathNavigationQuery),
}

impl std::fmt::Display for LevelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BlockBreak(error) => std::fmt::Display::fmt(error.error(), formatter),
            Self::WorldgenMutationFailed { pos, source } => {
                write!(formatter, "worldgen mutation at {pos:?}: {source}")
            }
            error => std::fmt::Debug::fmt(error, formatter),
        }
    }
}
impl std::error::Error for LevelError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::BlockBreak(error) => Some(error.error()),
            Self::WorldgenMutationFailed { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Dimension settings consumed by the live core. The server supplies the
/// dimension's FAST_LAVA attribute and synchronized game-rule snapshot.
#[derive(Clone, Copy, Debug)]
pub struct LevelRules {
    /// Published by the canonical GameRules owner after load and each change.
    pub block_drops: Option<bool>,
    pub show_advancement_messages: bool,
    pub tnt_explodes: bool,
    pub entity_drops: bool,
    pub max_entity_cramming: i32,
    pub players_nether_portal_default_delay: i32,
    pub drowning_damage: bool,
    pub fall_damage: bool,
    pub fire_damage: bool,
    pub freeze_damage: bool,
    pub send_command_feedback: bool,
    pub natural_health_regeneration: bool,
    pub debug: bool,
    pub can_have_weather: bool,
    pub random_tick_speed: u32,
    pub max_snow_accumulation_height: i32,
    pub water_source_conversion: bool,
    pub lava_source_conversion: bool,
    pub fast_lava: bool,
    pub redstone_experiments: bool,
    pub spread_fire: bool,
    pub spread_vines: bool,
    pub mob_griefing: bool,
}
impl Default for LevelRules {
    fn default() -> Self {
        use crate::gamerules::{GameRule, RuleType};
        let damage_default = |rule: GameRule| {
            let RuleType::Boolean { default } = rule.definition().kind else {
                unreachable!("damage game rules are boolean");
            };
            default
        };
        Self {
            block_drops: None,
            show_advancement_messages: true,
            tnt_explodes: true,
            entity_drops: true,
            max_entity_cramming: 24,
            players_nether_portal_default_delay: match GameRule::PlayersNetherPortalDefaultDelay
                .definition()
                .kind
            {
                RuleType::Integer { default, .. } => default,
                _ => unreachable!("portal delay is an integer game rule"),
            },
            drowning_damage: damage_default(GameRule::DrowningDamage),
            fall_damage: damage_default(GameRule::FallDamage),
            fire_damage: damage_default(GameRule::FireDamage),
            freeze_damage: damage_default(GameRule::FreezeDamage),
            send_command_feedback: damage_default(GameRule::SendCommandFeedback),
            natural_health_regeneration: damage_default(GameRule::NaturalHealthRegeneration),
            debug: false,
            can_have_weather: true,
            random_tick_speed: 3,
            max_snow_accumulation_height: 1,
            water_source_conversion: true,
            lava_source_conversion: false,
            fast_lava: false,
            redstone_experiments: false,
            spread_fire: true,
            spread_vines: true,
            mob_griefing: true,
        }
    }
}

/// Identity of the owner of canonical chunk/entity stores. Reads compare only
/// the copied integer; the monotonic counter runs once during construction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RegionId(u64);
impl RegionId {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(
            NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .expect("region identities exhausted"),
        )
    }
}

/// Owned portion of ServerLevel. The scheduler moves this value between workers.
pub struct Region {
    id: RegionId,
    /// Port of ServerLevel#pathTypesByPosCache. One region per level currently
    /// matches vanilla exactly; a future region split must preserve per-level semantics.
    path_types_by_pos_cache: crate::entity::ai::pathfinder::PathTypeCache,
    chunks: store::InsertOnlyMap<ChunkPos, chunk::LevelChunk>,
    next_chunk_generation: Cell<u64>,
    entities: entity::EntityStorage,
    entity_tick_list: entity::EntityTickList,
    block_ticks: LevelTicks,
    fluid_ticks: LevelTicks<ferraloom_data::registries::fluid::Id>,
    block_entity_state: block_entity::BlockEntityRuntime,
    game_event_listeners: game_event::GameEventListenerRegistries,
    container_runtime: crate::block::containers::runtime::ContainerRuntime,
    next_sub_tick_order: i64,
    game_time: i64,
    empty_time: i32,
    handling_tick: bool,
    weather: weather::Weather,
    min_y: i32,
    max_y: i32,
    rules: LevelRules,
    spawn_protection: Option<interaction::SpawnProtectionSnapshot>,
    has_skylight: Option<bool>,
    environment: Option<environment::EnvironmentRuntime>,
    random: ferraloom_util::random::Random,
    sound_seed_generator: ferraloom_util::random::Random,
    rand_value: i32,
    simulation: RefCell<simulation::SimulationChunkTracker>,
    neighbor_updater: redstone::CollectingNeighborUpdater,
    // The family owner replaces its pending state/registration stub together.
    #[allow(dead_code)]
    pub(crate) redstone: crate::block::redstone::RedstoneState,
    block_events: events::BlockEvents,
    effects: Vec<effects::LevelEffect>,
    block_changes: broadcast::BlockChanges,
    tags: Option<Arc<ferraloom_data::datapack::TagSet<Block>>>,
    fluid_tags:
        Option<Arc<ferraloom_data::datapack::TagSet<ferraloom_data::registries::fluid::Id>>>,
    entity_type_tags: Option<Arc<ferraloom_data::datapack::TagSet<EntityType>>>,
    item_tags: Option<Arc<ferraloom_data::datapack::TagSet<ferraloom_data::Item>>>,
    pub(crate) unported: crate::block::UnportedCalls,
    behaviors: &'static Behaviors,
}

impl Region {
    pub fn new(behaviors: &'static Behaviors) -> Self {
        Self {
            id: RegionId::new(),
            chunks: store::InsertOnlyMap::default(),
            next_chunk_generation: Cell::new(0),
            entities: entity::EntityStorage::default(),
            entity_tick_list: entity::EntityTickList::default(),
            block_ticks: LevelTicks::default(),
            fluid_ticks: LevelTicks::default(),
            block_entity_state: block_entity::BlockEntityRuntime::default(),
            game_event_listeners: Default::default(),
            container_runtime: crate::block::containers::runtime::ContainerRuntime::default(),
            next_sub_tick_order: 0,
            game_time: 0,
            empty_time: 0,
            handling_tick: false,
            weather: weather::Weather::default(),
            min_y: -64,
            max_y: 320,
            rules: LevelRules::default(),
            spawn_protection: None,
            has_skylight: None,
            environment: None,
            random: random::new_level_random(),
            sound_seed_generator: random::new_sound_random(),
            rand_value: random::new_position_seed(),
            simulation: RefCell::new(simulation::SimulationChunkTracker::default()),
            neighbor_updater: redstone::CollectingNeighborUpdater::default(),
            redstone: crate::block::redstone::RedstoneState::default(),
            block_events: events::BlockEvents::default(),
            effects: Vec::new(),
            block_changes: broadcast::BlockChanges::default(),
            path_types_by_pos_cache: Default::default(),
            tags: None,
            fluid_tags: None,
            entity_type_tags: None,
            item_tags: None,
            unported: crate::block::UnportedCalls::default(),
            behaviors,
        }
    }
}

/// Concrete region-scoped Level/ServerLevel access, with inlinable world methods.
///
/// Canonical state belongs to Region, not to callback frames. The optional owner
/// lets the scheduler temporarily park ownership without moving this call facade.
/// A parked facade must be reattached before resuming any game method.
pub struct Level {
    command_storage: Option<Arc<crate::commands::storage::CommandStorageSnapshot>>,
    shared: Option<Box<crate::ServerShared>>,
    region: Option<Region>,
    context: Arc<LevelContext>,
    difficulty: ferraloom_protocol::play::Difficulty,
    acquisition: acquisition::ChunkAcquisition,
    acquire_driver: Option<Box<dyn acquisition::AcquireDriver>>,
    fluid_shapes: clip::FluidShapeCache,
}

impl Level {
    pub(crate) fn path_type_cache(&self) -> &crate::entity::ai::pathfinder::PathTypeCache {
        &self.region().path_types_by_pos_cache
    }
    pub(crate) fn path_type_cache_mut(
        &mut self,
    ) -> &mut crate::entity::ai::pathfinder::PathTypeCache {
        &mut self.region_mut().path_types_by_pos_cache
    }
    pub fn new(region: Region, context: Arc<LevelContext>) -> Self {
        Self {
            command_storage: None,
            shared: None,
            region: Some(region),
            difficulty: context.initial_difficulty,
            context,
            acquisition: Default::default(),
            acquire_driver: None,
            fluid_shapes: Default::default(),
        }
    }
    /// Required server-owned storage view, absent until startup installs it.
    pub fn command_storage(&self) -> Option<&crate::commands::storage::CommandStorageSnapshot> {
        self.command_storage.as_deref()
    }
    pub fn set_command_storage(
        &mut self,
        snapshot: Arc<crate::commands::storage::CommandStorageSnapshot>,
    ) {
        self.command_storage = Some(snapshot);
    }
    /// The server installs this canonical owner before entering a gameplay phase.
    pub fn shared(&self) -> Result<&crate::ServerShared, LevelError> {
        self.shared
            .as_deref()
            .ok_or(LevelError::MissingServerShared)
    }
    pub fn shared_mut(&mut self) -> Result<&mut crate::ServerShared, LevelError> {
        self.shared
            .as_deref_mut()
            .ok_or(LevelError::MissingServerShared)
    }
    /// Phase-boundary lending only. Replacing an active owner is a scheduler error.
    pub fn install_shared(&mut self, shared: Box<crate::ServerShared>) {
        assert!(
            self.shared.is_none(),
            "server shared owner already installed"
        );
        self.shared = Some(shared);
    }
    /// Return the same owner after the phase and all nested callbacks finish.
    pub fn take_shared(&mut self) -> Result<Box<crate::ServerShared>, LevelError> {
        self.shared.take().ok_or(LevelError::MissingServerShared)
    }
    pub fn context(&self) -> &Arc<LevelContext> {
        &self.context
    }
    /// Port of Level#getDifficulty, published from canonical world data.
    pub fn difficulty(&self) -> ferraloom_protocol::play::Difficulty {
        self.difficulty
    }
    pub fn set_difficulty(&mut self, difficulty: ferraloom_protocol::play::Difficulty) {
        self.difficulty = difficulty;
    }
    pub fn seed(&self) -> i64 {
        self.context.seed
    }
    pub fn sea_level(&self) -> i32 {
        self.context.sea_level
    }
    pub fn into_region(self) -> Region {
        self.region.expect("level is parked")
    }
    #[inline]
    pub(crate) fn region(&self) -> &Region {
        self.region.as_ref().expect("level is parked")
    }
    #[inline]
    pub(crate) fn region_mut(&mut self) -> &mut Region {
        self.region.as_mut().expect("level is parked")
    }

    pub fn chunk(&self, pos: ChunkPos) -> Option<&chunk::LevelChunk> {
        self.region().chunks.get(&pos)
    }
    pub fn remove_chunk(&mut self, pos: ChunkPos) -> Option<chunk::LevelChunk> {
        self.acquisition.clear_cache();
        let region = self.region_mut();
        // An unrelated competing registration cannot replace the stage's owned
        // saved queue. Keep both records until its failed continuation resolves.
        if region
            .chunks
            .get(&pos)
            .and_then(|chunk| chunk.staged_publication.as_ref())
            .is_some_and(|stage| {
                (stage.block_ticks.borrow().is_some()
                    && region.block_ticks.container(pos).is_some())
                    || (stage.fluid_ticks.borrow().is_some()
                        && region.fluid_ticks.container(pos).is_some())
            })
        {
            return None;
        }
        let mut chunk = region.chunks.remove(&pos)?;
        chunk.block_ticks = region
            .block_ticks
            .remove_container(pos)
            .or_else(|| {
                chunk
                    .staged_publication
                    .as_mut()
                    .and_then(|stage| stage.block_ticks.get_mut().take())
            })
            .unwrap_or_default();
        chunk.fluid_ticks = region
            .fluid_ticks
            .remove_container(pos)
            .or_else(|| {
                chunk
                    .staged_publication
                    .as_mut()
                    .and_then(|stage| stage.fluid_ticks.get_mut().take())
            })
            .unwrap_or_default();
        chunk.loaded.set(false);
        self.remove_light_snapshot(pos);
        Some(chunk)
    }
    pub fn add_chunk(&mut self, mut chunk: chunk::LevelChunk) -> Result<(), LevelError> {
        let pos = chunk.pos();
        let attached_only = self.acquire_driver.is_none();
        let region = self.region_mut();
        if region.chunks.contains_key(&pos) {
            return Err(LevelError::DuplicateChunk(pos));
        }
        if attached_only && region.chunks.is_empty() {
            region.min_y = chunk.min_y();
            region.max_y = chunk.max_y();
        }
        if region.min_y != chunk.min_y() || region.max_y != chunk.max_y() {
            return Err(LevelError::WrongChunkHeight(pos));
        }
        if chunk.staged_publication.as_ref().is_some_and(|stage| {
            stage.block_ticks.borrow().is_some() || stage.fluid_ticks.borrow().is_some()
        }) {
            return Err(LevelError::InvalidPendingGenerationData);
        }
        chunk.publication_generation = region
            .next_chunk_generation
            .get()
            .checked_add(1)
            .expect("chunk identities exhausted");
        region
            .next_chunk_generation
            .set(chunk.publication_generation);
        chunk.staged_publication = None;
        chunk.loaded.set(true);
        region
            .block_entity_state
            .prepare_published_chunk(&mut chunk, region.game_time);
        region
            .block_entity_state
            .register_published_chunk(&chunk, region.has_skylight)?;
        region
            .block_ticks
            .add_container(pos, std::mem::take(&mut chunk.block_ticks));
        region
            .fluid_ticks
            .add_container(pos, std::mem::take(&mut chunk.fluid_ticks));
        region.chunks.insert(pos, chunk);
        Ok(())
    }
    /// Port of ServerLevel#startTickingChunk, after generation post-processing.
    /// FULL registration alone leaves saved delays relative until this point.
    pub fn start_ticking_chunk(&mut self, pos: ChunkPos) -> Result<(), LevelError> {
        let region = self.region_mut();
        let chunk = region
            .chunks
            .get(&pos)
            .ok_or(LevelError::MissingChunk(pos))?;
        if !chunk.full_publication_complete() {
            return Err(LevelError::ChunkAcquisition(
                pos,
                chunk_holder::ChunkStatus::Full,
                acquisition::AcquireFailure::ReentrantFull,
            ));
        }
        region.block_ticks.unpack_container(pos, region.game_time);
        region.fluid_ticks.unpack_container(pos, region.game_time);
        Ok(())
    }
    /// Compatibility entry for the scheduled block phase. It uses the same
    /// native simulation, ticking-future and entity-load gates as the runtime.
    pub fn tick_scheduled_blocks(
        &mut self,
        game_time: i64,
        budget: usize,
    ) -> Result<(), LevelError> {
        self.tick_block_phase(game_time, budget)
    }
    fn run_collected_blocks(&mut self) -> Result<(), LevelError> {
        while let Some(tick) = self.region_mut().block_ticks.poll_this_tick() {
            let state = self.get_block_state(tick.pos)?;
            if state.block() == tick.block {
                self.optional_block_call(
                    tick.block,
                    crate::block::BlockMethod::Tick,
                    |behavior, level| behavior.tick(state, tick.pos, level),
                )?;
            }
        }
        Ok(())
    }
    #[inline]
    pub fn entity<T: EntityDataView>(&self, entity: EntityRef) -> Result<&T, LevelError> {
        T::view(self.entity_state(entity)?).ok_or(LevelError::WrongEntityType(entity))
    }
    #[inline]
    pub fn entity_mut<T: EntityDataView>(
        &mut self,
        entity: EntityRef,
    ) -> Result<&mut T, LevelError> {
        T::view_mut(self.entity_state_mut(entity)?).ok_or(LevelError::WrongEntityType(entity))
    }
    #[inline]
    pub fn entity_state(&self, entity: EntityRef) -> Result<&EntityState, LevelError> {
        self.region()
            .entities
            .get(&entity)
            .ok_or(LevelError::MissingEntity(entity))
    }
    #[inline]
    pub fn entity_state_mut(&mut self, entity: EntityRef) -> Result<&mut EntityState, LevelError> {
        self.region_mut()
            .entities
            .get_mut(&entity)
            .ok_or(LevelError::MissingEntity(entity))
    }
    pub fn entity_behavior(&self, entity: EntityRef) -> Result<&'static dyn Entity, LevelError> {
        let kind = self.entity_state(entity)?.entity_type();
        self.region()
            .behaviors
            .entity(kind)
            .or_else(|| crate::entity::vanilla_behavior(kind))
            .ok_or(LevelError::UnsupportedEntity(kind))
    }
    #[inline]
    pub fn get_block_state(&self, pos: BlockPos) -> Result<BlockState, LevelError> {
        if !self.is_in_valid_bounds(pos) {
            return Ok(Block::VOID_AIR.default_state());
        }
        let chunk_pos = ChunkPos::containing(pos);
        self.get_full_chunk(chunk_pos)
            .map(|chunk| chunk.get_block_state(pos))
    }
    /// Low-level mutation, before Level#setBlock's neighbor/lighting hooks.
    #[inline]
    pub fn set_block_state(
        &mut self,
        pos: BlockPos,
        state: BlockState,
    ) -> Result<BlockState, LevelError> {
        let chunk_pos = ChunkPos::containing(pos);
        self.get_full_chunk(chunk_pos)?;
        self.region_mut()
            .chunks
            .get_mut(&chunk_pos)
            .ok_or(LevelError::MissingChunk(chunk_pos))?
            .set_block_state(pos, state)
            .ok_or(LevelError::OutsideBuildHeight)
    }
    pub fn schedule_block_tick(
        &mut self,
        pos: BlockPos,
        block: Block,
        trigger_tick: i64,
        priority: TickPriority,
    ) -> Result<(), LevelError> {
        let chunk = ChunkPos::containing(pos);
        let region = self.region_mut();
        if !region.chunks.contains_key(&chunk) {
            return Err(LevelError::MissingChunk(chunk));
        }
        let sub_tick_order = region.next_sub_tick_order;
        region.next_sub_tick_order = sub_tick_order.wrapping_add(1);
        region.block_ticks.schedule_tick(ScheduledTick {
            pos,
            block,
            trigger_tick,
            priority,
            sub_tick_order,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) mod drop_fixtures;

#[cfg(test)]
mod destroy_tests;
