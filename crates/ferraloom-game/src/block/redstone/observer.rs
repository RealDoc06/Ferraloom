//! Port of ObserverBlock's scheduled two-tick pulse.
use super::{initial_orientation, state::facing};
use crate::{
    block::{BlockBehavior, LevelAccessRef, LevelReaderRef, UPDATE_LIMIT, UpdateFlags},
    level::{Level, LevelError},
};
use ferraloom_data::{Block, BlockState, properties as p};
use ferraloom_util::{BlockPos, Direction};

pub struct ObserverBlock;
pub static OBSERVER: ObserverBlock = ObserverBlock;
fn update_front(level: &mut Level, pos: BlockPos, state: BlockState) -> Result<(), LevelError> {
    let direction = facing(state);
    let opposite = pos.relative(direction.opposite(), 1);
    let orientation = initial_orientation(level, Some(direction.opposite()), None);
    level.neighbor_changed(opposite, state.block(), orientation)?;
    level.update_neighbors_at_except_from_facing(
        opposite,
        state.block(),
        Some(direction),
        orientation,
    )
}
impl BlockBehavior for ObserverBlock {
    fn is_signal_source(&self, _state: BlockState) -> bool {
        true
    }
    fn own_signal(
        &self,
        state: BlockState,
        _pos: BlockPos,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        Ok(if state.get(p::POWERED).unwrap() {
            15
        } else {
            0
        })
    }
    fn signal(
        &self,
        state: BlockState,
        _pos: BlockPos,
        direction: Direction,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        Ok(
            if facing(state) == direction && state.get(p::POWERED).unwrap() {
                15
            } else {
                0
            },
        )
    }
    fn direct_signal(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        self.signal(state, pos, direction, level)
    }
    fn should_redstone_wire_connect_to(
        &self,
        state: BlockState,
        _pos: BlockPos,
        direction: Option<Direction>,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        Ok(direction == Some(facing(state)))
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        let powered = state.get(p::POWERED).unwrap();
        level.set_block(
            pos,
            state.with(p::POWERED, !powered).unwrap(),
            UpdateFlags::CLIENTS,
            UPDATE_LIMIT,
        )?;
        if !powered {
            level.schedule_block_tick_after(pos, state.block(), 2)?;
        }
        update_front(level, pos, state)
    }
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        if facing(state) == direction && !state.get(p::POWERED).unwrap() {
            match level {
                LevelAccessRef::Server(server) => {
                    if !server.has_scheduled_block_tick(pos, state.block()) {
                        server.schedule_block_tick_after(pos, state.block(), 2)?;
                    }
                }
                // ProtoChunkTicks#schedule deduplicates the same position/type.
                LevelAccessRef::Worldgen(_) => level.schedule_block_tick(pos, state.block(), 2)?,
            }
        }
        Ok(state)
    }
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if old.block() != state.block()
            && state.get(p::POWERED).unwrap()
            && !level.has_scheduled_block_tick(pos, state.block())
        {
            let new = state.with(p::POWERED, false).unwrap();
            level.set_block(pos, new, UpdateFlags(18), UPDATE_LIMIT)?;
            update_front(level, pos, new)?;
        }
        Ok(())
    }
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        pos: BlockPos,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if state.get(p::POWERED).unwrap() && level.has_scheduled_block_tick(pos, state.block()) {
            update_front(level, pos, state.with(p::POWERED, false).unwrap())?;
        }
        Ok(())
    }
}
