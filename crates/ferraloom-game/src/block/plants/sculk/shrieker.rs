//! Port of SculkShriekerBlockEntity's ordinary shriek branch.
use crate::{
    block::{LevelAccessRef, UpdateFlags, entity::BlockEntityRef},
    entity::EntityRef,
    level::{Level, LevelError},
};
use ferraloom_data::{properties as p, registries::game_event::Id as GameEvent};
use ferraloom_protocol::play::Difficulty;
use ferraloom_util::Vec3;

/// Port of SculkShriekerBlockEntity#tryShriek. WardenSpawnTracker and spawning
/// remain required seams; the warning reset precedes the native response query.
pub(super) fn try_shriek(
    receiver: BlockEntityRef,
    player: Option<EntityRef>,
    level: &mut Level,
) -> Result<(), LevelError> {
    let Some(player) = player else {
        return Ok(());
    };
    let state = level.block_entity_state(receiver)?.data().block_state;
    if state.get(p::SHRIEKING) == Some(true) {
        return Ok(());
    }
    level
        .block_entity_mut::<crate::block::entity::sculk::SculkShriekerBlockEntity>(receiver)?
        .warning_level = 0;
    if state.get(p::CAN_SUMMON) == Some(true) && level.difficulty() != Difficulty::Peaceful {
        // canRespond next needs canonical SPAWN_WARDENS and WardenSpawnTracker.
        return Err(LevelError::UnsupportedGameRule("minecraft:spawn_wardens"));
    }
    LevelAccessRef::Server(level).set_block(
        receiver.pos,
        state.with(p::SHRIEKING, true).unwrap(),
        UpdateFlags::CLIENTS,
    )?;
    level.schedule_block_tick_after(receiver.pos, state.block(), 90)?;
    level.level_event(None, 3007, receiver.pos, 0)?;
    level.game_event(
        GameEvent::SHRIEK,
        Vec3::at_center_of(receiver.pos.into()),
        Some(player),
        None,
    )?;
    Ok(())
}
