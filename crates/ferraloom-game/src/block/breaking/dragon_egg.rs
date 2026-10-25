//! DragonEggBlock's server-side attack teleport. Falling ticks and the other
//! interaction/shape virtuals remain on their own per-method dispatch.
use crate::{
    block::{BlockBehavior, LevelAccessRef, UPDATE_LIMIT, UpdateFlags},
    entity::EntityRef,
    level::{Level, LevelError},
};
use ferraloom_data::BlockState;
use ferraloom_util::{BlockPos, Direction, random::RandomSource};

pub(super) struct DragonEggBlock;
pub(super) static DRAGON_EGG: DragonEggBlock = DragonEggBlock;
impl BlockBehavior for DragonEggBlock {
    /// Port of DragonEggBlock#attack -> teleport.
    fn attack(
        &self,
        state: BlockState,
        pos: BlockPos,
        _player: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        teleport(state, pos, level)
    }
    /// Port of FallingBlock#onPlace with DragonEggBlock#getDelayAfterPlace.
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        _old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level.schedule_block_tick_after(pos, state.block(), 5)
    }
    /// Port of FallingBlock#updateShape with the dragon egg's native delay 5.
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        level.schedule_block_tick(pos, state.block(), 5)?;
        Ok(state)
    }
}

/// Port of DragonEggBlock#teleport. Every candidate draws X/Y/Z pairs first,
/// then performs air, support, border and build-height checks in source order.
fn teleport(state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
    for _ in 0..1000 {
        let random = level.random();
        let dx = random.next_int_bound(16) - random.next_int_bound(16);
        let dy = random.next_int_bound(8) - random.next_int_bound(8);
        let dz = random.next_int_bound(16) - random.next_int_bound(16);
        let target = pos.offset(dx, dy, dz);
        if level.get_block_state(target)?.is_air()
            && !level.get_block_state(target.below(1))?.is_air()
            && level.entity_world_border().is_within_block(target)
            && (level.min_build_height()..=level.max_build_height()).contains(&target.y)
        {
            // Port of BlockUtil#packDifferenceInPosition(pos,target,16,8,16).
            let packed = ((dx + 16) & 0xff) << 16 | ((dy + 8) & 0xff) << 8 | ((dz + 16) & 0xff);
            level.level_event(None, 2015, pos, packed)?;
            level.set_block(target, state, UpdateFlags::CLIENTS, UPDATE_LIMIT)?;
            level.remove_block(pos, false)?;
            return Ok(());
        }
    }
    Ok(())
}
