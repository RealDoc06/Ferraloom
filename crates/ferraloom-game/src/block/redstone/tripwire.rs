//! Port of TripWireBlock and TripWireHookBlock. Hook scans capture wire states
//! before receiver updates, retaining native reentrant replacement behavior.
use super::{
    initial_orientation,
    state::{facing, with_facing},
};
use crate::{
    block::{BlockBehavior, LevelAccessRef, LevelReaderRef},
    entity::EntityRef,
    level::{Level, LevelError},
};
use ferraloom_data::{
    Block, BlockState, properties as p,
    registries::{game_event::Id as GameEvent, sound_event::Id as SoundEvent},
};
use ferraloom_protocol::play::SoundSource;
use ferraloom_util::{Axis, BlockPos, Direction, Vec3, VoxelShape, random::RandomSource};
pub struct TripWireBlock;
pub struct TripWireHookBlock;
pub static WIRE: TripWireBlock = TripWireBlock;
pub static HOOK: TripWireHookBlock = TripWireHookBlock;

fn notify(
    level: &mut Level,
    pos: BlockPos,
    block: Block,
    direction: Direction,
) -> Result<(), LevelError> {
    let front = direction.opposite();
    let orientation = initial_orientation(level, Some(front), Some(Direction::Up));
    level.update_neighbors_at_except_from_facing(pos, block, None, orientation)?;
    level.update_neighbors_at_except_from_facing(pos.relative(front, 1), block, None, orientation)
}
fn emit_state(
    level: &mut Level,
    pos: BlockPos,
    attached: bool,
    powered: bool,
    was_attached: bool,
    was_powered: bool,
) -> Result<(), LevelError> {
    let transition = if powered && !was_powered {
        Some((
            SoundEvent::BLOCK_TRIPWIRE_CLICK_ON,
            0.6,
            GameEvent::BLOCK_ACTIVATE,
        ))
    } else if !powered && was_powered {
        Some((
            SoundEvent::BLOCK_TRIPWIRE_CLICK_OFF,
            0.5,
            GameEvent::BLOCK_DEACTIVATE,
        ))
    } else if attached && !was_attached {
        Some((
            SoundEvent::BLOCK_TRIPWIRE_ATTACH,
            0.7,
            GameEvent::BLOCK_ATTACH,
        ))
    } else if !attached && was_attached {
        Some((
            SoundEvent::BLOCK_TRIPWIRE_DETACH,
            1.2 / (level.random().next_float() * 0.2 + 0.9),
            GameEvent::BLOCK_DETACH,
        ))
    } else {
        None
    };
    if let Some((sound, pitch, event)) = transition {
        let center = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        );
        level.play_sound(None, center, sound, SoundSource::Blocks, 0.4, pitch)?;
        level.game_event(event, center, None, None);
    }
    Ok(())
}
fn on_removed(level: &mut Level, pos: BlockPos, state: BlockState) -> Result<(), LevelError> {
    if state.get(p::ATTACHED).unwrap() || state.get(p::POWERED).unwrap() {
        calculate_state(level, pos, state, true, false, None)?;
    }
    if state.get(p::POWERED).unwrap() {
        notify(level, pos, state.block(), facing(state))?;
    }
    Ok(())
}
/// Port of TripWireHookBlock#calculateState. A source entry replaces that
/// position's live state and schedules the hook even when the wire is gone.
pub fn calculate_state(
    level: &mut Level,
    pos: BlockPos,
    state: BlockState,
    destroyed: bool,
    can_update: bool,
    source: Option<(usize, Option<BlockState>)>,
) -> Result<(), LevelError> {
    if state.get(p::FACING).is_none() {
        return Ok(());
    }
    let direction = facing(state);
    let was_attached = state.get(p::ATTACHED).unwrap_or(false);
    let was_powered = state.get(p::POWERED).unwrap_or(false);
    let mut attached = !destroyed;
    let mut powered = false;
    let mut receiver = 0;
    let mut wires = [None; 42];
    for (i, entry) in wires.iter_mut().enumerate().skip(1) {
        let mut wire = level.get_block_state(pos.relative(direction, i as i32))?;
        if wire.block() == Block::TRIPWIRE_HOOK {
            if facing(wire) == direction.opposite() {
                receiver = i;
            }
            break;
        }
        let source_state = source.filter(|(index, _)| *index == i);
        if wire.block() != Block::TRIPWIRE && source_state.is_none() {
            attached = false;
            continue;
        }
        if let Some((_, Some(replacement))) = source_state {
            wire = replacement;
        }
        let armed = !wire.get(p::DISARMED).unwrap();
        powered |= armed && wire.get(p::POWERED).unwrap();
        *entry = Some(wire);
        if source_state.is_some() {
            level.schedule_block_tick_after(pos, state.block(), 10)?;
            attached &= armed;
        }
    }
    attached &= receiver > 1;
    powered &= attached;
    let new_state = state.block().default_state();
    let new_state = new_state.with(p::ATTACHED, attached).unwrap_or(new_state);
    let new_state = new_state.with(p::POWERED, powered).unwrap_or(new_state);
    if receiver > 0 {
        let end = pos.relative(direction, receiver as i32);
        level.set_block_and_update(end, with_facing(new_state, direction.opposite()))?;
        notify(level, end, state.block(), direction.opposite())?;
        if level.get_block_state(pos)?.block() != Block::TRIPWIRE_HOOK {
            on_removed(level, pos, new_state)?;
            return Ok(());
        }
        emit_state(level, end, attached, powered, was_attached, was_powered)?;
    }
    emit_state(level, pos, attached, powered, was_attached, was_powered)?;
    if !destroyed {
        level.set_block_and_update(pos, with_facing(new_state, direction))?;
        if can_update {
            notify(level, pos, state.block(), direction)?;
        }
    }
    if was_attached != attached {
        for (i, &wire) in wires.iter().enumerate().take(receiver).skip(1) {
            if let Some(wire) = wire {
                let target = pos.relative(direction, i as i32);
                if matches!(
                    level.get_block_state(target)?.block(),
                    Block::TRIPWIRE | Block::TRIPWIRE_HOOK
                ) {
                    level.set_block_and_update(
                        target,
                        wire.with(p::ATTACHED, attached).unwrap_or(wire),
                    )?;
                }
            }
        }
    }
    Ok(())
}
fn update_source(level: &mut Level, pos: BlockPos, state: BlockState) -> Result<(), LevelError> {
    for direction in [Direction::South, Direction::West] {
        for i in 1..42 {
            let target = pos.relative(direction, i);
            let target_state = level.get_block_state(target)?;
            if target_state.block() == Block::TRIPWIRE_HOOK {
                if facing(target_state) == direction.opposite() {
                    calculate_state(
                        level,
                        target,
                        target_state,
                        false,
                        true,
                        Some((i as usize, Some(state))),
                    )?;
                }
                break;
            }
            if target_state.block() != Block::TRIPWIRE {
                break;
            }
        }
    }
    Ok(())
}
pub(super) fn with_connection(
    state: BlockState,
    direction: Direction,
    neighbor: BlockState,
) -> BlockState {
    let connect = if neighbor.block() == Block::TRIPWIRE_HOOK {
        facing(neighbor) == direction.opposite()
    } else {
        neighbor.block() == Block::TRIPWIRE
    };
    macro_rules! property {
        ($key:ident,$ty:ident) => {
            state
                .with(p::$key, if connect { p::$ty::True } else { p::$ty::False })
                .unwrap()
        };
    }
    match direction {
        Direction::North => property!(NORTH, North),
        Direction::East => property!(EAST, East),
        Direction::South => property!(SOUTH, South),
        Direction::West => property!(WEST, West),
        _ => state,
    }
}
fn check_pressed(
    level: &mut Level,
    pos: BlockPos,
    candidates: Vec<EntityRef>,
) -> Result<(), LevelError> {
    let state = level.get_block_state(pos)?;
    let was_pressed = state.get(p::POWERED).unwrap();
    let mut pressed = false;
    for entity in candidates {
        if !level
            .entity_behavior(entity)?
            .is_ignoring_block_triggers(entity, level)?
        {
            pressed = true;
            break;
        }
    }
    if pressed != was_pressed {
        let changed = state.with(p::POWERED, pressed).unwrap();
        level.set_block_and_update(pos, changed)?;
        update_source(level, pos, changed)?;
    }
    if pressed {
        level.schedule_block_tick_after(pos, state.block(), 10)?;
    } else if was_pressed {
        level.schedule_block_tick_after(pos, state.block(), 1)?;
    }
    Ok(())
}
impl BlockBehavior for TripWireBlock {
    fn player_will_destroy(
        &self,
        state: BlockState,
        pos: BlockPos,
        player: EntityRef,
        level: &mut Level,
    ) -> Result<BlockState, LevelError> {
        let held = level
            .entity::<crate::entity::player::ServerPlayer>(player)?
            .inventory
            .selected_item();
        if held.item() == ferraloom_data::Item::SHEARS {
            level.set_block(
                pos,
                state.with(p::DISARMED, true).unwrap(),
                crate::block::UpdateFlags::NONE,
                crate::block::UPDATE_LIMIT,
            )?;
            level.game_event(
                GameEvent::SHEAR,
                Vec3::new(
                    f64::from(pos.x) + 0.5,
                    f64::from(pos.y) + 0.5,
                    f64::from(pos.z) + 0.5,
                ),
                Some(player),
                None,
            );
        }
        crate::block::BASE_BLOCK.player_will_destroy(state, pos, player, level)
    }
    fn update_shape(
        &self,
        state: BlockState,
        _pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor: BlockState,
        _level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        Ok(with_connection(state, direction, neighbor))
    }
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if state.block() != old.block() {
            update_source(level, pos, state)?;
        }
        Ok(())
    }
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        pos: BlockPos,
        moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if !moved {
            update_source(level, pos, state.with(p::POWERED, true).unwrap())?;
        }
        Ok(())
    }
    fn entity_inside_collision_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        _entity: EntityRef,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        level.shape(state, pos)
    }
    fn entity_inside(
        &self,
        state: BlockState,
        pos: BlockPos,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if !state.get(p::POWERED).unwrap() && !level.has_scheduled_block_tick(pos, state.block()) {
            check_pressed(level, pos, vec![entity])?;
        }
        Ok(())
    }
    fn tick(&self, _state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        let state = level.get_block_state(pos)?;
        if state.get(p::POWERED).unwrap() {
            let bounds = LevelReaderRef::Server(level)
                .shape(state, pos)?
                .bounds()
                .move_by(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
            let mut entities = Vec::new();
            for entity in level.entities_in(bounds, |_, _| true) {
                if !level.entity_behavior(entity)?.is_spectator(entity, level)? {
                    entities.push(entity);
                }
            }
            check_pressed(level, pos, entities)?;
        }
        Ok(())
    }
}
impl BlockBehavior for TripWireHookBlock {
    fn set_placed_by(
        &self,
        state: BlockState,
        pos: BlockPos,
        _by: Option<EntityRef>,
        _stack: crate::block::StackView<'_>,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        calculate_state(level, pos, state, false, false, None)
    }
    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let direction = facing(state);
        let support = pos.relative(direction.opposite(), 1);
        Ok(direction.axis() != Axis::Y
            && level.is_face_sturdy(level.get_block_state(support)?, support, direction)?)
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
            if direction.opposite() == facing(state)
                && !self.can_survive(state, pos, &level.as_reader())?
            {
                Block::AIR.default_state()
            } else {
                state
            },
        )
    }
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        pos: BlockPos,
        moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if !moved {
            on_removed(level, pos, state)?;
        }
        Ok(())
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        calculate_state(level, pos, state, false, true, None)
    }
}
