//! LeverBlock, ButtonBlock and the two pressure-plate classes.
use super::{diode, initial_orientation, sources::connected_direction, state::facing};
use crate::{
    block::{
        BlockBehavior, InteractionResult, LevelAccessRef, LevelReaderRef, UPDATE_LIMIT,
        UpdateFlags, UseContext,
    },
    entity::EntityRef,
    level::{Level, LevelError},
};
use ferraloom_data::{
    Block, BlockClass, BlockState, EntityType, properties as p,
    registries::{game_event::Id as GameEvent, sound_event::Id as SoundEvent},
};
use ferraloom_protocol::play::SoundSource;
use ferraloom_util::{Aabb, BlockPos, Direction, Vec3};

pub struct Switches;
pub static SWITCHES: Switches = Switches;
fn center(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    )
}
fn notify_switch(level: &mut Level, pos: BlockPos, state: BlockState) -> Result<(), LevelError> {
    let front = connected_direction(state).opposite();
    let up = if front.axis() == ferraloom_util::Axis::Y {
        facing(state)
    } else {
        Direction::Up
    };
    let orientation = initial_orientation(level, Some(front), Some(up));
    level.update_neighbors_at_except_from_facing(pos, state.block(), None, orientation)?;
    level.update_neighbors_at_except_from_facing(
        pos.relative(front, 1),
        state.block(),
        None,
        orientation,
    )
}
fn sound(
    level: &mut Level,
    pos: BlockPos,
    sound: SoundEvent,
    except: Option<EntityRef>,
    volume: f32,
    pitch: f32,
) -> Result<(), LevelError> {
    level.play_sound(
        except,
        center(pos),
        sound,
        SoundSource::Blocks,
        volume,
        pitch,
    )
}
fn event(level: &mut Level, pos: BlockPos, on: bool, source: Option<EntityRef>) {
    level.game_event(
        if on {
            GameEvent::BLOCK_ACTIVATE
        } else {
            GameEvent::BLOCK_DEACTIVATE
        },
        center(pos),
        source,
        None,
    )?;
}
fn button_pressed(level: &mut Level, pos: BlockPos, state: BlockState) -> Result<(), LevelError> {
    let facts = state.block().redstone_properties();
    let set = facts.block_set.unwrap();
    let arrow = if set.arrows_press_button {
        let bounds = LevelReaderRef::Server(level)
            .shape(state, pos)?
            .bounds()
            .move_by(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
        level
            .entities_in(bounds, |_, entity| {
                matches!(
                    entity.entity_type(),
                    EntityType::ARROW | EntityType::SPECTRAL_ARROW | EntityType::TRIDENT
                )
            })
            .first()
            .copied()
    } else {
        None
    };
    let pressed = arrow.is_some();
    let was_pressed = state.get(p::POWERED).unwrap();
    if pressed != was_pressed {
        level.set_block(
            pos,
            state.with(p::POWERED, pressed).unwrap(),
            UpdateFlags::ALL,
            UPDATE_LIMIT,
        )?;
        notify_switch(level, pos, state)?;
        sound(
            level,
            pos,
            if pressed {
                set.button_on
            } else {
                set.button_off
            },
            None,
            1.0,
            1.0,
        )?;
        event(level, pos, pressed, arrow);
    }
    if pressed {
        level.schedule_block_tick_after(pos, state.block(), facts.button_ticks)?;
    }
    Ok(())
}
fn plate_signal(state: BlockState) -> i32 {
    if state.block().class() == BlockClass::WeightedPressurePlateBlock {
        i32::from(state.get(p::POWER).unwrap())
    } else if state.get(p::POWERED).unwrap() {
        15
    } else {
        0
    }
}
fn check_plate(
    level: &mut Level,
    pos: BlockPos,
    state: BlockState,
    old: i32,
    source: Option<EntityRef>,
) -> Result<(), LevelError> {
    let facts = state.block().redstone_properties();
    let set = facts.block_set.unwrap();
    let bounds = Aabb::new(1.0 / 16.0, 0.0, 1.0 / 16.0, 15.0 / 16.0, 0.25, 15.0 / 16.0).move_by(
        f64::from(pos.x),
        f64::from(pos.y),
        f64::from(pos.z),
    );
    let candidates = level.entities_in(bounds, |_, state| {
        !set.mobs_press_plate || state.as_living().is_some()
    });
    let mut count = 0;
    for entity in candidates {
        let behavior = level.entity_behavior(entity)?;
        if !behavior.is_spectator(entity, level)?
            && !behavior.is_ignoring_block_triggers(entity, level)?
        {
            count += 1;
        }
    }
    let weighted = facts.max_weight > 0;
    let signal = if weighted {
        let count = count.min(facts.max_weight);
        if count > 0 {
            ((count as f32 / facts.max_weight as f32) * 15.0).ceil() as i32
        } else {
            0
        }
    } else if count > 0 {
        15
    } else {
        0
    };
    let pressed = signal > 0;
    let was_pressed = old > 0;
    if old != signal {
        let new = if weighted {
            state.with(p::POWER, signal as u8).unwrap()
        } else {
            state.with(p::POWERED, pressed).unwrap()
        };
        level.set_block(pos, new, UpdateFlags::CLIENTS, UPDATE_LIMIT)?;
        level.update_neighbors_at(pos, state.block())?;
        level.update_neighbors_at(pos.below(1), state.block())?;
    }
    if pressed != was_pressed {
        sound(
            level,
            pos,
            if pressed { set.plate_on } else { set.plate_off },
            None,
            1.0,
            1.0,
        )?;
        event(level, pos, pressed, source);
    }
    if pressed {
        level.schedule_block_tick_after(pos, state.block(), if weighted { 10 } else { 20 })?;
    }
    Ok(())
}
impl BlockBehavior for Switches {
    fn use_without_item(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &UseContext,
        level: &mut Level,
    ) -> Result<InteractionResult, LevelError> {
        if state.block().class() == BlockClass::LeverBlock {
            let on = !state.get(p::POWERED).unwrap();
            let state = state.with(p::POWERED, on).unwrap();
            level.set_block(pos, state, UpdateFlags::ALL, UPDATE_LIMIT)?;
            notify_switch(level, pos, state)?;
            // LeverBlock#useWithoutItem calls pull with a null player on server.
            sound(
                level,
                pos,
                SoundEvent::BLOCK_LEVER_CLICK,
                None,
                0.3,
                if on { 0.6 } else { 0.5 },
            )?;
            event(level, pos, on, None);
            Ok(InteractionResult::SUCCESS)
        } else {
            if state.get(p::POWERED).unwrap() {
                return Ok(InteractionResult::CONSUME);
            }
            let facts = state.block().redstone_properties();
            level.set_block(
                pos,
                state.with(p::POWERED, true).unwrap(),
                UpdateFlags::ALL,
                UPDATE_LIMIT,
            )?;
            notify_switch(level, pos, state)?;
            level.schedule_block_tick_after(pos, state.block(), facts.button_ticks)?;
            sound(
                level,
                pos,
                facts.block_set.unwrap().button_on,
                Some(context.player),
                1.0,
                1.0,
            )?;
            event(level, pos, true, Some(context.player));
            Ok(InteractionResult::SUCCESS)
        }
    }
    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        if matches!(
            state.block().class(),
            BlockClass::PressurePlateBlock | BlockClass::WeightedPressurePlateBlock
        ) {
            let below = pos.below(1);
            Ok(
                diode::survives_on(reader, below, reader.get_block_state(below)?)?
                    || reader.can_support_center(below, Direction::Up)?,
            )
        } else {
            let direction = connected_direction(state).opposite();
            let target = pos.relative(direction, 1);
            reader.is_face_sturdy(
                reader.get_block_state(target)?,
                target,
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
        let attached = if matches!(
            state.block().class(),
            BlockClass::PressurePlateBlock | BlockClass::WeightedPressurePlateBlock
        ) {
            Direction::Down
        } else {
            connected_direction(state).opposite()
        };
        Ok(
            if direction == attached && !self.can_survive(state, pos, &level.as_reader())? {
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
            if matches!(
                state.block().class(),
                BlockClass::PressurePlateBlock | BlockClass::WeightedPressurePlateBlock
            ) {
                if plate_signal(state) > 0 {
                    level.update_neighbors_at(pos, state.block())?;
                    level.update_neighbors_at(pos.below(1), state.block())?;
                }
            } else if state.get(p::POWERED).unwrap() {
                notify_switch(level, pos, state)?;
            }
        }
        Ok(())
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        if state.block().class() == BlockClass::ButtonBlock {
            if state.get(p::POWERED).unwrap() {
                button_pressed(level, pos, state)?;
            }
        } else {
            let signal = plate_signal(state);
            if signal > 0 {
                check_plate(level, pos, state, signal, None)?;
            }
        }
        Ok(())
    }
    fn entity_inside(
        &self,
        state: BlockState,
        pos: BlockPos,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if state.block().class() == BlockClass::ButtonBlock {
            if state
                .block()
                .redstone_properties()
                .block_set
                .unwrap()
                .arrows_press_button
                && !state.get(p::POWERED).unwrap()
            {
                button_pressed(level, pos, state)?;
            }
        } else if plate_signal(state) == 0 {
            check_plate(level, pos, state, 0, Some(entity))?;
        }
        Ok(())
    }
}
