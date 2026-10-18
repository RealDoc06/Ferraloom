//! Port of HugeMushroomBlock's exposed cap faces. Rotations and mirrors use
//! BlockState's existing state-only native transform tables.
use super::multiface::with_face;
use crate::{
    block::{BlockBehavior, BlockPlaceContext, LevelAccessRef},
    level::LevelError,
};
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{BlockPos, Direction};

pub struct HugeMushroom;
pub static HUGE_MUSHROOM: HugeMushroom = HugeMushroom;

impl BlockBehavior for HugeMushroom {
    /// Port of HugeMushroomBlock#updateShape. Removing a neighbor never restores
    /// an already hidden face; only initial placement computes all exposed faces.
    fn update_shape(
        &self,
        state: BlockState,
        _pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor: BlockState,
        _level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        Ok(if neighbor.block() == state.block() {
            with_face(state, direction, false).expect("huge mushroom face")
        } else {
            state
        })
    }

    /// Port of HugeMushroomBlock#getStateForPlacement, including native read order.
    fn get_state_for_placement(
        &self,
        block: Block,
        context: &BlockPlaceContext<'_>,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<BlockState>, LevelError> {
        let pos = context.clicked_pos();
        let mut state = block.default_state();
        for direction in [
            Direction::Down,
            Direction::Up,
            Direction::North,
            Direction::East,
            Direction::South,
            Direction::West,
        ] {
            let exposed = level.get_block_state(pos.relative(direction, 1))?.block() != block;
            state = with_face(state, direction, exposed).expect("huge mushroom face");
        }
        Ok(Some(state))
    }
}

#[cfg(test)]
mod tests;
