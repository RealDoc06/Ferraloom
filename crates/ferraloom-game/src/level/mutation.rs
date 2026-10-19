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
    /// Port of the GameRules.BLOCK_DROPS query used by Block's drop helpers.
    /// An unpublished rule is a required-query error under either policy.
    pub fn block_drops_enabled(&self) -> Result<bool, LevelError> {
        self.rules()
            .block_drops
            .ok_or(LevelError::UnsupportedGameRule("minecraft:block_drops"))
    }
    /// Install LevelChunk's four live heightmaps when a full chunk enters the
    /// simulation. Raw section writes remain a storage-only operation.
    pub fn prime_chunk_heightmaps(&mut self, pos: ChunkPos) -> Result<(), LevelError> {
        let tags = self
            .region()
            .tags
            .clone()
            .ok_or(LevelError::MissingBlockTags)?;
        self.region_mut()
            .chunks
            .get_mut(&pos)
            .ok_or(LevelError::MissingChunk(pos))?
            .prime_heightmaps(&tags);
        Ok(())
    }
    pub fn set_block_tags(&mut self, tags: Arc<TagSet<Block>>) {
        self.region_mut().tags = Some(tags);
    }
    pub fn set_fluid_tags(&mut self, tags: Arc<TagSet<ferraloom_data::registries::fluid::Id>>) {
        self.region_mut().fluid_tags = Some(tags);
    }
    pub fn set_entity_type_tags(&mut self, tags: Arc<TagSet<ferraloom_data::EntityType>>) {
        self.region_mut().entity_type_tags = Some(tags);
    }
    pub fn set_item_tags(&mut self, tags: Arc<TagSet<ferraloom_data::Item>>) {
        self.region_mut().item_tags = Some(tags);
    }
    pub fn item_tags(&self) -> Result<&TagSet<ferraloom_data::Item>, LevelError> {
        self.region()
            .item_tags
            .as_deref()
            .ok_or(LevelError::MissingItemTags)
    }
    pub fn entity_type_tags(&self) -> Result<&TagSet<ferraloom_data::EntityType>, LevelError> {
        self.region()
            .entity_type_tags
            .as_deref()
            .ok_or(LevelError::MissingEntityTypeTags)
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
        crate::block::dispatch::registered_behavior(block, method)
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
    /// Attach the holder's loading-ticket status and current ticking future.
    /// Neither is implied by simulation tickets or by owning a block chunk.
    pub fn set_chunk_loading_status(
        &self,
        pos: ChunkPos,
        requested: super::distance_manager::FullChunkStatus,
        ticking_ready: bool,
    ) -> Result<(), LevelError> {
        let chunk = self.chunk(pos).ok_or(LevelError::MissingChunk(pos))?;
        chunk.set_loading_status(requested, ticking_ready);
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
        let chunk = self.get_full_chunk(chunk_pos)?;
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
        let block_changed = old.block() != state.block();
        if block_changed
            && old.has_block_entity()
            && !crate::block::entity::registry::should_changed_state_keep_block_entity(
                state, old, &tags,
            )
        {
            if !flags.contains(UpdateFlags::SKIP_BLOCK_ENTITY_SIDEEFFECTS)
                && let Some(entity) = self.get_or_promote_block_entity(pos)?
            {
                let behavior = self.block_entity_state(entity)?.behavior();
                self.with_block_entity_callback(entity, |level| {
                    behavior.pre_remove_side_effects(entity, level)
                })?;
            }
            self.remove_block_entity(pos)?;
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
        if state.has_block_entity() && self.get_block_state(pos)?.block() == state.block() {
            self.ensure_block_entity(pos, state)?;
        }
        self.region_mut()
            .chunks
            .get_mut(&chunk_pos)
            .unwrap()
            .mark_unsaved();
        let new = self.get_block_state(pos)?;
        if new == state {
            if flags.contains(UpdateFlags::CLIENTS)
                && self.chunk(chunk_pos).unwrap().full_status()
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
        self.destroy_block_with_breaker(pos, drop, None, limit)
    }
    /// Port of Level#destroyBlock, including synchronous drops before the live
    /// fluid replacement is read. Particle dispatch uses no breaker exclusion.
    pub fn destroy_block_with_breaker(
        &mut self,
        pos: BlockPos,
        drop: bool,
        breaker: Option<crate::entity::EntityRef>,
        limit: i32,
    ) -> Result<bool, LevelError> {
        let state = self.get_block_state(pos)?;
        if state.is_air() {
            return Ok(false);
        }
        self.optional_block_call(
            state.block(),
            BlockMethod::SpawnDestroyParticles,
            |behavior, level| behavior.spawn_destroy_particles(state, pos, level),
        )?;
        if drop {
            let block_entity = if state.has_block_entity() {
                self.get_or_promote_block_entity(pos)?
            } else {
                None
            };
            crate::block::drops::drop_resources_with_tool(
                state,
                pos,
                block_entity,
                breaker,
                &ferraloom_protocol::component::ItemStack::default(),
                self,
            )?;
        }
        let fluid = self.get_block_state(pos)?.data().fluid;
        let replacement = crate::block::fluid::legacy_block(fluid);
        let destroyed = self.set_block(pos, replacement, UpdateFlags::ALL, limit)?;
        if destroyed {
            self.game_event(
                ferraloom_data::registries::game_event::Id::BLOCK_DESTROY,
                ferraloom_util::Vec3::new(
                    f64::from(pos.x) + 0.5,
                    f64::from(pos.y) + 0.5,
                    f64::from(pos.z) + 0.5,
                ),
                breaker,
                Some(state),
            )?;
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
