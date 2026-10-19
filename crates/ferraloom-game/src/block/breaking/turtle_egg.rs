//! TurtleEggBlock's player-destruction path. Hatching and entity trampling are
//! separate required virtuals and do not inherit this method registration.
use super::PlayerDestroyContext;
use crate::{
    block::{BlockBehavior, UPDATE_LIMIT, UpdateFlags},
    level::{Level, LevelError},
};
use ferraloom_data::{
    BlockState, properties as p,
    registries::{game_event::Id as GameEvent, sound_event::Id as SoundEvent},
    tags::BlockTags,
};
use ferraloom_protocol::play::SoundSource;
use ferraloom_util::{BlockPos, Vec3, random::RandomSource};

pub(super) struct TurtleEggBlock;
pub(super) static TURTLE_EGG: TurtleEggBlock = TurtleEggBlock;
impl BlockBehavior for TurtleEggBlock {
    fn player_destroy(
        &self,
        context: PlayerDestroyContext<'_>,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        super::player_destroy(context, level)?;
        decrease_eggs(context.state, context.pos, level)
    }
    /// Port of TurtleEggBlock#onPlace, also reached when decreaseEggs restores
    /// the remaining eggs after the original block has already been removed.
    fn on_place(
        &self,
        _state: BlockState,
        pos: BlockPos,
        _old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let below = level.get_block_state(pos.below(1))?;
        if level
            .block_tags()?
            .contains_key(BlockTags::SAND, below.block())
        {
            level.level_event(None, 2012, pos, 15)?;
        }
        Ok(())
    }
}

/// Port of TurtleEggBlock#decreaseEggs. The pitch consumes Level#random before
/// playSound consumes its independent sound-seed stream; source effects persist
/// if a later required read or mutation fails.
fn decrease_eggs(state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
    let pitch = 0.9 + level.random().next_float() * 0.2;
    let position = Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    );
    level.play_sound(
        None,
        position,
        SoundEvent::TURTLE_EGG_BREAK,
        SoundSource::Blocks,
        0.7,
        pitch,
    )?;
    let count = state.get(p::EGGS).expect("turtle egg property");
    if count <= 1 {
        level.destroy_block(pos, false, UPDATE_LIMIT)?;
    } else {
        level.set_block(
            pos,
            state.with(p::EGGS, count - 1).unwrap(),
            UpdateFlags::CLIENTS,
            UPDATE_LIMIT,
        )?;
        level.game_event(GameEvent::BLOCK_DESTROY, position, None, Some(state))?;
        level.level_event(None, 2001, pos, i32::from(state.id()))?;
    }
    Ok(())
}
