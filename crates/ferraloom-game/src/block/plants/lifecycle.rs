//! DoublePlantBlock's placement/destruction lifecycle and subclass placement overrides.
use super::super::{BASE_BLOCK, BlockBehavior, LevelAccessRef, UpdateFlags, drops};
use crate::{
    entity::{EntityRef, player::ServerPlayer},
    level::{Level, LevelError},
};
use ferraloom_data::{Block, BlockClass, BlockState, properties as p, tags::FluidTags};
use ferraloom_util::BlockPos;

pub struct PlantLifecycle;
pub static PLANT_LIFECYCLE: PlantLifecycle = PlantLifecycle;
impl BlockBehavior for PlantLifecycle {
    fn set_placed_by(
        &self,
        state: BlockState,
        pos: BlockPos,
        _by: Option<EntityRef>,
        _stack: crate::block::StackView<'_>,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if state.block().class() == BlockClass::PitcherCropBlock {
            return Ok(());
        }
        let above = pos.above(1);
        let mut upper = state
            .block()
            .default_state()
            .with(p::HALF, p::Half::Upper)
            .unwrap();
        if state.block() == Block::SMALL_DRIPLEAF {
            upper = upper
                .with(p::FACING, state.get(p::FACING).unwrap())
                .unwrap();
        }
        if upper.get(p::WATERLOGGED).is_some() {
            let fluid = level.get_block_state(above)?.data().fluid;
            let water = level
                .fluid_tags()?
                .contains_key(FluidTags::WATER, fluid.fluid_type);
            upper = upper.with(p::WATERLOGGED, water).unwrap();
        }
        LevelAccessRef::Server(level).set_block(above, upper, UpdateFlags::ALL)?;
        Ok(())
    }
    fn player_will_destroy(
        &self,
        state: BlockState,
        pos: BlockPos,
        player: EntityRef,
        level: &mut Level,
    ) -> Result<BlockState, LevelError> {
        if level.entity::<ServerPlayer>(player)?.prevents_block_drops() {
            if state.get(p::HALF) == Some(p::Half::Upper) {
                let below = pos.below(1);
                let lower = level.get_block_state(below)?;
                if lower.block() == state.block() && lower.get(p::HALF) == Some(p::Half::Lower) {
                    let next = if lower.data().fluid.fluid_type
                        == ferraloom_data::registries::fluid::Id::WATER
                    {
                        Block::WATER
                    } else {
                        Block::AIR
                    }
                    .default_state();
                    LevelAccessRef::Server(level).set_block(below, next, UpdateFlags(35))?;
                    BASE_BLOCK.spawn_destroy_by_entity_particles(
                        lower,
                        below,
                        Some(player),
                        level,
                    )?;
                }
            }
        } else {
            let tool = level
                .entity::<ServerPlayer>(player)?
                .get_main_hand_item()
                .clone();
            drops::drop_resources_with_tool(state, pos, None, Some(player), &tool, level)?;
        }
        BASE_BLOCK.player_will_destroy(state, pos, player, level)
    }
}
