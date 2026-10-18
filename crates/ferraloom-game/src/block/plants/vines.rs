//! Port of VineBlock's attachment maintenance and survival queries.
use super::{
    super::{BlockBehavior, LevelAccessRef, LevelReaderRef},
    multiface::{can_attach_to_neighbor, has_any_face, has_face, with_face},
};
use crate::level::LevelError;
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{BlockPos, Direction, core::Plane};
pub struct Vines;
pub static VINES: Vines = Vines;
/// Port of VineBlock#isAcceptableNeighbour; pos is the neighbor's position.
pub fn acceptable_neighbor(
    level: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
    toward: Direction,
) -> Result<bool, LevelError> {
    can_attach_to_neighbor(level, level.get_block_state(pos)?, pos, toward)
}
fn updated_state(
    mut state: BlockState,
    pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
) -> Result<BlockState, LevelError> {
    if has_face(state, Direction::Up) {
        state = with_face(
            state,
            Direction::Up,
            acceptable_neighbor(level, pos.above(1), Direction::Down)?,
        )
        .unwrap();
    }
    for &face in Plane::Horizontal.faces() {
        if has_face(state, face) {
            let supported = acceptable_neighbor(level, pos.relative(face, 1), face)? || {
                let above = level.get_block_state(pos.above(1))?;
                above.block() == Block::VINE && has_face(above, face)
            };
            state = with_face(state, face, supported).unwrap();
        }
    }
    Ok(state)
}
impl BlockBehavior for Vines {
    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        Ok(has_any_face(updated_state(state, pos, level)?))
    }
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        if direction == Direction::Down {
            return Ok(state);
        }
        let next = updated_state(state, pos, &level.as_reader())?;
        Ok(if has_any_face(next) {
            next
        } else {
            Block::AIR.default_state()
        })
    }
}
