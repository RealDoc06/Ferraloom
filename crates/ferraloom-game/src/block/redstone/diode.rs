//! Port of DiodeBlock and RepeaterBlock. Input capture and priorities follow
//! vanilla's scheduling points, including pulses whose input has already gone.
use super::{initial_orientation, state::facing};
use crate::{
    block::{
        BlockBehavior, BlockMethod, LevelAccessRef, LevelReaderRef, UPDATE_LIMIT, UpdateFlags,
    },
    level::{Level, LevelError, redstone::Orientation},
    ticks::TickPriority,
};
use ferraloom_data::{Block, BlockClass, BlockState, properties as p};
use ferraloom_util::{
    BlockPos, Direction,
    shapes::{self, BooleanOp},
};

pub struct RepeaterBlock;
pub static REPEATER: RepeaterBlock = RepeaterBlock;
pub(super) fn survives_on(
    reader: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
    state: BlockState,
) -> Result<bool, LevelError> {
    let shape = reader
        .required_behavior(state.block(), BlockMethod::IsFaceSturdy)?
        .support_shape(state, pos, reader)?;
    let rigid = shapes::join(
        &shapes::block(),
        &shapes::box_shape(2.0 / 16.0, 0.0, 2.0 / 16.0, 14.0 / 16.0, 1.0, 14.0 / 16.0),
        BooleanOp::OnlyFirst,
    );
    Ok(!shapes::join_is_not_empty(
        &shape.get_face_shape(Direction::Up),
        &rigid,
        BooleanOp::OnlySecond,
    ))
}
pub(super) fn alternate_signal(
    reader: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
    state: BlockState,
    diodes_only: bool,
) -> Result<i32, LevelError> {
    let facing = facing(state);
    let clockwise = facing.clockwise();
    let counter = facing.counter_clockwise();
    let first =
        reader.get_control_input_signal(pos.relative(clockwise, 1), clockwise, diodes_only)?;
    Ok(first.max(reader.get_control_input_signal(
        pos.relative(counter, 1),
        counter,
        diodes_only,
    )?))
}
pub(super) fn input_signal(
    level: &Level,
    pos: BlockPos,
    state: BlockState,
) -> Result<i32, LevelError> {
    let facing = facing(state);
    let target = pos.relative(facing, 1);
    let input = level.get_signal(target, facing)?;
    if input >= 15 {
        return Ok(input);
    }
    let target_state = level.get_block_state(target)?;
    Ok(input.max(if target_state.block() == Block::REDSTONE_WIRE {
        i32::from(target_state.get(p::POWER).unwrap())
    } else {
        0
    }))
}
pub(super) fn should_prioritize(
    reader: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
    state: BlockState,
) -> Result<bool, LevelError> {
    let direction = facing(state).opposite();
    let opposite = reader.get_block_state(pos.relative(direction, 1))?;
    Ok(opposite.block().is_instance_of(BlockClass::DiodeBlock) && facing(opposite) != direction)
}
pub(super) fn update_front(
    level: &mut Level,
    pos: BlockPos,
    state: BlockState,
) -> Result<(), LevelError> {
    let direction = facing(state);
    let opposite = pos.relative(direction.opposite(), 1);
    let orientation = initial_orientation(level, Some(direction.opposite()), Some(Direction::Up));
    level.neighbor_changed(opposite, state.block(), orientation)?;
    level.update_neighbors_at_except_from_facing(
        opposite,
        state.block(),
        Some(direction),
        orientation,
    )
}
pub(super) fn locked(
    reader: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
    state: BlockState,
) -> Result<bool, LevelError> {
    Ok(alternate_signal(reader, pos, state, true)? > 0)
}
fn check_tick(level: &mut Level, pos: BlockPos, state: BlockState) -> Result<(), LevelError> {
    if !locked(&LevelReaderRef::Server(level), pos, state)? {
        let on = state.get(p::POWERED).unwrap();
        let should_on = input_signal(level, pos, state)? > 0;
        if on != should_on && !level.will_tick_block_this_tick(pos, state.block()) {
            let priority = if should_prioritize(&LevelReaderRef::Server(level), pos, state)? {
                TickPriority::ExtremelyHigh
            } else if on {
                TickPriority::VeryHigh
            } else {
                TickPriority::High
            };
            let when = level
                .game_time()
                .wrapping_add(i64::from(state.get(p::DELAY).unwrap()) * 2);
            level.schedule_block_tick(pos, state.block(), when, priority)?;
        }
    }
    Ok(())
}
impl BlockBehavior for RepeaterBlock {
    fn set_placed_by(
        &self,
        state: BlockState,
        pos: BlockPos,
        _by: Option<crate::entity::EntityRef>,
        _stack: crate::block::StackView<'_>,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if input_signal(level, pos, state)? > 0 {
            level.schedule_block_tick_after(pos, state.block(), 1)?;
        }
        Ok(())
    }
    fn use_without_item(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &crate::block::UseContext,
        level: &mut Level,
    ) -> Result<crate::block::InteractionResult, LevelError> {
        use crate::{block::InteractionResult, entity::player::ServerPlayer};
        if !level.entity::<ServerPlayer>(context.player)?.may_build() {
            return Ok(InteractionResult::Pass);
        }
        level.set_block_and_update(
            pos,
            state
                .with(p::DELAY, state.get(p::DELAY).unwrap() % 4 + 1)
                .unwrap(),
        )?;
        Ok(InteractionResult::SUCCESS)
    }

    fn is_signal_source(&self, _state: BlockState) -> bool {
        true
    }
    fn own_signal(
        &self,
        state: BlockState,
        _pos: BlockPos,
        _reader: &LevelReaderRef<'_, '_>,
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
        pos: BlockPos,
        direction: Direction,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        if facing(state) == direction {
            self.own_signal(state, pos, reader)
        } else {
            Ok(0)
        }
    }
    fn direct_signal(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        self.signal(state, pos, direction, reader)
    }
    fn should_redstone_wire_connect_to(
        &self,
        state: BlockState,
        _pos: BlockPos,
        direction: Option<Direction>,
        _reader: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        Ok(direction.is_some_and(|d| d == facing(state) || d == facing(state).opposite()))
    }
    fn can_survive(
        &self,
        _state: BlockState,
        pos: BlockPos,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let below = pos.below(1);
        survives_on(reader, below, reader.get_block_state(below)?)
    }
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        _old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        update_front(level, pos, state)
    }
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        pos: BlockPos,
        moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if !moved {
            update_front(level, pos, state)?;
        }
        Ok(())
    }
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        neighbor_pos: BlockPos,
        neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        let reader = level.as_reader();
        if direction == Direction::Down && !survives_on(&reader, neighbor_pos, neighbor)? {
            return Ok(Block::AIR.default_state());
        }
        if direction.axis() != facing(state).axis() {
            Ok(state.with(p::LOCKED, locked(&reader, pos, state)?).unwrap())
        } else {
            Ok(state)
        }
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
        if level.get_block_state(pos)?.block() == state.block() {
            if self.can_survive(state, pos, &LevelReaderRef::Server(level))? {
                check_tick(level, pos, state)?;
            } else {
                let block_entity = if state.has_block_entity() {
                    level.get_or_promote_block_entity(pos)?
                } else {
                    None
                };
                crate::block::drops::drop_resources(state, pos, block_entity, level)?;
                level.remove_block(pos, false)?;
                for direction in Direction::VALUES {
                    level.update_neighbors_at(pos.relative(direction, 1), state.block())?;
                }
            }
        }
        Ok(())
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        if !locked(&LevelReaderRef::Server(level), pos, state)? {
            let on = state.get(p::POWERED).unwrap();
            let should_on = input_signal(level, pos, state)? > 0;
            if on && !should_on {
                level.set_block(
                    pos,
                    state.with(p::POWERED, false).unwrap(),
                    UpdateFlags::CLIENTS,
                    UPDATE_LIMIT,
                )?;
            } else if !on {
                level.set_block(
                    pos,
                    state.with(p::POWERED, true).unwrap(),
                    UpdateFlags::CLIENTS,
                    UPDATE_LIMIT,
                )?;
                if !should_on {
                    let when = level
                        .game_time()
                        .wrapping_add(i64::from(state.get(p::DELAY).unwrap()) * 2);
                    level.schedule_block_tick(pos, state.block(), when, TickPriority::VeryHigh)?;
                }
            }
        }
        Ok(())
    }
}
