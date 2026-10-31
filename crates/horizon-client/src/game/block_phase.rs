//! The serial shell borrows its actual live storage for the shared kernel.
use super::GameLevel;
use ferraloom_game::{block_phase::BlockPhaseLevel, random_tick_sections::Sections};
use horizon_client::chunk::Chunk;
use horizon_core::position::ChunkPos;
use std::sync::Arc;

impl BlockPhaseLevel for GameLevel<'_> {
    type Chunk = Chunk;

    fn block_revision(&self) -> u64 {
        self.world.revision
    }
    fn resident_chunk(&self, chunk: ChunkPos) -> Option<Arc<Chunk>> {
        self.world.chunks.get(&(chunk.x, chunk.z)).cloned()
    }
    fn random_tick_sections(&mut self, chunk: ChunkPos) -> Sections {
        self.world
            .chunks
            .get(&(chunk.x, chunk.z))
            .map_or_else(Sections::default, |column| {
                self.random_tick_sections
                    .get((chunk.x, chunk.z), column, self.facts)
            })
    }
}
