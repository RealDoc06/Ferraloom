//! ServerPlayerGameMode's mining state and Player's canonical mining queries.
//! GameType and abilities remain on ServerPlayer; this is not another game mode.
use super::{ServerPlayer, inventory};
use crate::{
    entity::EntityRef,
    level::{Level, LevelError},
};
use ferraloom_data::{
    BlockState,
    registries::{attribute::Id as Attribute, mob_effect::Id as MobEffect},
    tags::FluidTags,
};
use ferraloom_util::{BlockPos, Direction};

mod block_actions;
mod mining_item;
pub use block_actions::{
    BlockBreakAction, BlockBreakError, BlockBreakFailure, DestroyAction, destroy_block,
    handle_block_break_action, tick,
};
pub use mining_item::mine_block;

/// Port of ServerPlayerGameMode's destruction fields, owned by ServerPlayer.
#[derive(Debug)]
pub struct BlockBreakingState {
    pub is_destroying_block: bool,
    pub destroy_progress_start: i32,
    pub destroy_pos: BlockPos,
    pub destroy_direction: Direction,
    pub game_ticks: i32,
    pub has_delayed_destroy: bool,
    pub delayed_destroy_pos: BlockPos,
    pub delayed_tick_start: i32,
    pub last_sent_state: i32,
}
impl Default for BlockBreakingState {
    fn default() -> Self {
        Self {
            is_destroying_block: false,
            destroy_progress_start: 0,
            destroy_pos: BlockPos::ZERO,
            destroy_direction: Direction::Down,
            game_ticks: 0,
            has_delayed_destroy: false,
            delayed_destroy_pos: BlockPos::ZERO,
            delayed_tick_start: 0,
            last_sent_state: -1,
        }
    }
}
impl BlockBreakingState {
    /// Port of the two ServerPlayerGameMode#tick progress operands. In 26.3 the
    /// delayed branch passes delayedTickStart itself, not elapsed gameTicks.
    pub fn progress_ticks(&self) -> Option<i32> {
        if self.has_delayed_destroy {
            Some(self.delayed_tick_start)
        } else if self.is_destroying_block {
            Some(self.game_ticks.wrapping_sub(self.destroy_progress_start))
        } else {
            None
        }
    }
}

/// Port of ServerPlayerGameMode#incrementDestroyProgress's arithmetic. The
/// caller sends a changed stage before committing lastSentState, as vanilla does.
pub fn accumulated_destroy_progress(per_tick: f32, ticks_spent: i32) -> (f32, i32) {
    let progress = per_tick * ticks_spent.wrapping_add(1) as f32;
    (progress, (progress * 10.0) as i32)
}

/// Port of Player#getDestroySpeed in native float order. Reborrows end before
/// the underwater getAttribute call lazily creates its canonical instance.
pub fn get_destroy_speed(
    player: EntityRef,
    state: BlockState,
    level: &mut Level,
) -> Result<f32, LevelError> {
    let mut speed = inventory::get_destroy_speed(
        level
            .entity::<ServerPlayer>(player)?
            .inventory
            .selected_item(),
        state,
        level,
    )?;
    if speed > 1.0 {
        speed += level
            .entity::<ServerPlayer>(player)?
            .attributes
            .value(Attribute::MINING_EFFICIENCY)
            .expect("player supplier defines mining efficiency") as f32;
    }
    let entity = level.entity::<ServerPlayer>(player)?;
    if entity.has_effect(MobEffect::HASTE) || entity.has_effect(MobEffect::CONDUIT_POWER) {
        let haste = entity
            .get_effect(MobEffect::HASTE)
            .map_or(0, |effect| effect.amplifier());
        let conduit = entity
            .get_effect(MobEffect::CONDUIT_POWER)
            .map_or(0, |effect| effect.amplifier());
        speed *= 1.0 + haste.max(conduit).wrapping_add(1) as f32 * 0.2;
    }
    if let Some(effect) = entity.get_effect(MobEffect::MINING_FATIGUE) {
        let scale = 0.3_f64.powf(f64::from(effect.amplifier().wrapping_add(1))) as f32;
        speed *= scale;
    }
    speed *= entity
        .attributes
        .value(Attribute::BLOCK_BREAK_SPEED)
        .expect("player supplier defines block break speed") as f32;
    if entity
        .fluid_interaction
        .eyes_inside(FluidTags::WATER, level)?
    {
        speed *= level
            .entity_mut::<ServerPlayer>(player)?
            .attributes
            .get_instance(Attribute::SUBMERGED_MINING_SPEED)
            .expect("player supplier defines submerged mining speed")
            .value() as f32;
    }
    if !level.entity::<ServerPlayer>(player)?.on_ground {
        speed /= 5.0;
    }
    Ok(speed)
}

#[cfg(test)]
mod block_action_tests;
#[cfg(test)]
mod player_destroy_tests;
#[cfg(test)]
mod tests;
