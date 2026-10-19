//! SlabBlock placement, replacement and double-slab waterlogging restrictions.
use super::{schedule_water, top_half};
use crate::{
    block::{
        BlockBehavior, BlockMethod, BlockPlaceContext, LevelAccessRef, LevelReaderRef,
        fluid::SimpleWaterloggedBlock,
    },
    level::LevelError,
};
use ferraloom_data::{
    Block, BlockClass as C, BlockState, FluidState, properties as p, registries::fluid::Id as Fluid,
};
use ferraloom_util::{BlockPos, Direction};

pub(super) struct Slab;
pub(super) static SLAB: Slab = Slab;

pub(super) fn supports(block: Block, method: BlockMethod) -> bool {
    matches!(block.class(), C::SlabBlock | C::WeatheringCopperSlabBlock)
        && matches!(
            method,
            BlockMethod::GetStateForPlacement
                | BlockMethod::CanBeReplaced
                | BlockMethod::UpdateShape
                | BlockMethod::UpdateFromNeighbourShapes
                | BlockMethod::CanPlaceLiquid
                | BlockMethod::PlaceLiquid
        )
}

impl BlockBehavior for Slab {
    /// Port of SlabBlock#getStateForPlacement. Joining an existing slab preserves
    /// its other state properties and clears water without another fluid query.
    fn get_state_for_placement(
        &self,
        block: Block,
        context: &BlockPlaceContext<'_>,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<BlockState>, LevelError> {
        let reader = level.as_reader();
        let pos = context.clicked_pos();
        let old = reader.get_block_state(pos)?;
        if old.block() == block {
            return Ok(Some(
                old.with(p::TYPE_, p::Type::Double)
                    .unwrap()
                    .with(p::WATERLOGGED, false)
                    .unwrap(),
            ));
        }
        let fluid = reader.get_fluid_state(pos)?;
        Ok(Some(
            block
                .default_state()
                .with(
                    p::TYPE_,
                    if top_half(context) {
                        p::Type::Top
                    } else {
                        p::Type::Bottom
                    },
                )
                .unwrap()
                .with(p::WATERLOGGED, fluid.fluid_type == Fluid::WATER)
                .unwrap(),
        ))
    }

    /// Port of SlabBlock#canBeReplaced, including the constructor-time query
    /// where BlockPlaceContext still reports replacingClickedOnBlock=true.
    fn can_be_replaced(
        &self,
        state: BlockState,
        context: &BlockPlaceContext<'_>,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let item = context.item_in_hand(level)?;
        let kind = state.get(p::TYPE_).unwrap();
        if kind == p::Type::Double || item.item() != state.block().as_item() {
            return Ok(false);
        }
        if !context.replacing_clicked_on_block(level)? {
            return Ok(true);
        }
        let above = context.click_location().y - f64::from(context.clicked_pos().y) > 0.5;
        let face = context.clicked_face();
        Ok(if kind == p::Type::Bottom {
            face == Direction::Up || above && face.axis().is_horizontal()
        } else {
            face == Direction::Down || !above && face.axis().is_horizontal()
        })
    }

    /// Port of SlabBlock#updateShape.
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
        Ok(state)
    }

    fn can_place_liquid(&self, state: BlockState, fluid: Fluid) -> bool {
        state.get(p::TYPE_) != Some(p::Type::Double) && fluid == Fluid::WATER
    }

    fn place_liquid(
        &self,
        state: BlockState,
        pos: BlockPos,
        fluid: FluidState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        if state.get(p::TYPE_) == Some(p::Type::Double) {
            return Ok(false);
        }
        SimpleWaterloggedBlock::place_liquid(state, pos, fluid, level)
    }
}
