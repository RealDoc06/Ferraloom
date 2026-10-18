use std::collections::HashMap;

use ferraloom_data::{Block, BlockState, EntityType};
use ferraloom_util::{BlockPos, ChunkPos};

use crate::{
    Behaviors,
    entity::{Entity, EntityDataView, EntityRef, EntityState},
    ticks::{LevelTicks, TickPriority},
};

pub mod chunk;
pub mod chunk_holder;
pub mod chunk_map;
pub mod distance_manager;
pub mod entity;
pub mod server_level;
pub mod tickets;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LevelError {
    MissingChunk(ChunkPos),
    OutsideBuildHeight,
    MissingEntity(EntityRef),
    WrongEntityType(EntityRef),
    DuplicateEntity(EntityRef),
    DuplicateChunk(ChunkPos),
    UnsupportedEntity(EntityType),
    UnsupportedBlock(Block),
}

/// Owned portion of ServerLevel. The scheduler moves this value between workers.
pub struct Region {
    chunks: HashMap<ChunkPos, chunk::LevelChunk>,
    entities: entity::EntityStorage,
    entity_tick_list: entity::EntityTickList,
    block_ticks: LevelTicks,
    behaviors: &'static Behaviors,
}

impl Region {
    pub fn new(behaviors: &'static Behaviors) -> Self {
        Self {
            chunks: HashMap::new(),
            entities: entity::EntityStorage::default(),
            entity_tick_list: entity::EntityTickList::default(),
            block_ticks: LevelTicks::default(),
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
    fn region(&self) -> &Region {
        self.region.as_ref().expect("level is parked")
    }
    #[inline]
    fn region_mut(&mut self) -> &mut Region {
        self.region.as_mut().expect("level is parked")
    }

    pub fn chunk(&self, pos: ChunkPos) -> Option<&chunk::LevelChunk> {
        self.region().chunks.get(&pos)
    }
    pub fn remove_chunk(&mut self, pos: ChunkPos) -> Option<chunk::LevelChunk> {
        self.region_mut().chunks.remove(&pos)
    }
    pub fn add_chunk(&mut self, chunk: chunk::LevelChunk) -> Result<(), LevelError> {
        let pos = chunk.pos();
        let region = self.region_mut();
        if region.chunks.contains_key(&pos) {
            return Err(LevelError::DuplicateChunk(pos));
        }
        region.chunks.insert(pos, chunk);
        Ok(())
    }
    /// Port of ServerLevel's blockTicks phase: collect first, then invoke callbacks.
    pub fn tick_scheduled_blocks(
        &mut self,
        game_time: i64,
        budget: usize,
    ) -> Result<(), LevelError> {
        let collected = self.region_mut().block_ticks.collect_due(game_time, budget);
        for tick in collected {
            let state = self.get_block_state(tick.pos)?;
            if state.block() == tick.block {
                let behavior = self
                    .region()
                    .behaviors
                    .block(tick.block)
                    .ok_or(LevelError::UnsupportedBlock(tick.block))?;
                behavior.tick(state, tick.pos, self)?;
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
            .ok_or(LevelError::UnsupportedEntity(kind))
    }
    #[inline]
    pub fn get_block_state(&self, pos: BlockPos) -> Result<BlockState, LevelError> {
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
        region
            .block_ticks
            .schedule(pos, block, trigger_tick, priority);
        Ok(())
    }
}
