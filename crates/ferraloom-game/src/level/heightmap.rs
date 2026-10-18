//! Port of Heightmap.Types for the four live LevelChunk heightmaps.
use ferraloom_data::{Block, BlockState, datapack::TagSet};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum Heightmap {
    MotionBlocking,
    MotionBlockingNoLeaves,
    OceanFloor,
    WorldSurface,
}
impl Heightmap {
    pub const ALL: [Self; 4] = [
        Self::MotionBlocking,
        Self::MotionBlockingNoLeaves,
        Self::OceanFloor,
        Self::WorldSurface,
    ];
    pub(super) fn matches(self, state: BlockState, tags: &TagSet<Block>) -> bool {
        use ferraloom_data::tags::BlockTags;
        match self {
            Self::WorldSurface => !state.is_air(),
            Self::OceanFloor => {
                tags.contains_key(BlockTags::BLOCKS_MOTION_IN_HEIGHTMAP, state.block())
            }
            Self::MotionBlocking => {
                tags.contains_key(BlockTags::BLOCKS_MOTION_IN_HEIGHTMAP, state.block())
                    || !state.data().fluid.empty
            }
            Self::MotionBlockingNoLeaves => {
                tags.contains_key(
                    BlockTags::BLOCKS_MOTION_IN_HEIGHTMAP_NO_LEAVES,
                    state.block(),
                ) || !state.data().fluid.empty
            }
        }
    }
}
