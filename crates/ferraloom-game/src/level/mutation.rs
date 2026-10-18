//! Port of Level#setBlock and LevelChunk#setBlockState's callback sequence.
use super::{Level, LevelError, LevelRules, effects::LevelEffect};
use crate::block::{BlockBehavior, BlockMethod, UpdateFlags};
use ferraloom_data::{Block, BlockState, datapack::TagSet};
use ferraloom_util::{BlockPos, ChunkPos};
use std::sync::Arc;
impl Level {
    pub fn is_in_valid_bounds(&self, pos: BlockPos) -> bool {
        // Port of ChunkPyramid.MAX_CHUNK_COORDINATE_VALUE, checked by the
        // Java fixture. The gameplay 30-million-block border is separate.
        const MAX_CHUNK_COORDINATE: u32 = 2_097_061;
        pos.y >= self.region().min_y
            && pos.y < self.region().max_y
            && (pos.x >> 4).unsigned_abs() <= MAX_CHUNK_COORDINATE
            && (pos.z >> 4).unsigned_abs() <= MAX_CHUNK_COORDINATE
    }
    pub fn rules(&self) -> &LevelRules {
        &self.region().rules
    }
    pub fn rules_mut(&mut self) -> &mut LevelRules {
        &mut self.region_mut().rules
    }
    pub fn set_block_tags(&mut self, tags: Arc<TagSet<Block>>) {
        self.region_mut().tags = Some(tags);
    }
    pub fn set_fluid_tags(&mut self, tags: Arc<TagSet<ferraloom_data::registries::fluid::Id>>) {
        self.region_mut().fluid_tags = Some(tags);
    }
    pub fn fluid_tags(&self) -> Result<&TagSet<ferraloom_data::registries::fluid::Id>, LevelError> {
        self.region()
            .fluid_tags
            .as_deref()
            .ok_or(LevelError::MissingFluidTags)
    }
    pub fn block_tags(&self) -> Result<&ferraloom_data::datapack::TagSet<Block>, LevelError> {
        self.region()
            .tags
            .as_deref()
            .ok_or(LevelError::MissingBlockTags)
    }
    pub(crate) fn custom_block_behavior(&self, block: Block) -> Option<&'static dyn BlockBehavior> {
        self.region().behaviors.block(block)
    }
    pub fn block_behavior(&self, block: Block) -> Result<&'static dyn BlockBehavior, LevelError> {
        self.region()
            .behaviors
            .block(block)
            .or_else(|| crate::block::dispatch::vanilla_behavior(block))
            .ok_or(LevelError::UnsupportedBlock(block))
    }
    pub(crate) fn behavior_for_method(
        &self,
        block: Block,
        method: BlockMethod,
    ) -> Result<&'static dyn BlockBehavior, LevelError> {
        if let Some(custom) = self.custom_block_behavior(block) {
            return Ok(custom);
        }
        if method
            .vanilla_method()
            .is_some_and(|method| !block.overrides(method))
        {
            if matches!(
                method,
                BlockMethod::CanPlaceLiquid | BlockMethod::PlaceLiquid | BlockMethod::PickupBlock
            ) {
                return Ok(&crate::block::fluid::SIMPLE_WATERLOGGED_BLOCK);
            }
            return Ok(&crate::block::BASE_BLOCK);
        }
        crate::block::dispatch::ported_behavior(block, method)
            .ok_or(LevelError::UnsupportedBlock(block))
    }
    pub fn required_block_behavior(
        &self,
        block: Block,
        method: BlockMethod,
    ) -> Result<&'static dyn BlockBehavior, LevelError> {
        self.behavior_for_method(block, method)
            .map_err(|_| LevelError::UnsupportedBlockMethod(block, method))
    }
    pub(crate) fn emit(&mut self, effect: LevelEffect) {
        self.region_mut().effects.push(effect);
    }
    pub fn take_effects(&mut self) -> Vec<LevelEffect> {
        std::mem::take(&mut self.region_mut().effects)
    }
    pub fn set_chunk_block_ticking(
        &mut self,
        pos: ChunkPos,
        ticking: bool,
    ) -> Result<(), LevelError> {
        if self.chunk(pos).is_none() {
            return Err(LevelError::MissingChunk(pos));
        }
        if ticking {
            self.region_mut().block_ticking_chunks.insert(pos);
        } else {
            self.region_mut().block_ticking_chunks.remove(&pos);
        }
        Ok(())
    }
    pub fn should_tick_blocks_at(&self, pos: BlockPos) -> bool {
        self.region()
            .block_ticking_chunks
            .contains(&ChunkPos::containing(pos))
    }
    /// Attach the holder's loading-ticket status and current ticking future.
    /// Neither is implied by simulation tickets or by owning a block chunk.
    pub fn set_chunk_loading_status(
        &mut self,
        pos: ChunkPos,
        requested: super::distance_manager::FullChunkStatus,
        ticking_ready: bool,
    ) -> Result<(), LevelError> {
        let chunk = self
            .region_mut()
            .chunks
            .get_mut(&pos)
            .ok_or(LevelError::MissingChunk(pos))?;
        chunk.requested_full_status = requested;
        chunk.ticking_ready = ticking_ready;
        Ok(())
    }
    /// Return false for vanilla's rejected placements (bounds/debug/identical
    /// state or callback replacement); missing ownership is always an error.
    pub fn set_block(
        &mut self,
        pos: BlockPos,
        state: BlockState,
        flags: UpdateFlags,
        limit: i32,
    ) -> Result<bool, LevelError> {
        if !self.is_in_valid_bounds(pos) || self.rules().debug {
            return Ok(false);
        }
        let chunk_pos = ChunkPos::containing(pos);
        let chunk = self
            .chunk(chunk_pos)
            .ok_or(LevelError::MissingChunk(chunk_pos))?;
        if pos.y < chunk.min_y() || pos.y >= chunk.max_y() {
            return Ok(false);
        }
        if state.is_air() && chunk.section(pos.y >> 4).unwrap().has_only_air() {
            return Ok(false);
        }
        let old = chunk.get_block_state(pos);
        if old == state {
            return Ok(false);
        }
        // Block entity families must provide canonical entities and tickers before
        // they can use this path. Never silently discard unported entity state.
        if old.has_block_entity() || state.has_block_entity() {
            return Err(LevelError::UnsupportedBlockEntity(
                if old.has_block_entity() {
                    old.block()
                } else {
                    state.block()
                },
            ));
        }
        let tags = self
            .region()
            .tags
            .clone()
            .ok_or(LevelError::MissingBlockTags)?;
        let section_y = pos.y >> 4;
        let chunk = self.region_mut().chunks.get_mut(&chunk_pos).unwrap();
        chunk.prime_heightmaps(&tags);
        let was_empty = chunk.section(section_y).unwrap().has_only_air();
        chunk
            .set_block_state(pos, state)
            .expect("validated build height");
        chunk.update_heightmaps(pos, state, &tags);
        let empty = chunk.section(section_y).unwrap().has_only_air();
        if was_empty != empty {
            self.emit(LevelEffect::SectionEmptiness {
                chunk: chunk_pos,
                section_y,
                empty,
            });
        }
        let (old_data, new_data) = (old.data(), state.data());
        if old_data.opacity != new_data.opacity
            || old_data.light_emission != new_data.light_emission
            || old_data.use_shape_for_light_occlusion
            || new_data.use_shape_for_light_occlusion
        {
            self.emit(LevelEffect::CheckLight(pos));
        }
        let moved = flags.contains(UpdateFlags::MOVE_BY_PISTON);
        if (old.block() != state.block()
            || state
                .block()
                .is_instance_of(ferraloom_data::BlockClass::BaseRailBlock))
            && (flags.contains(UpdateFlags::NEIGHBORS) || moved)
        {
            self.optional_block_call(
                old.block(),
                BlockMethod::AffectNeighborsAfterRemoval,
                |behavior, level| behavior.affect_neighbors_after_removal(old, pos, moved, level),
            )?;
        }
        if self.get_block_state(pos)?.block() != state.block() {
            return Ok(false);
        }
        if !flags.contains(UpdateFlags::SKIP_ON_PLACE) {
            self.optional_block_call(state.block(), BlockMethod::OnPlace, |behavior, level| {
                behavior.on_place(state, pos, old, moved, level)
            })?;
        }
        self.region_mut()
            .chunks
            .get_mut(&chunk_pos)
            .unwrap()
            .unsaved = true;
        let new = self.get_block_state(pos)?;
        if new == state {
            if flags.contains(UpdateFlags::CLIENTS)
                && self.chunk(chunk_pos).unwrap().requested_full_status
                    >= super::distance_manager::FullChunkStatus::BlockTicking
            {
                self.send_block_updated(pos, old, state);
            }
            if flags.contains(UpdateFlags::NEIGHBORS) {
                self.update_neighbors_at(pos, old.block())?;
                if state.has_analog_output_signal() {
                    self.update_neighbour_for_output_signal(pos, state.block())?;
                }
            }
            if !flags.contains(UpdateFlags::KNOWN_SHAPE) && limit > 0 {
                let neighbor_flags =
                    flags.without(UpdateFlags::NEIGHBORS | UpdateFlags::SUPPRESS_DROPS);
                self.optional_block_call(
                    old.block(),
                    BlockMethod::UpdateIndirectNeighborShapes,
                    |behavior, level| {
                        behavior.update_indirect_neighbor_shapes(
                            old,
                            pos,
                            neighbor_flags,
                            limit - 1,
                            level,
                        )
                    },
                )?;
                self.update_neighbor_shapes(state, pos, neighbor_flags, limit - 1)?;
                self.optional_block_call(
                    state.block(),
                    BlockMethod::UpdateIndirectNeighborShapes,
                    |behavior, level| {
                        behavior.update_indirect_neighbor_shapes(
                            state,
                            pos,
                            neighbor_flags,
                            limit - 1,
                            level,
                        )
                    },
                )?;
            }
            self.emit(LevelEffect::PoiChanged { pos, old, new });
        }
        Ok(true)
    }
    /// Port of ServerLevel#updateNeighboursOnBlockSet, used by commands after
    /// their deferred-update placement pass. The source is the new block.
    pub fn update_neighbors_on_block_set(
        &mut self,
        pos: BlockPos,
        old: BlockState,
    ) -> Result<(), LevelError> {
        let state = self.get_block_state(pos)?;
        if old.block() != state.block() {
            self.optional_block_call(
                old.block(),
                BlockMethod::AffectNeighborsAfterRemoval,
                |behavior, level| behavior.affect_neighbors_after_removal(old, pos, false, level),
            )?;
        }
        self.update_neighbors_at(pos, state.block())?;
        if state.has_analog_output_signal() {
            self.update_neighbour_for_output_signal(pos, state.block())?;
        }
        Ok(())
    }
    pub fn update_from_neighbor_shapes(
        &mut self,
        state: BlockState,
        pos: BlockPos,
    ) -> Result<BlockState, LevelError> {
        crate::block::LevelAccessRef::Server(self).update_from_neighbor_shapes(state, pos)
    }
    pub fn destroy_block(
        &mut self,
        pos: BlockPos,
        drop: bool,
        limit: i32,
    ) -> Result<bool, LevelError> {
        let state = self.get_block_state(pos)?;
        if state.is_air() {
            return Ok(false);
        }
        self.emit(LevelEffect::DestroyParticles { pos, state });
        if drop {
            self.emit(LevelEffect::DropResources { pos, state });
        }
        let fluid = state.data().fluid;
        let replacement = crate::block::fluid::legacy_block(fluid);
        let destroyed = self.set_block(pos, replacement, UpdateFlags::ALL, limit)?;
        if destroyed {
            self.emit(LevelEffect::BlockDestroyed { pos, state });
        }
        Ok(destroyed)
    }
    pub fn remove_block(&mut self, pos: BlockPos, moved: bool) -> Result<bool, LevelError> {
        let state = crate::block::fluid::legacy_block(self.get_block_state(pos)?.data().fluid);
        self.set_block(
            pos,
            state,
            UpdateFlags::ALL
                | if moved {
                    UpdateFlags::MOVE_BY_PISTON
                } else {
                    UpdateFlags::default()
                },
            crate::block::UPDATE_LIMIT,
        )
    }
    pub fn update_or_destroy(
        &mut self,
        old: BlockState,
        new: BlockState,
        pos: BlockPos,
        flags: UpdateFlags,
        limit: i32,
    ) -> Result<(), LevelError> {
        if old != new {
            if new.is_air() {
                self.destroy_block(pos, !flags.contains(UpdateFlags::SUPPRESS_DROPS), limit)?;
            } else {
                self.set_block(pos, new, flags.without(UpdateFlags::SUPPRESS_DROPS), limit)?;
            }
        }
        Ok(())
    }
}
