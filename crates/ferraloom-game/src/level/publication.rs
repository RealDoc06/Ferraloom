//! Main-owner publication boundary for an owned, prepared FULL chunk.
use super::{
    Level, LevelError,
    acquisition::{ChunkKind, ChunkReference},
    chunk::LevelChunk,
    weather::ChunkLightSnapshot,
};
use ferraloom_nbt::CompoundTag;
use std::fmt;

/// Produced by the pipeline after native LIGHT and source-order metadata import.
/// Nonempty entity post-load remains required until entity shared publication is
/// available; rejecting it returns this entire owned payload unchanged.
pub struct PreparedFullChunk {
    pub chunk: LevelChunk,
    pub light: ChunkLightSnapshot,
    pub worldgen_entities: Vec<CompoundTag>,
}

/// Every returned error currently precedes canonical insertion and owns the
/// original input. Entity Publishing-stage failures require a continuation,
/// rather than retrying this payload; that path is not enabled here yet.
pub struct ChunkPublicationError {
    pub source: LevelError,
    pub prepared: PreparedFullChunk,
}
impl fmt::Debug for ChunkPublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChunkPublicationError")
            .field("source", &self.source)
            .field("pos", &self.prepared.chunk.pos())
            .finish()
    }
}

impl Level {
    /// Port of ChunkStatusTasks#full's registration boundary. No mutable borrow
    /// of any previously published chunk, entity or tick container is created.
    pub fn publish_full_chunk(
        &self,
        mut prepared: PreparedFullChunk,
    ) -> Result<ChunkReference, Box<ChunkPublicationError>> {
        if let Err(source) = self.validate_full_publication(&prepared) {
            return Err(Box::new(ChunkPublicationError { source, prepared }));
        }
        let region = self.region();
        let pos = prepared.chunk.pos();
        prepared
            .chunk
            .prime_heightmaps(self.block_tags().expect("publication validated tags"));
        let generation = region
            .next_chunk_generation
            .get()
            .checked_add(1)
            .expect("chunk identities exhausted");
        region.next_chunk_generation.set(generation);
        prepared.chunk.publication_generation = generation;
        region
            .block_entity_state
            .register_published_chunk(&mut prepared.chunk, region.game_time);
        // No callbacks or acquisition suspension occur between precondition
        // checks and these insertions; duplicate registration is an invariant bug.
        assert!(
            region
                .block_ticks
                .publish_container(pos, std::mem::take(&mut prepared.chunk.block_ticks))
                .is_ok()
        );
        assert!(
            region
                .fluid_ticks
                .publish_container(pos, std::mem::take(&mut prepared.chunk.fluid_ticks))
                .is_ok()
        );
        self.publish_light_snapshot(pos, prepared.light);
        assert!(region.chunks.publish(pos, prepared.chunk).is_ok());
        Ok(ChunkReference {
            pos,
            generation,
            kind: ChunkKind::Full,
        })
    }

    fn validate_full_publication(&self, prepared: &PreparedFullChunk) -> Result<(), LevelError> {
        let region = self.region();
        let pos = prepared.chunk.pos();
        if region.chunks.contains_key(&pos)
            || region.block_ticks.container(pos).is_some()
            || region.fluid_ticks.container(pos).is_some()
        {
            return Err(LevelError::DuplicateChunk(pos));
        }
        if prepared.chunk.min_y() != region.min_y || prepared.chunk.max_y() != region.max_y {
            return Err(LevelError::WrongChunkHeight(pos));
        }
        if !prepared.worldgen_entities.is_empty() {
            return Err(LevelError::UnsupportedChunkEntityPublication(pos));
        }
        self.block_tags()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
