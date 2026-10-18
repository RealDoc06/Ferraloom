//! Port of RedstoneTorchBlock and RedstoneWallTorchBlock, including shared
//! per-level burnout history. Removal and replacement do not clear that history.
use super::{initial_orientation, state::facing};
use crate::{
    block::{BlockBehavior, LevelAccessRef, LevelReaderRef, UPDATE_LIMIT, UpdateFlags},
    level::{Level, LevelError, effects::LevelEffect, redstone::Orientation},
};
use ferraloom_data::{Block, BlockState, properties as p};
use ferraloom_util::{BlockPos, Direction};

pub struct RedstoneTorchBlock;
pub static TORCH: RedstoneTorchBlock = RedstoneTorchBlock;
fn attached_direction(state: BlockState) -> Direction {
    if state.block() == Block::REDSTONE_WALL_TORCH {
        facing(state).opposite()
    } else {
        Direction::Down
    }
}
fn neighbor_signal(level: &Level, pos: BlockPos, state: BlockState) -> Result<bool, LevelError> {
    let direction = attached_direction(state);
    level.has_signal(pos.relative(direction, 1), direction)
}
fn toggled_too_frequently(level: &mut Level, pos: BlockPos, add: bool) -> bool {
    let time = level.game_time();
    let toggles = &mut level.region_mut().redstone.torch_toggles;
    if add {
        toggles.push_back((pos, time));
    }
    toggles.iter().filter(|(p, _)| *p == pos).take(8).count() == 8
}
fn notify_neighbors(level: &mut Level, pos: BlockPos, state: BlockState) -> Result<(), LevelError> {
    let front = (state.block() == Block::REDSTONE_WALL_TORCH).then(|| facing(state).opposite());
    let orientation = initial_orientation(level, front, Some(Direction::Up));
    for direction in Direction::VALUES {
        level.update_neighbors_at_except_from_facing(
            pos.relative(direction, 1),
            state.block(),
            None,
            orientation.map(|o| o.with_front(direction)),
        )?;
    }
    Ok(())
}
impl BlockBehavior for RedstoneTorchBlock {
    fn is_signal_source(&self, _state: BlockState) -> bool {
        true
    }
    fn own_signal(
        &self,
        state: BlockState,
        _pos: BlockPos,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        Ok(if state.get(p::LIT).unwrap() { 15 } else { 0 })
    }
    fn signal(
        &self,
        state: BlockState,
        _pos: BlockPos,
        direction: Direction,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        let excluded = if state.block() == Block::REDSTONE_WALL_TORCH {
            facing(state)
        } else {
            Direction::Up
        };
        Ok(if direction != excluded && state.get(p::LIT).unwrap() {
            15
        } else {
            0
        })
    }
    fn direct_signal(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        if direction == Direction::Down {
            self.signal(state, pos, direction, level)
        } else {
            Ok(0)
        }
    }
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        _old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        notify_neighbors(level, pos, state)
    }
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        pos: BlockPos,
        moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if !moved {
            notify_neighbors(level, pos, state)?;
        }
        Ok(())
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        pos: BlockPos,
        _source: Block,
        _orientation: Option<Orientation>,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if state.get(p::LIT).unwrap() == neighbor_signal(level, pos, state)?
            && !level.will_tick_block_this_tick(pos, state.block())
        {
            level.schedule_block_tick_after(pos, state.block(), 2)?;
        }
        Ok(())
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        let signal = neighbor_signal(level, pos, state)?;
        let time = level.game_time();
        let toggles = &mut level.region_mut().redstone.torch_toggles;
        while toggles
            .front()
            .is_some_and(|(_, when)| time.wrapping_sub(*when) > 60)
        {
            toggles.pop_front();
        }
        if state.get(p::LIT).unwrap() {
            if signal {
                level.set_block(
                    pos,
                    state.with(p::LIT, false).unwrap(),
                    UpdateFlags::ALL,
                    UPDATE_LIMIT,
                )?;
                if toggled_too_frequently(level, pos, true) {
                    level.emit(LevelEffect::LevelEvent {
                        pos,
                        id: 1502,
                        data: 0,
                    });
                    level.schedule_block_tick_after(
                        pos,
                        level.get_block_state(pos)?.block(),
                        160,
                    )?;
                }
            }
        } else if !signal && !toggled_too_frequently(level, pos, false) {
            level.set_block(
                pos,
                state.with(p::LIT, true).unwrap(),
                UpdateFlags::ALL,
                UPDATE_LIMIT,
            )?;
        }
        Ok(())
    }
    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let direction = attached_direction(state);
        let attached = pos.relative(direction, 1);
        if direction == Direction::Down {
            level.can_support_center(attached, Direction::Up)
        } else {
            level.is_face_sturdy(
                level.get_block_state(attached)?,
                attached,
                direction.opposite(),
            )
        }
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
        Ok(
            if direction == attached_direction(state)
                && !self.can_survive(state, pos, &level.as_reader())?
            {
                Block::AIR.default_state()
            } else {
                state
            },
        )
    }
}
