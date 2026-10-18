//! LevelChunk's pending proto data and postProcessGeneration call order.
use super::{Level, LevelError, chunk::LevelChunk};
use crate::block::{BlockMethod, UPDATE_LIMIT, UpdateFlags, fluid::FlowingFluid};
use ferraloom_data::{BlockClass, BlockState};
use ferraloom_nbt::CompoundTag;
use ferraloom_storage::{SerializableChunkData, UpgradeData};
use ferraloom_util::{BlockPos, ChunkPos};

#[derive(Default)]
pub(super) struct PendingGenerationData {
    block_entities: Vec<CompoundTag>,
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
        for index in 0..self.chunk(key).unwrap().pending_block_entity_nbt().len() {
            let (pos, factory) = {
                let tag = &self.chunk(key).unwrap().pending_block_entity_nbt()[index];
                let position = || {
                    Some(BlockPos::new(
                        tag.get_int("x")?,
                        tag.get_int("y")?,
                        tag.get_int("z")?,
                    ))
                };
                (
                    position().ok_or(LevelError::InvalidPendingGenerationData)?,
                    tag.get_string("id").cloned().unwrap_or_default(),
                )
            };
            self.skip_block_entity_activation(self.get_block_state(pos)?.block(), factory)?;
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
