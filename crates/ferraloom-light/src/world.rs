use crate::ChunkSkyLightSources;
use ferraloom_chunk::LevelChunkSection;
use ferraloom_data::{Block, BlockState};
use ferraloom_util::core::{BlockPos, ChunkPos, SectionPos};
use std::borrow::Cow;
use std::collections::HashMap;

/// Read-only input to a light job. Implementations must keep all reads stable
/// for the job's duration. A missing chunk is opaque, never implicitly air.
/// Loaded chunks may omit all-air sections. Section bounds are half-open.
/// Generic calls keep block reads monomorphized on the propagation hot path.
pub trait LightWorld {
    fn min_section(&self) -> i32;
    fn max_section(&self) -> i32;
    fn has_chunk(&self, pos: ChunkPos) -> bool;
    fn section(&self, pos: SectionPos) -> Option<&LevelChunkSection>;
    /// Port of LightChunk#getSkyLightSources. A supplied canonical snapshot
    /// wins over section-derived caches, including after raw bulk writes.
    fn sky_light_sources(&self, _pos: ChunkPos) -> Option<&ChunkSkyLightSources> {
        None
    }

    fn block_state(&self, pos: BlockPos) -> BlockState {
        let section = SectionPos::from_block(pos);
        if !self.has_chunk(section.chunk()) {
            return Block::BEDROCK.default_state();
        }
        if section.y < self.min_section() || section.y >= self.max_section() {
            return Block::AIR.default_state();
        }
        self.section(section)
            .map_or(Block::AIR.default_state(), |s| {
                s.get_block_state((pos.x & 15) as u8, (pos.y & 15) as u8, (pos.z & 15) as u8)
            })
    }
}

/// A light-pool input assembled from owned sections or borrowed immutable
/// sections. No live-state writer or scheduler is retained by the engine.
#[derive(Debug)]
pub struct SectionSnapshots<'a> {
    min_section: i32,
    max_section: i32,
    chunks: HashMap<i64, Vec<Option<Cow<'a, LevelChunkSection>>>>,
    sky_sources: HashMap<i64, ChunkSkyLightSources>,
}
impl<'a> SectionSnapshots<'a> {
    pub fn new(min_section: i32, max_section: i32) -> Self {
        assert!(min_section < max_section);
        Self {
            min_section,
            max_section,
            chunks: HashMap::new(),
            sky_sources: HashMap::new(),
        }
    }
    pub fn insert_chunk(
        &mut self,
        pos: ChunkPos,
        sections: Vec<Option<Cow<'a, LevelChunkSection>>>,
    ) {
        self.insert_chunk_with_sky_light_sources(pos, sections, None);
    }
    /// Capture native per-chunk source heights with the section snapshot. The
    /// optional buffer is never reconstructed from raw bulk-mutated sections.
    pub fn insert_chunk_with_sky_light_sources(
        &mut self,
        pos: ChunkPos,
        sections: Vec<Option<Cow<'a, LevelChunkSection>>>,
        sources: Option<ChunkSkyLightSources>,
    ) {
        assert_eq!(
            sections.len(),
            (self.max_section - self.min_section) as usize
        );
        self.chunks.insert(pos.pack(), sections);
        if let Some(sources) = sources {
            self.sky_sources.insert(pos.pack(), sources);
        } else {
            self.sky_sources.remove(&pos.pack());
        }
    }
    pub fn remove_chunk(&mut self, pos: ChunkPos) {
        self.chunks.remove(&pos.pack());
        self.sky_sources.remove(&pos.pack());
    }
    /// Change a snapshot before submitting it, copying a borrowed section only
    /// when necessary. Live game state remains outside this adapter.
    pub fn section_mut(&mut self, pos: SectionPos) -> Option<&mut LevelChunkSection> {
        let index = usize::try_from(pos.y - self.min_section).ok()?;
        self.chunks
            .get_mut(&pos.chunk().pack())?
            .get_mut(index)?
            .as_mut()
            .map(Cow::to_mut)
    }
}
impl LightWorld for SectionSnapshots<'_> {
    fn min_section(&self) -> i32 {
        self.min_section
    }
    fn max_section(&self) -> i32 {
        self.max_section
    }
    fn has_chunk(&self, pos: ChunkPos) -> bool {
        self.chunks.contains_key(&pos.pack())
    }
    fn section(&self, pos: SectionPos) -> Option<&LevelChunkSection> {
        let index = usize::try_from(pos.y - self.min_section).ok()?;
        self.chunks.get(&pos.chunk().pack())?.get(index)?.as_deref()
    }
    fn sky_light_sources(&self, pos: ChunkPos) -> Option<&ChunkSkyLightSources> {
        self.sky_sources.get(&pos.pack())
    }
}
