//! LevelChunk's pending proto data and postProcessGeneration call order.
use super::{Level, LevelError, chunk::LevelChunk};
use crate::block::{BlockMethod, UPDATE_LIMIT, UpdateFlags, fluid::FlowingFluid};
use ferraloom_data::{BlockClass, BlockState};
use ferraloom_nbt::CompoundTag;
use ferraloom_storage::{SerializableChunkData, UpgradeData};
use ferraloom_util::{BlockPos, ChunkPos};

#[derive(Default)]
pub(super) struct PendingGenerationData {
    pub(super) block_entities: Vec<CompoundTag>,
    offsets: Vec<Vec<i16>>,
    upgrade: Option<UpgradeData>,
}
impl LevelChunk {
    /// Preserve pending proto NBT through FULL without pretending that a live
    /// block entity has been instantiated. Call before Level#add_chunk.
    pub fn load_pending_generation_data(
        &mut self,
        data: &SerializableChunkData,
    ) -> Result<(), LevelError> {
        let offsets = data.post_processing.clone().unwrap_or_default();
        if offsets.len() > self.sections().count() {
            return Err(LevelError::InvalidPendingGenerationData);
        }
        self.pending_generation = PendingGenerationData {
            block_entities: data.block_entities.clone().unwrap_or_default(),
            offsets,
            upgrade: data.upgrade_data.clone(),
        };
        Ok(())
    }
    pub fn pending_block_entity_nbt(&self) -> &[CompoundTag] {
        &self.pending_generation.block_entities
    }
    pub fn post_processing_complete(&self) -> bool {
        self.generation_callbacks_drained() && self.pending_generation.block_entities.is_empty()
    }
    /// Generation callbacks can finish under Warn while raw block entities wait
    /// for their live factories. This never reports those entities as activated.
    pub fn generation_callbacks_drained(&self) -> bool {
        self.pending_generation.offsets.iter().all(Vec::is_empty)
            && self.pending_generation.upgrade.is_none()
    }
    pub fn save_pending_generation_data(&self, data: &mut SerializableChunkData) {
        data.block_entities = Some(self.pending_generation.block_entities.clone());
        data.post_processing = Some(self.pending_generation.offsets.clone());
        data.upgrade_data = self.pending_generation.upgrade.clone();
    }
}
impl Level {
    /// LevelChunk#getBlockEntity(CHECK): activate a pending receiver before
    /// returning it. Unsupported factories retain the exact original raw NBT.
    pub fn get_or_promote_block_entity(
        &mut self,
        pos: BlockPos,
    ) -> Result<Option<crate::block::entity::BlockEntityRef>, LevelError> {
        let key = ChunkPos::containing(pos);
        let chunk = self.get_full_chunk(key)?;
        if let Some(state) = chunk.block_entities.get(pos) {
            if !state.data().removed {
                return Ok(Some(state.data().reference()));
            }
            self.discard_removed_block_entity(pos)?;
            return Ok(None);
        }
        let tag = chunk
            .pending_block_entity_nbt()
            .iter()
            .rev()
            .find(|tag| crate::block::entity::get_pos_from_tag(key, tag) == pos)
            .cloned();
        let Some(tag) = tag else {
            return Ok(None);
        };
        let state = self.get_block_state(pos)?;
        let entity = if tag
            .get_string("id")
            .is_some_and(|id| id.to_string_lossy() == "DUMMY")
        {
            if state.has_block_entity() {
                crate::block::entity::new_block_entity(pos, state)?
            } else {
                None
            }
        } else {
            crate::block::entity::load_static(pos, state, &tag, self.context().registry_context()?)?
        };
        if let Some(entity) = entity {
            self.set_block_entity(entity)?;
        }
        self.region_mut()
            .chunks
            .get_mut(&key)
            .unwrap()
            .pending_generation
            .block_entities
            .retain(|tag| crate::block::entity::get_pos_from_tag(key, tag) != pos);
        self.block_entity_ref(pos)
    }
    /// Port of ChunkAccess#markPosForPostProcessing and ProtoChunk's packing.
    /// A callback may append to the list currently being drained.
    pub fn mark_pos_for_post_processing(&mut self, pos: BlockPos) -> Result<(), LevelError> {
        let key = ChunkPos::containing(pos);
        let chunk = self
            .region_mut()
            .chunks
            .get_mut(&key)
            .ok_or(LevelError::MissingChunk(key))?;
        if pos.y < chunk.min_y() || pos.y >= chunk.max_y() {
            return Ok(());
        }
        let section = ((pos.y - chunk.min_y()) >> 4) as usize;
        let section_count = chunk.sections().count();
        chunk
            .pending_generation
            .offsets
            .resize_with(section_count, Vec::new);
        chunk.pending_generation.offsets[section]
            .push(((pos.x & 15) | ((pos.y & 15) << 4) | ((pos.z & 15) << 8)) as i16);
        Ok(())
    }
    /// Port of LevelChunk#postProcessGeneration. Captured state is reused after
    /// the immediate fluid tick; section lists are cleared only after draining.
    /// Strict rejects pending block entities; Warn counts each activation skip
    /// and retains the raw NBT. Upgrade data stays owned on explicit failure.
    pub fn post_process_generation(&mut self, key: ChunkPos) -> Result<(), LevelError> {
        let chunk = self.chunk(key).ok_or(LevelError::MissingChunk(key))?;
        let min = chunk.min_section();
        let count = chunk.sections().count();
        for section in 0..count {
            let mut cursor = 0;
            let mut skipped = Vec::new();
            loop {
                let offset = self
                    .chunk(key)
                    .unwrap()
                    .pending_generation
                    .offsets
                    .get(section)
                    .and_then(|list| list.get(cursor))
                    .copied();
                let Some(offset) = offset else {
                    break;
                };
                cursor += 1;
                let pos = BlockPos::new(
                    key.x.wrapping_mul(16).wrapping_add(i32::from(offset) & 15),
                    (min + section as i32)
                        .wrapping_mul(16)
                        .wrapping_add((i32::from(offset) >> 4) & 15),
                    key.z
                        .wrapping_mul(16)
                        .wrapping_add((i32::from(offset) >> 8) & 15),
                );
                let state = self.get_block_state(pos)?;
                if let Err(error) = self.post_process_position(pos, state) {
                    self.skip_post_processing(error)?;
                    skipped.push(offset);
                }
            }
            if let Some(list) = self
                .region_mut()
                .chunks
                .get_mut(&key)
                .unwrap()
                .pending_generation
                .offsets
                .get_mut(section)
            {
                *list = skipped;
            }
        }
        let mut pending = ferraloom_util::collections::JavaHashMap::<BlockPos, ()>::new();
        for tag in self.chunk(key).unwrap().pending_block_entity_nbt() {
            pending.insert(crate::block::entity::get_pos_from_tag(key, tag), ());
        }
        let positions: Vec<_> = pending.keys().copied().collect();
        for pos in positions {
            if let Err(error) = self.get_or_promote_block_entity(pos) {
                match error {
                    LevelError::UnsupportedBlockEntity(block) => {
                        let factory = self
                            .chunk(key)
                            .unwrap()
                            .pending_block_entity_nbt()
                            .iter()
                            .rev()
                            .find(|tag| crate::block::entity::get_pos_from_tag(key, tag) == pos)
                            .and_then(|tag| tag.get_string("id"))
                            .cloned()
                            .unwrap_or_default();
                        self.skip_block_entity_activation(block, factory)?;
                    }
                    _ => return Err(error),
                }
            }
        }
        if self
            .chunk(key)
            .unwrap()
            .pending_generation
            .upgrade
            .is_some()
        {
            return Err(LevelError::UnsupportedChunkUpgrade);
        }
        Ok(())
    }
    fn post_process_position(
        &mut self,
        pos: BlockPos,
        state: BlockState,
    ) -> Result<(), LevelError> {
        let fluid = state.data().fluid;
        if !fluid.empty {
            FlowingFluid::from_id(fluid.fluid_type)
                .ok_or(LevelError::UnsupportedBlockMethod(
                    state.block(),
                    BlockMethod::FluidTick,
                ))?
                .tick(self, pos, state, fluid)?;
        }
        if state.block().is_instance_of(BlockClass::LiquidBlock) {
            self.behavior_for_method(state.block(), BlockMethod::Tick)?
                .tick(state, pos, self)?;
        } else {
            let next = self.update_from_neighbor_shapes(state, pos)?;
            if next != state {
                self.set_block(pos, next, UpdateFlags(276), UPDATE_LIMIT)?;
            }
        }
        Ok(())
    }
}
