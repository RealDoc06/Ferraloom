use crate::level::{Level, LevelError};
use ferraloom_chunk::LevelChunkSection;
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{BlockPos, ChunkPos};

#[derive(Clone, Copy)]
struct CacheEntry<'a> {
    section_pos: BlockPos,
    section: Option<&'a LevelChunkSection>,
}

/// Port of FilteredSectionCache. Eight parity-indexed slots cache both accepted
/// and rejected palettes. Out-of-height AIR never acquires a chunk.
pub struct FilteredSectionCache<'a, P> {
    level: &'a Level,
    predicate: P,
    entries: [Option<CacheEntry<'a>>; 8],
}
impl<'a, P: Fn(BlockState) -> bool> FilteredSectionCache<'a, P> {
    pub fn new(level: &'a Level, predicate: P) -> Self {
        Self {
            level,
            predicate,
            entries: [None; 8],
        }
    }
    pub fn get_block_state(&mut self, pos: BlockPos) -> Result<Option<BlockState>, LevelError> {
        let section_pos = BlockPos::new(pos.x >> 4, pos.y >> 4, pos.z >> 4);
        let state = if section_pos.y < (self.level.min_build_height() >> 4)
            || section_pos.y > (self.level.max_build_height() >> 4)
        {
            Block::AIR.default_state()
        } else {
            let hash = (((section_pos.z & 1) << 2)
                + ((section_pos.y & 1) << 1)
                + (section_pos.x & 1)) as usize;
            if self.entries[hash].is_none_or(|entry| entry.section_pos != section_pos) {
                let chunk = self
                    .level
                    .get_full_chunk(ChunkPos::new(section_pos.x, section_pos.z))?;
                let section = chunk
                    .section(section_pos.y)
                    .expect("cached section within dimension bounds");
                self.entries[hash] = Some(CacheEntry {
                    section_pos,
                    section: section
                        .states()
                        .maybe_has(&self.predicate)
                        .then_some(section),
                });
            }
            let Some(section) = self.entries[hash].unwrap().section else {
                return Ok(None);
            };
            section.get_block_state((pos.x & 15) as u8, (pos.y & 15) as u8, (pos.z & 15) as u8)
        };
        Ok((self.predicate)(state).then_some(state))
    }
}
