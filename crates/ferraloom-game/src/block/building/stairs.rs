//! StairBlock placement and inner/outer corner selection.
use super::{facing, schedule_water, top_half, with_facing};
use crate::{
    block::{BlockBehavior, BlockMethod, BlockPlaceContext, LevelAccessRef, LevelReaderRef},
    level::LevelError,
};
use ferraloom_data::{
    Block, BlockClass as C, BlockState, properties as p, registries::fluid::Id as Fluid,
};
use ferraloom_util::{BlockPos, Direction};

pub(super) struct Stairs;
pub(super) static STAIRS: Stairs = Stairs;

pub(super) fn supports(block: Block, method: BlockMethod) -> bool {
    matches!(block.class(), C::StairBlock | C::WeatheringCopperStairBlock)
        && matches!(
            method,
            BlockMethod::GetStateForPlacement
                | BlockMethod::UpdateShape
                | BlockMethod::UpdateFromNeighbourShapes
        )
}

/// Port of StairBlock#canTakeShape. Any stair subclass with the same facing and
/// half blocks a corner, independently of its own shape, material and water.
pub(super) fn can_take_shape(
    state: BlockState,
    level: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
    direction: Direction,
) -> Result<bool, LevelError> {
    let neighbor = level.get_block_state(pos.relative(direction, 1))?;
    Ok(!neighbor.block().is_instance_of(C::StairBlock)
        || neighbor.get(p::FACING) != state.get(p::FACING)
        || neighbor.get(p::HALF) != state.get(p::HALF))
}

/// Port of StairBlock#getStairsShape. The outer corner is checked before the
/// inner corner, including each branch's side obstruction and required reads.
pub(super) fn stairs_shape(
    state: BlockState,
    level: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
) -> Result<p::Shape, LevelError> {
    let direction = facing(state);
    let behind = level.get_block_state(pos.relative(direction, 1))?;
    if behind.block().is_instance_of(C::StairBlock) && behind.get(p::HALF) == state.get(p::HALF) {
        let neighbor_facing = facing(behind);
        if neighbor_facing.axis() != direction.axis()
            && can_take_shape(state, level, pos, neighbor_facing.opposite())?
        {
            return Ok(if neighbor_facing == direction.counter_clockwise() {
                p::Shape::OuterLeft
            } else {
                p::Shape::OuterRight
            });
        }
    }
    let front = level.get_block_state(pos.relative(direction.opposite(), 1))?;
    if front.block().is_instance_of(C::StairBlock) && front.get(p::HALF) == state.get(p::HALF) {
        let neighbor_facing = facing(front);
        if neighbor_facing.axis() != direction.axis()
            && can_take_shape(state, level, pos, neighbor_facing)?
        {
            return Ok(if neighbor_facing == direction.counter_clockwise() {
                p::Shape::InnerLeft
            } else {
                p::Shape::InnerRight
            });
        }
    }
    Ok(p::Shape::Straight)
}

impl BlockBehavior for Stairs {
    /// Port of StairBlock#getStateForPlacement.
    fn get_state_for_placement(
        &self,
        block: Block,
        context: &BlockPlaceContext<'_>,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<BlockState>, LevelError> {
        let reader = level.as_reader();
        let pos = context.clicked_pos();
        let fluid = reader.get_fluid_state(pos)?;
        let state = with_facing(
            block.default_state(),
            context.horizontal_direction(&reader)?,
        )
        .with(
            p::HALF,
            if top_half(context) {
                p::Half::Top
            } else {
                p::Half::Bottom
            },
        )
        .unwrap()
        .with(p::WATERLOGGED, fluid.fluid_type == Fluid::WATER)
        .unwrap();
        Ok(Some(
            state
                .with(p::SHAPE, stairs_shape(state, &reader, pos)?)
                .unwrap(),
        ))
    }

    /// Port of StairBlock#updateShape. Vertical notifications keep the captured
    /// stair shape; horizontal notifications recompute it from the actual level.
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        schedule_water(state, pos, level)?;
        Ok(if direction.axis().is_horizontal() {
            state
                .with(p::SHAPE, stairs_shape(state, &level.as_reader(), pos)?)
                .unwrap()
        } else {
            state
        })
    }
}
