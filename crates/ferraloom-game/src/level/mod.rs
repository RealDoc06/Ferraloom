use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use ferraloom_data::{Block, BlockState, EntityType};
use ferraloom_util::{BlockPos, ChunkPos};

use crate::{
    Behaviors,
    entity::{Entity, EntityDataView, EntityRef, EntityState},
    ticks::{LevelTicks, ScheduledTick, TickPriority},
};

pub mod border;
mod broadcast;
pub mod chunk;
pub mod chunk_holder;
pub mod chunk_map;
pub mod distance_manager;
pub mod effects;
pub mod entity;
pub mod events;
pub mod heightmap;
mod mutation;
mod random;
pub mod redstone;
mod saved_ticks;
pub mod server_level;
pub mod simulation;
pub mod tick_rate;
pub mod tickets;
mod ticking;
pub mod weather;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LevelError {
    Inventory(&'static str),
    MissingChunk(ChunkPos),
    OutsideBuildHeight,
    MissingEntity(EntityRef),
    WrongEntityType(EntityRef),
    DuplicateEntity(EntityRef),
    DuplicateChunk(ChunkPos),
    UnsupportedEntity(EntityType),
    UnsupportedBlock(Block),
    UnsupportedBlockMethod(Block, crate::block::BlockMethod),
    UnsupportedBlockEntity(Block),
    MissingBlockTags,
    MissingFluidTags,
    UnsupportedShape(BlockState),
    UnsupportedFluidRandomTick,
    UnsupportedWeatherQuery,
    UnsupportedPackReload,
    InvalidSavedTick,
    WrongChunkHeight(ChunkPos),
}

/// Dimension settings consumed by the live core. The server supplies the
/// dimension's FAST_LAVA attribute and synchronized game-rule snapshot.
#[derive(Clone, Copy, Debug)]
pub struct LevelRules {
    pub tnt_explodes: bool,
    pub entity_drops: bool,
    pub debug: bool,
    pub can_have_weather: bool,
    pub random_tick_speed: u32,
    pub water_source_conversion: bool,
    pub lava_source_conversion: bool,
    pub fast_lava: bool,
    pub redstone_experiments: bool,
    pub spread_fire: bool,
}
impl Default for LevelRules {
    fn default() -> Self {
        Self {
            tnt_explodes: true,
            entity_drops: true,
            debug: false,
            can_have_weather: true,
            random_tick_speed: 3,
            water_source_conversion: true,
            lava_source_conversion: false,
            fast_lava: false,
            redstone_experiments: false,
            spread_fire: true,
        }
    }
}

/// Owned portion of ServerLevel. The scheduler moves this value between workers.
pub struct Region {
    chunks: HashMap<ChunkPos, chunk::LevelChunk>,
    entities: entity::EntityStorage,
    entity_tick_list: entity::EntityTickList,
    block_ticks: LevelTicks,
    fluid_ticks: LevelTicks<ferraloom_data::registries::fluid::Id>,
    next_sub_tick_order: i64,
    game_time: i64,
    weather: weather::Weather,
    min_y: i32,
    max_y: i32,
    rules: LevelRules,
    random: ferraloom_util::random::Random,
    rand_value: i32,
    block_ticking_chunks: HashSet<ChunkPos>,
    neighbor_updater: redstone::CollectingNeighborUpdater,
    block_events: events::BlockEvents,
    effects: Vec<effects::LevelEffect>,
    block_changes: broadcast::BlockChanges,
    tags: Option<Arc<ferraloom_data::datapack::TagSet<Block>>>,
    fluid_tags:
        Option<Arc<ferraloom_data::datapack::TagSet<ferraloom_data::registries::fluid::Id>>>,
    pub(crate) unported: crate::block::UnportedCalls,
    behaviors: &'static Behaviors,
}

impl Region {
    pub fn new(behaviors: &'static Behaviors) -> Self {
        Self {
            chunks: HashMap::new(),
            entities: entity::EntityStorage::default(),
            entity_tick_list: entity::EntityTickList::default(),
            block_ticks: LevelTicks::default(),
            fluid_ticks: LevelTicks::default(),
            next_sub_tick_order: 0,
            game_time: 0,
            weather: weather::Weather::default(),
            min_y: -64,
            max_y: 320,
            rules: LevelRules::default(),
            random: random::new_level_random(),
            rand_value: random::new_position_seed(),
            block_ticking_chunks: HashSet::new(),
            neighbor_updater: redstone::CollectingNeighborUpdater::default(),
            block_events: events::BlockEvents::default(),
            effects: Vec::new(),
            block_changes: broadcast::BlockChanges::default(),
            tags: None,
            fluid_tags: None,
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
    region: Option<Region>,
}

impl Level {
    pub fn new(region: Region) -> Self {
        Self {
            region: Some(region),
        }
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
        let region = self.region_mut();
        let mut chunk = region.chunks.remove(&pos)?;
        chunk.block_ticks = region.block_ticks.remove_container(pos).unwrap_or_default();
        chunk.fluid_ticks = region.fluid_ticks.remove_container(pos).unwrap_or_default();
        region.block_ticking_chunks.remove(&pos);
        Some(chunk)
    }
    pub fn add_chunk(&mut self, mut chunk: chunk::LevelChunk) -> Result<(), LevelError> {
        let pos = chunk.pos();
        let region = self.region_mut();
        if region.chunks.contains_key(&pos) {
            return Err(LevelError::DuplicateChunk(pos));
        }
        if region.chunks.is_empty() {
            region.min_y = chunk.min_y();
            region.max_y = chunk.max_y();
        }
        if region.min_y != chunk.min_y() || region.max_y != chunk.max_y() {
            return Err(LevelError::WrongChunkHeight(pos));
        }
        chunk.block_ticks.unpack(region.game_time);
        chunk.fluid_ticks.unpack(region.game_time);
        region
            .block_ticks
            .add_container(pos, std::mem::take(&mut chunk.block_ticks));
        region
            .fluid_ticks
            .add_container(pos, std::mem::take(&mut chunk.fluid_ticks));
        region.chunks.insert(pos, chunk);
        Ok(())
    }
    /// Port of ServerLevel's blockTicks phase: collect first, then invoke callbacks.
    pub fn tick_scheduled_blocks(
        &mut self,
        game_time: i64,
        budget: usize,
    ) -> Result<(), LevelError> {
        self.region_mut().game_time = game_time;
        let region = self.region_mut();
        region.block_ticks.collect(game_time, budget, |_| true);
        let result = self.run_collected_blocks();
        self.region_mut().block_ticks.cleanup_after_tick();
        result
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
        self.region()
            .chunks
            .get(&chunk_pos)
            .ok_or(LevelError::MissingChunk(chunk_pos))
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
