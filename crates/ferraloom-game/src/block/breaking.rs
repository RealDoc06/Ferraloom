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
