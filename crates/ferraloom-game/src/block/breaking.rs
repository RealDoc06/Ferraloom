//! Player mining queries and the BlockBehaviour destruction virtual boundary.
use super::BlockMethod;
use crate::{
    entity::{
        EntityRef,
        player::{ServerPlayer, game_mode},
    },
    level::{Level, LevelError},
};
use ferraloom_data::BlockState;
use ferraloom_util::BlockPos;

mod dragon_egg;
mod turtle_egg;

pub(crate) fn ported_behavior(
    block: ferraloom_data::Block,
    method: BlockMethod,
) -> Option<&'static dyn super::BlockBehavior> {
    use BlockMethod as M;
    use ferraloom_data::Block;
    match block {
        Block::TURTLE_EGG if matches!(method, M::PlayerDestroy | M::OnPlace) => {
            Some(&turtle_egg::TURTLE_EGG)
        }
        Block::DRAGON_EGG
            if matches!(
                method,
                M::Attack | M::OnPlace | M::UpdateShape | M::UpdateFromNeighbourShapes
            ) =>
        {
            Some(&dragon_egg::DRAGON_EGG)
        }
        _ => None,
    }
}

/// Bundles Block#playerDestroy(ServerLevel, ServerPlayer, BlockPos, BlockState,
/// BlockEntity, ItemStack), retaining the original Block#this separately from
/// the adjusted state and the native pre-wear destroyedWith copy.
#[derive(Clone, Copy)]
pub struct PlayerDestroyContext<'a> {
    pub block: ferraloom_data::Block,
    pub state: BlockState,
    pub pos: BlockPos,
    pub player: EntityRef,
    pub block_entity: Option<super::entity::BlockEntityRef>,
    pub destroyed_with: &'a ferraloom_protocol::component::ItemStack,
}

/// Port of Block#playerDestroy's base body. Family super calls retain the
/// original receiver even when they pass another state, including AIR.
pub fn player_destroy(
    context: PlayerDestroyContext<'_>,
    level: &mut Level,
) -> Result<(), LevelError> {
    crate::entity::player::award_stat(
        context.player,
        crate::stats::Stat::Mined(context.block),
        1,
        level,
    )?;
    level
        .entity_mut::<ServerPlayer>(context.player)?
        .cause_food_exhaustion(0.005);
    super::drops::drop_resources_with_tool(
        context.state,
        context.pos,
        context.block_entity,
        Some(context.player),
        context.destroyed_with,
        level,
    )
}

/// Port of BlockBehaviour#getDestroyProgress. Unbreakable hardness returns
/// before any player, tool, attribute, effect or fluid query.
pub(super) fn get_destroy_progress(
    state: BlockState,
    player: EntityRef,
    level: &mut Level,
) -> Result<f32, LevelError> {
    let hardness = state.data().destroy_time;
    if hardness == -1.0 {
        return Ok(0.0);
    }
    let divisor = if level
        .entity::<ServerPlayer>(player)?
        .has_correct_tool_for_drops(state, level)?
    {
        30.0
    } else {
        100.0
    };
    Ok(game_mode::get_destroy_speed(player, state, level)? / hardness / divisor)
}

impl Level {
    /// Port of BlockStateBase#getDestroyProgress. The mutable receiver permits
    /// Player's native underwater attribute-instance creation.
    pub fn get_destroy_progress(
        &mut self,
        state: BlockState,
        pos: BlockPos,
        player: EntityRef,
    ) -> Result<f32, LevelError> {
        self.required_block_behavior(state.block(), BlockMethod::GetDestroyProgress)?
            .get_destroy_progress(state, pos, player, self)
    }

    /// Port of BlockStateBase#attack's virtual effect dispatch.
    pub fn attack_block(
        &mut self,
        state: BlockState,
        pos: BlockPos,
        player: EntityRef,
    ) -> Result<(), LevelError> {
        self.optional_block_call(state.block(), BlockMethod::Attack, |behavior, level| {
            behavior.attack(state, pos, player, level)
        })
    }
}
