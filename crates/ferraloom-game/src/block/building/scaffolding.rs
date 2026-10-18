//! Scaffolding placement distance, replacement, survival and shape-update ticks.
use super::schedule_water;
use crate::{
    block::{BlockBehavior, BlockMethod, BlockPlaceContext, LevelAccessRef, LevelReaderRef},
    level::LevelError,
};
use ferraloom_data::{
    Block, BlockClass as C, BlockState, properties as p, registries::fluid::Id as Fluid,
};
use ferraloom_util::{BlockPos, Direction};

pub(super) struct Scaffolding;
pub(super) static SCAFFOLDING: Scaffolding = Scaffolding;
pub(super) fn supports(block: Block, method: BlockMethod) -> bool {
    block.class() == C::ScaffoldingBlock
        && matches!(
            method,
            BlockMethod::GetStateForPlacement
                | BlockMethod::CanBeReplaced
                | BlockMethod::CanSurvive
                | BlockMethod::UpdateShape
                | BlockMethod::UpdateFromNeighbourShapes
        )
}
/// Port of ScaffoldingBlock#getDistance, including its distance==1 early exit.
fn distance(pos: BlockPos, level: &LevelReaderRef<'_, '_>) -> Result<u8, LevelError> {
    let at = pos.below(1);
    let below = level.get_block_state(at)?;
    let mut distance = 7;
    if below.block() == Block::SCAFFOLDING {
        distance = below.get(p::DISTANCE).unwrap();
    } else if level.is_face_sturdy(below, at, Direction::Up)? {
        return Ok(0);
    }
    for side in [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ] {
        let neighbor = level.get_block_state(pos.relative(side, 1))?;
        if neighbor.block() == Block::SCAFFOLDING {
            distance = distance.min(neighbor.get(p::DISTANCE).unwrap() + 1);
            if distance == 1 {
                break;
            }
        }
    }
    Ok(distance)
}
impl BlockBehavior for Scaffolding {
    /// Port of ScaffoldingBlock#getStateForPlacement; bottom is queried after
    /// the fluid, using the computed distance and a fresh below-state read.
    fn get_state_for_placement(
        &self,
        block: Block,
        context: &BlockPlaceContext<'_>,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<BlockState>, LevelError> {
        let reader = level.as_reader();
        let pos = context.clicked_pos();
        let distance = distance(pos, &reader)?;
        let water = reader.get_fluid_state(pos)?.fluid_type == Fluid::WATER;
        let bottom = distance > 0 && reader.get_block_state(pos.below(1))?.block() != block;
        Ok(Some(
            block
                .default_state()
                .with(p::WATERLOGGED, water)
                .unwrap()
                .with(p::DISTANCE, distance)
                .unwrap()
                .with(p::BOTTOM, bottom)
                .unwrap(),
        ))
    }
    fn can_be_replaced(
        &self,
        state: BlockState,
        context: &BlockPlaceContext<'_>,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        Ok(context.item_in_hand(level)?.item() == state.block().as_item())
    }
    fn can_survive(
        &self,
        _state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        Ok(distance(pos, level)? < 7)
    }
    /// Port of ScaffoldingBlock#updateShape; both concrete adapters are server
    /// access, and schedule the native one-tick block update after the water.
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        schedule_water(state, pos, level)?;
        level.schedule_block_tick(pos, state.block(), 1)?;
        Ok(state)
    }
}
