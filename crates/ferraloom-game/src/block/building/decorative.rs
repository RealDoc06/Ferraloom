//! Candle/egg stacking and basic decorative placement with DriedGhast sound.
use super::{schedule_water, with_facing};
use crate::{
    block::{
        BASE_BLOCK, BlockBehavior, BlockMethod, BlockPlaceContext, LevelAccessRef, LevelReaderRef,
        StackView,
    },
    entity::EntityRef,
    level::{Level, LevelError},
};
use ferraloom_data::{
    Block, BlockClass as C, BlockState, properties as p,
    registries::{fluid::Id as Fluid, sound_event::Id as Sound},
};
use ferraloom_protocol::play::SoundSource;
use ferraloom_util::{BlockPos, Direction, Vec3};

pub(super) struct Decorative;
pub(super) static DECORATIVE: Decorative = Decorative;
fn waterlogged(block: Block) -> bool {
    matches!(
        block.class(),
        C::CandleBlock | C::BarrierBlock | C::DriedGhastBlock
    )
}
pub(super) fn supports(block: Block, method: BlockMethod) -> bool {
    match method {
        BlockMethod::GetStateForPlacement => {
            waterlogged(block)
                || matches!(
                    block.class(),
                    C::TurtleEggBlock | C::CarvedPumpkinBlock | C::EndPortalFrameBlock
                )
        }
        BlockMethod::CanBeReplaced => matches!(block.class(), C::CandleBlock | C::TurtleEggBlock),
        BlockMethod::CanSurvive => block.class() == C::CandleBlock,
        BlockMethod::UpdateShape | BlockMethod::UpdateFromNeighbourShapes => waterlogged(block),
        BlockMethod::SetPlacedBy => block.class() == C::DriedGhastBlock,
        _ => false,
    }
}
impl BlockBehavior for Decorative {
    /// Ports CandleBlock/TurtleEggBlock's native same-block stacking, and the
    /// native pumpkin/frame/barrier/dried-ghast placement state constructors.
    fn get_state_for_placement(
        &self,
        block: Block,
        context: &BlockPlaceContext<'_>,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<BlockState>, LevelError> {
        let reader = level.as_reader();
        if matches!(block.class(), C::CandleBlock | C::TurtleEggBlock) {
            let old = reader.get_block_state(context.clicked_pos())?;
            if old.block() == block {
                return Ok(Some(if block.class() == C::CandleBlock {
                    old.with(p::CANDLES, old.get(p::CANDLES).unwrap() % 4 + 1)
                        .unwrap()
                } else {
                    old.with(p::EGGS, (old.get(p::EGGS).unwrap() + 1).min(4))
                        .unwrap()
                }));
            }
        }
        let mut state = block.default_state();
        if waterlogged(block) {
            state = state
                .with(
                    p::WATERLOGGED,
                    reader.get_fluid_state(context.clicked_pos())?.fluid_type == Fluid::WATER,
                )
                .unwrap();
        }
        if matches!(
            block.class(),
            C::CarvedPumpkinBlock | C::EndPortalFrameBlock | C::DriedGhastBlock
        ) {
            state = with_facing(state, context.horizontal_direction(&reader)?.opposite());
        }
        // EndPortalFrame's default eye is false, as explicitly set by native.
        if block.class() == C::EndPortalFrameBlock {
            state = state.with(p::EYE, false).unwrap();
        }
        Ok(Some(state))
    }
    /// Ports the native matching-item, secondary-use and stack-limit checks.
    fn can_be_replaced(
        &self,
        state: BlockState,
        context: &BlockPlaceContext<'_>,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        if !context.is_secondary_use_active(level)?
            && context.item_in_hand(level)?.item() == state.block().as_item()
        {
            let count = if state.block().class() == C::CandleBlock {
                state.get(p::CANDLES).unwrap()
            } else {
                state.get(p::EGGS).unwrap()
            };
            if count < 4 {
                return Ok(true);
            }
        }
        BASE_BLOCK.can_be_replaced(state, context, level)
    }
    fn can_survive(
        &self,
        _state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        level.can_support_center(pos.below(1), Direction::Up)
    }
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
    /// Port of DriedGhastBlock#setPlacedBy. The base is a no-op; Level retains
    /// vanilla's sound-seed draw and resolves recipients synchronously.
    fn set_placed_by(
        &self,
        state: BlockState,
        pos: BlockPos,
        _by: Option<EntityRef>,
        _stack: StackView<'_>,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level.play_sound(
            None,
            Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y) + 0.5,
                f64::from(pos.z) + 0.5,
            ),
            if state.get(p::WATERLOGGED) == Some(true) {
                Sound::BLOCK_DRIED_GHAST_PLACE_IN_WATER
            } else {
                Sound::BLOCK_DRIED_GHAST_PLACE
            },
            SoundSource::Blocks,
            1.0,
            1.0,
        )
    }
}
