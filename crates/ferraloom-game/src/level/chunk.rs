use std::ops::Range;

pub use ferraloom_chunk::LevelChunkSection;
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{BlockPos, ChunkPos};

/// Port of LevelChunk's section ownership. Absent sections use the supplied
/// empty-section template, including the dimension's default biome registry.
pub struct LevelChunk {
    pos: ChunkPos,
    min_section: i32,
    sections: Vec<Option<LevelChunkSection>>,
    empty_section: LevelChunkSection,
}

impl LevelChunk {
    pub fn new(pos: ChunkPos, sections: Range<i32>, empty_section: LevelChunkSection) -> Self {
        assert!(empty_section.has_only_air());
        Self {
            pos,
            min_section: sections.start,
            sections: sections.map(|_| None).collect(),
            empty_section,
        }
    }
    pub fn from_sections(
        pos: ChunkPos,
        min_section: i32,
        sections: Vec<LevelChunkSection>,
    ) -> Self {
        assert!(!sections.is_empty());
        let empty_section = sections[0].clone();
        Self {
            pos,
            min_section,
            sections: sections.into_iter().map(Some).collect(),
            empty_section,
        }
    }
    pub fn sections(&self) -> impl Iterator<Item = &LevelChunkSection> {
        self.sections
            .iter()
            .map(|s| s.as_ref().unwrap_or(&self.empty_section))
    }
    pub fn min_section(&self) -> i32 {
        self.min_section
    }
    pub fn pos(&self) -> ChunkPos {
        self.pos
    }
    fn section_index(&self, pos: BlockPos) -> Option<usize> {
        if ChunkPos::containing(pos) != self.pos {
            return None;
        }
        usize::try_from((pos.y >> 4) - self.min_section)
            .ok()
            .filter(|&index| index < self.sections.len())
    }
    #[inline]
    pub fn get_block_state(&self, pos: BlockPos) -> BlockState {
        let Some(index) = self.section_index(pos) else {
            return Block::VOID_AIR.default_state();
        };
        self.sections[index]
            .as_ref()
            .unwrap_or(&self.empty_section)
            .get_block_state((pos.x & 15) as u8, (pos.y & 15) as u8, (pos.z & 15) as u8)
    }
    #[inline]
    pub fn set_block_state(&mut self, pos: BlockPos, state: BlockState) -> Option<BlockState> {
        let index = self.section_index(pos)?;
        let section = self.sections[index].get_or_insert_with(|| self.empty_section.clone());
        Some(section.set_block_state(
            (pos.x & 15) as u8,
            (pos.y & 15) as u8,
            (pos.z & 15) as u8,
            state,
        ))
    }
}
