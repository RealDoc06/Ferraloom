//! ProtoChunk's synchronous light-source update and its borrowed engine sink.
use super::RegionChunk;
use crate::generator::GeneratedChunk;
use ferraloom_chunk::LevelChunkSection;
use ferraloom_light::{ChunkSkyLightSources, LevelLightEngine, LightWorld};
use ferraloom_util::{BlockPos, ChunkPos, SectionPos};

/// Port of the LevelLightEngine callbacks used by ProtoChunk#setBlockState.
/// The generation executor lends its existing engine; these calls enqueue work
/// there and never create a second engine or a region-local task queue.
pub trait ProtoChunkLightEngine {
    fn update_section_status(&mut self, pos: SectionPos, empty: bool);
    fn check_block(&mut self, pos: BlockPos);
}
impl ProtoChunkLightEngine for LevelLightEngine {
    fn update_section_status(&mut self, pos: SectionPos, empty: bool) {
        LevelLightEngine::update_section_status(self, pos, empty);
    }
    fn check_block(&mut self, pos: BlockPos) {
        LevelLightEngine::check_block(self, pos);
    }
}

impl RegionChunk {
    /// Port of ChunkAccess#initializeLightSources, before INITIALIZE_LIGHT.
    pub fn initialize_light_sources(&mut self) {
        self.sky_light_sources.fill_from(
            &ChunkBlocks(&self.chunk),
            ChunkPos::new(self.chunk.position[0], self.chunk.position[1]),
        );
    }
    /// Port of ChunkAccess#getSkyLightSources. Raw bulk writes intentionally do
    /// not update this buffer, while ProtoChunk writes do so before checkBlock.
    pub fn sky_light_sources(&self) -> &ChunkSkyLightSources {
        &self.sky_light_sources
    }
    pub(super) fn update_sky_light_sources(&mut self, pos: BlockPos) {
        self.sky_light_sources
            .update(&ChunkBlocks(&self.chunk), pos);
    }
}

/// Source-height updates read only the owning chunk's column. Section storage
/// is borrowed after the write; no heightmap or neighboring chunk is consulted.
struct ChunkBlocks<'a>(&'a GeneratedChunk);
impl LightWorld for ChunkBlocks<'_> {
    fn min_section(&self) -> i32 {
        self.0.min_y >> 4
    }
    fn max_section(&self) -> i32 {
        (self.0.min_y + self.0.height) >> 4
    }
    fn has_chunk(&self, pos: ChunkPos) -> bool {
        self.0.position == [pos.x, pos.z]
    }
    fn section(&self, pos: SectionPos) -> Option<&LevelChunkSection> {
        if !self.has_chunk(pos.chunk()) {
            return None;
        }
        self.0
            .sections
            .get(usize::try_from(pos.y - self.min_section()).ok()?)
    }
}
