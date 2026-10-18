//! LevelChunk-owned canonical block entities, generation handles and short borrows.
use super::{Level, LevelError};
use crate::{
    block::entity::{
        BlockEntityDataView, BlockEntityRef, BlockEntityState, ticking::BlockEntityTickers,
    },
    entity::{EntityDataView, EntityRef, player::ServerPlayer},
};
use ferraloom_util::{BlockPos, ChunkPos, collections::Object2ObjectOpenHashMap};
use std::collections::HashMap;

/// Positions retain fastutil traversal; slot storage permits safe disjoint borrows
/// without taking a receiver out of the canonical chunk during a callback.
#[derive(Default)]
pub(super) struct ChunkBlockEntities {
    positions: Object2ObjectOpenHashMap<BlockPos, usize>,
    slots: Vec<Option<BlockEntityState>>,
    free: Vec<usize>,
}
impl ChunkBlockEntities {
    pub(super) fn get(&self, pos: BlockPos) -> Option<&BlockEntityState> {
        self.slots[*self.positions.get(&pos)?].as_ref()
    }
    fn get_mut(&mut self, pos: BlockPos) -> Option<&mut BlockEntityState> {
        self.slots[*self.positions.get(&pos)?].as_mut()
    }
    fn insert(&mut self, state: BlockEntityState) {
        let pos = state.data().pos;
        assert!(!self.positions.contains_key(&pos));
        let index = self.free.pop().unwrap_or_else(|| {
            self.slots.push(None);
            self.slots.len() - 1
        });
        self.slots[index] = Some(state);
        self.positions.insert(pos, index);
    }
    fn remove(&mut self, pos: BlockPos) -> Option<BlockEntityState> {
        let index = self.positions.remove(&pos)?;
        self.free.push(index);
        self.slots[index].take()
    }
    pub(super) fn iter(&self) -> impl Iterator<Item = &BlockEntityState> {
        self.positions
            .values()
            .map(|&index| self.slots[index].as_ref().unwrap())
    }
    fn select_mut<'a>(
        &'a mut self,
        refs: &[BlockEntityRef],
    ) -> Vec<(usize, &'a mut BlockEntityState)> {
        let mut indices: Vec<_> = refs
            .iter()
            .enumerate()
            .filter_map(|(order, handle)| {
                self.positions.get(&handle.pos).map(|&index| (index, order))
            })
            .collect();
        indices.sort_unstable();
        let mut rest = self.slots.as_mut_slice();
        let mut start = 0;
        let mut result = Vec::with_capacity(indices.len());
        for (index, order) in indices {
            let (_, tail) = rest.split_at_mut(index - start);
            let (slot, tail) = tail.split_first_mut().unwrap();
            result.push((order, slot.as_mut().unwrap()));
            rest = tail;
            start = index + 1;
        }
        result
    }
}
#[derive(Default)]
pub(super) struct BlockEntityRuntime {
    next_generation: u64,
    detached: HashMap<BlockEntityRef, BlockEntityState>,
    roots: HashMap<BlockEntityRef, usize>,
    pub(super) tickers: BlockEntityTickers,
}
impl Level {
    /// Current positional receiver. Unactivated raw NBT is a required unsupported
    /// query in both policies, never an absent block entity.
    pub fn block_entity_ref(&self, pos: BlockPos) -> Result<Option<BlockEntityRef>, LevelError> {
        let key = ChunkPos::containing(pos);
        let chunk = self.chunk(key).ok_or(LevelError::MissingChunk(key))?;
        if let Some(state) = chunk.block_entities.get(pos) {
            return Ok(Some(state.data().reference()));
        }
        if chunk
            .pending_block_entity_nbt()
            .iter()
            .any(|tag| crate::block::entity::get_pos_from_tag(key, tag) == pos)
        {
            return Err(LevelError::UnsupportedBlockEntity(
                self.get_block_state(pos)?.block(),
            ));
        }
        Ok(None)
    }
    pub fn block_entity_state(
        &self,
        handle: BlockEntityRef,
    ) -> Result<&BlockEntityState, LevelError> {
        if let Some(state) = self
            .chunk(ChunkPos::containing(handle.pos))
            .and_then(|c| c.block_entities.get(handle.pos))
            && state.data().generation == handle.generation
        {
            return Ok(state);
        }
        self.region()
            .block_entity_state
            .detached
            .get(&handle)
            .ok_or(LevelError::MissingBlockEntity(handle))
    }
    pub fn block_entity_state_mut(
        &mut self,
        handle: BlockEntityRef,
    ) -> Result<&mut BlockEntityState, LevelError> {
        let region = self.region_mut();
        if let Some(state) = region
            .chunks
            .get_mut(&ChunkPos::containing(handle.pos))
            .and_then(|c| c.block_entities.get_mut(handle.pos))
            && state.data().generation == handle.generation
        {
            return Ok(state);
        }
        region
            .block_entity_state
            .detached
            .get_mut(&handle)
            .ok_or(LevelError::MissingBlockEntity(handle))
    }
    pub fn block_entity<T: BlockEntityDataView>(
        &self,
        handle: BlockEntityRef,
    ) -> Result<&T, LevelError> {
        T::view(self.block_entity_state(handle)?).ok_or(LevelError::WrongBlockEntityType(handle))
    }
    pub fn block_entity_mut<T: BlockEntityDataView>(
        &mut self,
        handle: BlockEntityRef,
    ) -> Result<&mut T, LevelError> {
        T::view_mut(self.block_entity_state_mut(handle)?)
            .ok_or(LevelError::WrongBlockEntityType(handle))
    }
    /// Port of LevelChunk#setBlockEntity. A new receiver always gets fresh identity;
    /// replacing it detaches the old receiver without invalidating active frames.
    pub fn set_block_entity(&mut self, mut state: BlockEntityState) -> Result<(), LevelError> {
        let pos = state.data().pos;
        let key = ChunkPos::containing(pos);
        let block = self.get_block_state(pos)?;
        if !state.data().kind.is_valid(block) {
            return Err(LevelError::UnsupportedBlockEntity(block.block()));
        }
        let region = self.region_mut();
        if let Some(mut old) = region
            .chunks
            .get_mut(&key)
            .ok_or(LevelError::MissingChunk(key))?
            .block_entities
            .remove(pos)
        {
            old.data_mut().set_removed();
            region
                .block_entity_state
                .detached
                .insert(old.data().reference(), old);
        }
        let runtime = &mut region.block_entity_state;
        runtime.next_generation = runtime
            .next_generation
            .checked_add(1)
            .expect("block entity generations exhausted");
        let data = state.data_mut();
        data.generation = runtime.next_generation;
        data.block_state = block;
        data.removed = false;
        data.has_level = true;
        let reference = data.reference();
        let kind = data.kind;
        if state.behavior().has_server_ticker() {
            runtime.tickers.bind(reference, kind);
        } else {
            runtime.tickers.unbind(pos);
        }
        region
            .chunks
            .get_mut(&key)
            .ok_or(LevelError::MissingChunk(key))?
            .block_entities
            .insert(state);
        Ok(())
    }
    /// Port of LevelChunk#removeBlockEntity. It does not call preRemoveSideEffects;
    /// setBlockState invokes that virtual before this operation at its source slot.
    pub fn remove_block_entity(&mut self, pos: BlockPos) -> Result<(), LevelError> {
        let key = ChunkPos::containing(pos);
        let region = self.region_mut();
        let chunk = region
            .chunks
            .get_mut(&key)
            .ok_or(LevelError::MissingChunk(key))?;
        if let Some(mut state) = chunk.block_entities.remove(pos) {
            state.data_mut().set_removed();
            region
                .block_entity_state
                .detached
                .insert(state.data().reference(), state);
        }
        region.block_entity_state.tickers.unbind(pos);
        Ok(())
    }
    /// BlockEntity#setChanged marks the current containing chunk, even when the
    /// receiver is an old detached generation still executing a callback.
    pub fn set_changed(&mut self, pos: BlockPos) -> Result<(), LevelError> {
        self.mark_chunk_unsaved(ChunkPos::containing(pos))
    }
    pub fn retain_block_entity(&mut self, handle: BlockEntityRef) -> Result<(), LevelError> {
        self.block_entity_state(handle)?;
        *self
            .region_mut()
            .block_entity_state
            .roots
            .entry(handle)
            .or_default() += 1;
        Ok(())
    }
    pub fn release_block_entity(&mut self, handle: BlockEntityRef) {
        let roots = &mut self.region_mut().block_entity_state.roots;
        let count = roots
            .get_mut(&handle)
            .expect("unbalanced block entity root release");
        *count -= 1;
        if *count == 0 {
            roots.remove(&handle);
        }
    }
    /// Pin the Java receiver across reentrant world calls, including replacement.
    pub fn with_block_entity_callback<R>(
        &mut self,
        handle: BlockEntityRef,
        callback: impl FnOnce(&mut Self) -> Result<R, LevelError>,
    ) -> Result<R, LevelError> {
        self.retain_block_entity(handle)?;
        let result = callback(self);
        self.release_block_entity(handle);
        result
    }
    /// Serialize active receivers in vanilla's fastutil map order, retaining raw
    /// unactivated NBT exactly for factories which are still unported.
    pub fn block_entity_tags_for_saving(
        &self,
        key: ChunkPos,
    ) -> Result<Vec<ferraloom_nbt::CompoundTag>, LevelError> {
        let chunk = self.chunk(key).ok_or(LevelError::MissingChunk(key))?;
        let mut tags = Vec::new();
        for state in chunk.block_entities.iter() {
            tags.push(
                state
                    .save_with_full_metadata()
                    .map_err(|_| LevelError::Inventory("block entity serialization codec"))?,
            );
        }
        tags.extend(
            chunk
                .pending_block_entity_nbt()
                .iter()
                .filter(|tag| {
                    let pos = crate::block::entity::get_pos_from_tag(key, tag);
                    chunk.block_entities.get(pos).is_none()
                })
                .cloned(),
        );
        Ok(tags)
    }
    /// Phase-boundary retirement. Menus and other persistent holders retain roots
    /// explicitly; callback roots are released only after their frame unwinds.
    pub fn collect_detached_block_entities(&mut self) {
        let runtime = &mut self.region_mut().block_entity_state;
        runtime
            .detached
            .retain(|handle, _| runtime.roots.contains_key(handle));
    }
    /// Disjoint pure menu borrow. Stale generations and duplicate receivers fail
    /// before any borrow; returned block entities follow the caller's refs order.
    pub fn player_and_block_entities_mut(
        &mut self,
        player: EntityRef,
        refs: &[BlockEntityRef],
    ) -> Result<(&mut ServerPlayer, Vec<&mut BlockEntityState>), LevelError> {
        for (index, &handle) in refs.iter().enumerate() {
            if refs[..index].contains(&handle) {
                return Err(LevelError::DuplicateBlockEntity(handle));
            }
            if self.block_entity_ref(handle.pos)? != Some(handle) {
                return Err(LevelError::MissingBlockEntity(handle));
            }
        }
        let region = self.region_mut();
        let player = ServerPlayer::view_mut(
            region
                .entities
                .get_mut(&player)
                .ok_or(LevelError::MissingEntity(player))?,
        )
        .ok_or(LevelError::WrongEntityType(player))?;
        let mut selected = Vec::with_capacity(refs.len());
        for (key, chunk) in &mut region.chunks {
            if refs
                .iter()
                .any(|handle| ChunkPos::containing(handle.pos) == *key)
            {
                selected.extend(chunk.block_entities.select_mut(refs));
            }
        }
        selected.sort_unstable_by_key(|(index, _)| *index);
        Ok((
            player,
            selected.into_iter().map(|(_, state)| state).collect(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        block::entity::{BlockEntityData, create},
        level::{Region, chunk::LevelChunk},
    };
    use ferraloom_data::Block;
    use std::sync::Arc;
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    fn level() -> Level {
        let mut level = Level::new(Region::new(&BEHAVIORS));
        for x in 0..=1 {
            level
                .add_chunk(LevelChunk::new(
                    ChunkPos::new(x, 0),
                    0..1,
                    ferraloom_chunk::LevelChunkSection::new(
                        ferraloom_chunk::Strategy::block_states(),
                        ferraloom_chunk::Strategy::biomes(Arc::new(
                            ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
                        )),
                        ferraloom_chunk::BiomeId(0),
                    ),
                ))
                .unwrap();
        }
        level
    }
    fn portal(level: &mut Level, pos: BlockPos) -> BlockEntityRef {
        let state = Block::END_PORTAL.default_state();
        level.set_block_state(pos, state).unwrap();
        level.set_block_entity(create(pos, state).unwrap()).unwrap();
        level.block_entity_ref(pos).unwrap().unwrap()
    }
    #[test]
    fn reentrant_replacement_preserves_old_receiver_and_pinned_menu_roots() {
        let mut level = level();
        let pos = BlockPos::new(1, 1, 1);
        let old = portal(&mut level, pos);
        level.retain_block_entity(old).unwrap();
        let fresh = level
            .with_block_entity_callback(old, |level| {
                let fresh = portal(level, pos);
                assert_ne!(old, fresh);
                level.block_entity_mut::<BlockEntityData>(old)?.has_level = false;
                level.collect_detached_block_entities();
                assert!(level.block_entity::<BlockEntityData>(old)?.removed);
                assert!(level.block_entity::<BlockEntityData>(fresh)?.has_level);
                Ok(fresh)
            })
            .unwrap();
        level.collect_detached_block_entities();
        assert!(level.block_entity::<BlockEntityData>(old).is_ok());
        level.release_block_entity(old);
        level.collect_detached_block_entities();
        assert_eq!(
            level.block_entity::<BlockEntityData>(old).unwrap_err(),
            LevelError::MissingBlockEntity(old)
        );
        assert_eq!(level.block_entity_ref(pos), Ok(Some(fresh)));
    }
    #[test]
    fn menu_borrow_preserves_order_and_rejects_duplicate_or_stale_generations() {
        use crate::entity::{EntityState, player::GameType};
        use ferraloom_util::{Uuid, Vec3};
        let mut level = level();
        let a = portal(&mut level, BlockPos::new(1, 1, 1));
        let b = portal(&mut level, BlockPos::new(17, 1, 1));
        let player = level.next_entity_ref();
        level
            .add_entity(
                player,
                EntityState::Player(Box::new(ServerPlayer::new(
                    Uuid::NIL,
                    "test".into(),
                    1,
                    Vec3::ZERO,
                    GameType::Survival,
                ))),
            )
            .unwrap();
        let (actor, mut states) = level
            .player_and_block_entities_mut(player, &[b, a])
            .unwrap();
        actor.name = "changed".into();
        assert_eq!(states[0].data().reference(), b);
        assert_eq!(states[1].data().reference(), a);
        states[0].data_mut().has_level = false;
        assert!(!level.block_entity::<BlockEntityData>(b).unwrap().has_level);
        assert!(
            matches!(level.player_and_block_entities_mut(player,&[a,a]),Err(LevelError::DuplicateBlockEntity(h)) if h==a)
        );
        portal(&mut level, a.pos);
        assert!(
            matches!(level.player_and_block_entities_mut(player,&[a]),Err(LevelError::MissingBlockEntity(h)) if h==a)
        );
    }
}
