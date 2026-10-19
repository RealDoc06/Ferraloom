//! RotatedPillarBlock and basic horizontal/waterlogged placement virtuals.
use super::{schedule_water, with_facing};
use crate::{
    block::{BlockBehavior, BlockMethod, BlockPlaceContext, LevelAccessRef},
    level::LevelError,
};
use ferraloom_data::{
    Block, BlockClass as C, BlockState, properties as p, registries::fluid::Id as Fluid,
};
use ferraloom_util::{Axis, BlockPos, Direction};

pub(super) struct Orientation;
pub(super) static ORIENTATION: Orientation = Orientation;

fn waterlogged(block: Block) -> bool {
    matches!(
        block.class(),
        C::ChainBlock
            | C::WeatheringCopperChainBlock
            | C::HeavyCoreBlock
            | C::WaterloggedTransparentBlock
            | C::WeatheringCopperGrateBlock
    )
}

pub(super) fn supports(block: Block, method: BlockMethod) -> bool {
    match method {
        BlockMethod::GetStateForPlacement => {
            waterlogged(block)
                || matches!(
                    block.class(),
                    C::RotatedPillarBlock
                        | C::HayBlock
                        | C::InfestedRotatedPillarBlock
                        | C::GlazedTerracottaBlock
                        | C::AnvilBlock
                        | C::LoomBlock
                        | C::StonecutterBlock
                )
        }
        BlockMethod::UpdateShape | BlockMethod::UpdateFromNeighbourShapes => {
            waterlogged(block) || block.class() == C::AnvilBlock
        }
        _ => false,
    }
}

impl BlockBehavior for Orientation {
    /// Ports the native getStateForPlacement implementations of the registered
    /// pillar, chain, grate, heavy core, terracotta, anvil and workstation classes.
    fn get_state_for_placement(
        &self,
        block: Block,
        context: &BlockPlaceContext<'_>,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<BlockState>, LevelError> {
        let reader = level.as_reader();
        let mut state = block.default_state();
        // ChainBlock reads the replaced fluid before its RotatedPillarBlock
        // super call. Preserve that required query before resolving the axis.
        if waterlogged(block) {
            state = state
                .with(
                    p::WATERLOGGED,
                    reader.get_fluid_state(context.clicked_pos())?.fluid_type == Fluid::WATER,
                )
                .unwrap();
        }
        state = match block.class() {
            C::RotatedPillarBlock
            | C::HayBlock
            | C::InfestedRotatedPillarBlock
            | C::ChainBlock
            | C::WeatheringCopperChainBlock => state
                .with(
                    p::AXIS,
                    match context.clicked_face().axis() {
                        Axis::X => p::Axis::X,
                        Axis::Y => p::Axis::Y,
                        Axis::Z => p::Axis::Z,
                    },
                )
                .unwrap(),
            C::GlazedTerracottaBlock | C::LoomBlock | C::StonecutterBlock => {
                with_facing(state, context.horizontal_direction(&reader)?.opposite())
            }
            C::AnvilBlock => with_facing(state, context.horizontal_direction(&reader)?.clockwise()),
            _ => state,
        };
        Ok(Some(state))
    }

    /// Ports ChainBlock/HeavyCoreBlock/WaterloggedTransparentBlock#updateShape
    /// and AnvilBlock's inherited FallingBlock#updateShape.
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        neighbor_pos: BlockPos,
        neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        if state.block().class() == C::AnvilBlock {
            return super::super::foundational::FALLING_BLOCK.update_shape(
                state,
                pos,
                direction,
                neighbor_pos,
                neighbor,
                level,
            );
        }
        schedule_water(state, pos, level)?;
        Ok(state)
    }
}
