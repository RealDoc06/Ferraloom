//! Placement support for CocoaBlock and ShelfMushroomBlock.
use super::super::{BlockBehavior, BlockPlaceContext, LevelAccessRef, LevelReaderRef};
use crate::level::LevelError;
use ferraloom_data::{Block, BlockState, properties as p, tags::BlockTags};
use ferraloom_util::BlockPos;

pub struct WallPlants;
pub static WALL_PLANTS: WallPlants = WallPlants;

impl BlockBehavior for WallPlants {
    /// Port of CocoaBlock#canSurvive and ShelfMushroomBlock#canSurvive.
    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let facing = super::survival::facing(state);
        if state.block() == Block::COCOA {
            let support = level.get_block_state(pos.relative(facing, 1))?;
            Ok(level
                .block_tags()?
                .contains_key(BlockTags::SUPPORTS_COCOA, support.block()))
        } else {
            let support_pos = pos.relative(facing.opposite(), 1);
            let support = level.get_block_state(support_pos)?;
            level.is_face_sturdy(support, support_pos, facing)
        }
    }

    /// Port of the two horizontal support searches, including their opposite facings.
    fn get_state_for_placement(
        &self,
        block: Block,
        context: &BlockPlaceContext<'_>,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<BlockState>, LevelError> {
        let reader = level.as_reader();
        for direction in context.nearest_looking_directions(&reader)? {
            if !direction.axis().is_horizontal() {
                continue;
            }
            let facing = if block == Block::COCOA {
                direction
            } else {
                direction.opposite()
            };
            let state = block
                .default_state()
                .with(p::FACING, super::placement::facing(facing))
                .unwrap();
            if reader.can_survive(state, context.clicked_pos())? {
                return Ok(Some(state));
            }
        }
        Ok(None)
    }
}
