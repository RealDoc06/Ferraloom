use std::ops::Range;

use ferraloom_data::{Block, BlockState};
use ferraloom_util::{BlockPos, ChunkPos};

/// Port of LevelChunkSection's block indexing. Dense backing is intentional until
/// the shared ferraloom-chunk palette implementation replaces this private field.
pub struct LevelChunkSection {
    states: Box<[BlockState; 4096]>,
}

impl Default for LevelChunkSection {
    fn default() -> Self {
        Self {
            states: Box::new([Block::AIR.default_state(); 4096]),
        }
    }
}

impl LevelChunkSection {
    fn index(pos: BlockPos) -> usize {
        ((pos.y & 15) << 8 | (pos.z & 15) << 4 | (pos.x & 15)) as usize
    }
    pub fn get_block_state(&self, pos: BlockPos) -> BlockState {
        self.states[Self::index(pos)]
    }
    pub fn set_block_state(&mut self, pos: BlockPos, state: BlockState) -> BlockState {
        std::mem::replace(&mut self.states[Self::index(pos)], state)
    }
}

/// Port of LevelChunk's section ownership. Empty sections allocate on first write.
pub struct LevelChunk {
    pos: ChunkPos,
    min_section: i32,
    sections: Vec<Option<LevelChunkSection>>,
}

impl LevelChunk {
    pub fn new(pos: ChunkPos, sections: Range<i32>) -> Self {
        Self {
            pos,
            min_section: sections.start,
            sections: sections.map(|_| None).collect(),
        }
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
    pub fn get_block_state(&self, pos: BlockPos) -> BlockState {
        let Some(index) = self.section_index(pos) else {
            return Block::VOID_AIR.default_state();
        };
        self.sections[index]
            .as_ref()
            .map_or(Block::AIR.default_state(), |section| {
                section.get_block_state(pos)
            })
    }
    pub fn set_block_state(&mut self, pos: BlockPos, state: BlockState) -> Option<BlockState> {
        let index = self.section_index(pos)?;
        Some(
            self.sections[index]
                .get_or_insert_default()
                .set_block_state(pos, state),
        )
    }
}
