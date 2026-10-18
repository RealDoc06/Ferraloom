//! Port of SerializableChunkData's live/saved tick conversion. Storage retains
//! NBT fields; registry resolution and live queue ownership belong to game.
use super::{Level, LevelError, chunk::LevelChunk};
use crate::ticks::{LevelChunkTicks, SavedTick};
use ferraloom_data::{Block, registries::fluid::Id as Fluid};
use ferraloom_util::ChunkPos;
impl LevelChunk {
    /// Load before attaching the chunk to a Level. Saved delays are unpacked
    /// when Level#add_chunk registers the containers at the current game time.
    pub fn load_saved_ticks(
        &mut self,
        data: &ferraloom_storage::SerializableChunkData,
    ) -> Result<(), LevelError> {
        let blocks = data
            .block_ticks
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|tick| {
                let name = tick
                    .id
                    .as_ref()
                    .ok_or(LevelError::InvalidSavedTick)?
                    .to_string_checked()
                    .map_err(|_| LevelError::InvalidSavedTick)?;
                let block = Block::from_name(&name).ok_or(LevelError::InvalidSavedTick)?;
                SavedTick::from_nbt(tick, block).ok_or(LevelError::InvalidSavedTick)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let fluids = data
            .fluid_ticks
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|tick| {
                let name = tick
                    .id
                    .as_ref()
                    .ok_or(LevelError::InvalidSavedTick)?
                    .to_string_checked()
                    .map_err(|_| LevelError::InvalidSavedTick)?;
                let fluid = Fluid::from_name(&name).ok_or(LevelError::InvalidSavedTick)?;
                SavedTick::from_nbt(tick, fluid).ok_or(LevelError::InvalidSavedTick)
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.block_ticks = LevelChunkTicks::from_saved(blocks, self.pos());
        self.fluid_ticks = LevelChunkTicks::from_saved(fluids, self.pos());
        Ok(())
    }
    /// Save an unattached/unloaded chunk's owned containers.
    pub fn save_owned_ticks(
        &self,
        game_time: i64,
        data: &mut ferraloom_storage::SerializableChunkData,
    ) {
        data.block_ticks = Some(
            self.block_ticks
                .pack(game_time)
                .into_iter()
                .map(|tick| tick.to_nbt(tick.block.name()))
                .collect(),
        );
        data.fluid_ticks = Some(
            self.fluid_ticks
                .pack(game_time)
                .into_iter()
                .map(|tick| tick.to_nbt(tick.block.name()))
                .collect(),
        );
    }
}
impl Level {
    /// Capture registered containers with the chunk snapshot, before its IO job.
    pub fn save_chunk_ticks(
        &self,
        pos: ChunkPos,
        data: &mut ferraloom_storage::SerializableChunkData,
    ) -> Result<(), LevelError> {
        let region = self.region();
        let blocks = region
            .block_ticks
            .container(pos)
            .ok_or(LevelError::MissingChunk(pos))?;
        let fluids = region
            .fluid_ticks
            .container(pos)
            .ok_or(LevelError::MissingChunk(pos))?;
        data.block_ticks = Some(
            blocks
                .pack(region.game_time)
                .into_iter()
                .map(|tick| tick.to_nbt(tick.block.name()))
                .collect(),
        );
        data.fluid_ticks = Some(
            fluids
                .pack(region.game_time)
                .into_iter()
                .map(|tick| tick.to_nbt(tick.block.name()))
                .collect(),
        );
        Ok(())
    }
}
