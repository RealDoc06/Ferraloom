//! Port of SculkSensorBlock activation, phases and redstone output.
use crate::{
    block::{
        BlockBehavior, LevelAccessRef, LevelReaderRef, UpdateFlags,
        entity::{BlockEntityRef, sculk::SculkSensorBlockEntity},
    },
    entity::{EntityData, EntityRef},
    level::{Level, LevelError},
};
use ferraloom_data::{
    Block, BlockState, EntityType, properties as p,
    registries::{game_event::Id as GameEvent, sound_event::Id as Sound},
    tags::BlockTags,
};
use ferraloom_protocol::play::SoundSource;
use ferraloom_util::{BlockPos, Direction, Vec3, random::RandomSource};

pub(super) struct Sensor;
pub(super) static SENSOR: Sensor = Sensor;
pub(super) fn can_activate(state: BlockState) -> bool {
    state.get(p::SCULK_SENSOR_PHASE) == Some(p::SculkSensorPhase::Inactive)
}
fn update_neighbors(pos: BlockPos, block: Block, level: &mut Level) -> Result<(), LevelError> {
    level.update_neighbors_at(pos, block)?;
    level.update_neighbors_at(pos.below(1), block)
}
pub(super) fn receive(
    receiver: BlockEntityRef,
    event: GameEvent,
    source: Option<EntityRef>,
    distance: f32,
    level: &mut Level,
) -> Result<(), LevelError> {
    let state = level.block_entity_state(receiver)?.data().block_state;
    if !can_activate(state) {
        return Ok(());
    }
    let frequency = super::vibration::frequency(event);
    level
        .block_entity_mut::<SculkSensorBlockEntity>(receiver)?
        .last_vibration_frequency = frequency;
    let power = (15
        - (15.0 / f64::from(super::vibration::radius(receiver, level)?) * f64::from(distance))
            .floor() as i32)
        .max(1);
    // Native checks the retained receiver's state, then the concrete block type.
    if matches!(
        state.block(),
        Block::SCULK_SENSOR | Block::CALIBRATED_SCULK_SENSOR
    ) {
        activate(source, receiver.pos, state, power, frequency, level)?;
    }
    Ok(())
}
/// Port of SculkSensorBlock#activate, including synchronous resonance events.
fn activate(
    source: Option<EntityRef>,
    pos: BlockPos,
    state: BlockState,
    power: i32,
    frequency: i32,
    level: &mut Level,
) -> Result<(), LevelError> {
    LevelAccessRef::Server(level).set_block(
        pos,
        state
            .with(p::SCULK_SENSOR_PHASE, p::SculkSensorPhase::Active)
            .unwrap()
            .with(p::POWER, power as u8)
            .unwrap(),
        UpdateFlags::ALL,
    )?;
    level.schedule_block_tick_after(
        pos,
        state.block(),
        if state.block() == Block::CALIBRATED_SCULK_SENSOR {
            10
        } else {
            30
        },
    )?;
    update_neighbors(pos, state.block(), level)?;
    let tones = [0, 0, 2, 4, 6, 7, 9, 10, 12, 14, 15, 18, 19, 21, 22, 24];
    for direction in Direction::VALUES {
        let next = pos.relative(direction, 1);
        let neighbor = level.get_block_state(next)?;
        if level
            .block_tags()?
            .contains_key(BlockTags::VIBRATION_RESONATORS, neighbor.block())
        {
            level.game_event(
                super::vibration::resonance_events()[(frequency - 1) as usize],
                Vec3::at_center_of(next.into()),
                source,
                Some(neighbor),
            )?;
            let pitch = 2.0_f64.powf(f64::from(tones[frequency as usize] - 12) / 12.0) as f32;
            level.play_sound(
                None,
                Vec3::at_center_of(next.into()),
                Sound::BLOCK_AMETHYST_BLOCK_RESONATE,
                SoundSource::Blocks,
                1.0,
                pitch,
            )?;
        }
    }
    level.game_event(
        GameEvent::SCULK_SENSOR_TENDRILS_CLICKING,
        Vec3::at_center_of(pos.into()),
        source,
        None,
    )?;
    if state.get(p::WATERLOGGED) != Some(true) {
        let pitch = level.random().next_float() * 0.2 + 0.8;
        level.play_sound(
            None,
            Vec3::at_center_of(pos.into()),
            Sound::BLOCK_SCULK_SENSOR_CLICKING,
            SoundSource::Blocks,
            1.0,
            pitch,
        )?;
    }
    Ok(())
}
impl BlockBehavior for Sensor {
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if old.block() != state.block()
            && state.get(p::POWER).unwrap() > 0
            && !level.has_scheduled_block_tick(pos, state.block())
        {
            LevelAccessRef::Server(level).set_block(
                pos,
                state.with(p::POWER, 0).unwrap(),
                UpdateFlags(18),
            )?;
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
        if state.get(p::SCULK_SENSOR_PHASE) == Some(p::SculkSensorPhase::Active) {
            update_neighbors(pos, state.block(), level)?;
        }
        Ok(())
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        match state.get(p::SCULK_SENSOR_PHASE).unwrap() {
            p::SculkSensorPhase::Active => {
                LevelAccessRef::Server(level).set_block(
                    pos,
                    state
                        .with(p::SCULK_SENSOR_PHASE, p::SculkSensorPhase::Cooldown)
                        .unwrap()
                        .with(p::POWER, 0)
                        .unwrap(),
                    UpdateFlags::ALL,
                )?;
                level.schedule_block_tick_after(pos, state.block(), 10)?;
                update_neighbors(pos, state.block(), level)?;
            }
            p::SculkSensorPhase::Cooldown => {
                LevelAccessRef::Server(level).set_block(
                    pos,
                    state
                        .with(p::SCULK_SENSOR_PHASE, p::SculkSensorPhase::Inactive)
                        .unwrap(),
                    UpdateFlags::ALL,
                )?;
                if state.get(p::WATERLOGGED) != Some(true) {
                    let pitch = level.random().next_float() * 0.2 + 0.8;
                    level.play_sound(
                        None,
                        Vec3::at_center_of(pos.into()),
                        Sound::BLOCK_SCULK_SENSOR_CLICKING_STOP,
                        SoundSource::Blocks,
                        1.0,
                        pitch,
                    )?;
                }
            }
            p::SculkSensorPhase::Inactive => {}
        }
        Ok(())
    }
    fn step_on(
        &self,
        state: BlockState,
        pos: BlockPos,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if can_activate(state)
            && level.entity::<EntityData>(entity)?.entity_type != EntityType::WARDEN
            && let Some(receiver) = level.block_entity_ref(pos)?
            && level
                .block_entity::<SculkSensorBlockEntity>(receiver)
                .is_ok()
            && super::vibration::can_receive(
                receiver,
                pos,
                GameEvent::STEP,
                crate::level::game_event::GameEventContext {
                    source_entity: None,
                    affected_state: Some(state),
                },
                level,
            )?
        {
            let origin = level.entity::<EntityData>(entity)?.position;
            super::vibration::force_schedule(
                receiver,
                GameEvent::STEP,
                crate::level::game_event::GameEventContext {
                    source_entity: Some(entity),
                    affected_state: None,
                },
                origin,
                level,
            )?;
        }
        Ok(())
    }
    fn is_signal_source(&self, _state: BlockState) -> bool {
        true
    }
    fn has_analog_output_signal(&self, _state: BlockState) -> bool {
        true
    }
    fn own_signal(
        &self,
        state: BlockState,
        _pos: BlockPos,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        Ok(i32::from(state.get(p::POWER).unwrap()))
    }
    fn signal(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        if state.block() == Block::CALIBRATED_SCULK_SENSOR
            && direction == super::super::survival::facing(state)
        {
            Ok(0)
        } else {
            self.own_signal(state, pos, level)
        }
    }
    fn direct_signal(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        if direction == Direction::Up {
            self.signal(state, pos, direction, level)
        } else {
            Ok(0)
        }
    }
    fn analog_output_signal(
        &self,
        state: BlockState,
        pos: BlockPos,
        _direction: Direction,
        level: &mut Level,
    ) -> Result<i32, LevelError> {
        if let Some(receiver) = level.block_entity_ref(pos)?
            && let Ok(sensor) = level.block_entity::<SculkSensorBlockEntity>(receiver)
        {
            return Ok(
                if state.get(p::SCULK_SENSOR_PHASE) == Some(p::SculkSensorPhase::Active) {
                    sensor.last_vibration_frequency
                } else {
                    0
                },
            );
        }
        Ok(0)
    }
}
