//! Port of VibrationSystem.Listener, User and Ticker on canonical BE generations.
use crate::{
    block::entity::{
        BlockEntityRef, BlockEntityState,
        sculk::{VibrationData, VibrationEvent, VibrationInfo},
    },
    entity::{EntityData, EntityRef, EntityState, player::ServerPlayer},
    level::{
        Level, LevelError,
        clip::ClipBlockStateContext,
        game_event::{GameEventContext, GameEventListener, GameEventListenerRef, PositionSource},
    },
};
use ferraloom_data::{
    Block, properties as p,
    registries::game_event::Id as GameEvent,
    tags::{BlockTags, GameEventTags},
};
use ferraloom_protocol::{
    codec::VarInt,
    entity_data::{Particle, PositionSource as ParticleSource, Vibration},
};
use ferraloom_util::{BlockPos, ChunkPos, Direction, Vec3};

pub(crate) struct Listener;
pub(crate) static LISTENER: Listener = Listener;

fn receiver(listener: GameEventListenerRef) -> BlockEntityRef {
    match listener {
        GameEventListenerRef::BlockEntity(receiver) => receiver,
        #[cfg(test)]
        GameEventListenerRef::Test(_) => unreachable!("concrete vibration listener"),
    }
}
pub(crate) fn data(receiver: BlockEntityRef, level: &Level) -> Result<&VibrationData, LevelError> {
    match level.block_entity_state(receiver)? {
        BlockEntityState::SculkSensor(value) => Ok(&value.vibration_data),
        BlockEntityState::CalibratedSculkSensor(value) => Ok(&value.parent.vibration_data),
        BlockEntityState::SculkShrieker(value) => Ok(&value.vibration_data),
        other => Err(LevelError::UnsupportedBlockEntity(
            other.data().block_state.block(),
        )),
    }
}
fn data_mut(receiver: BlockEntityRef, level: &mut Level) -> Result<&mut VibrationData, LevelError> {
    match level.block_entity_state_mut(receiver)? {
        BlockEntityState::SculkSensor(value) => Ok(&mut value.vibration_data),
        BlockEntityState::CalibratedSculkSensor(value) => Ok(&mut value.parent.vibration_data),
        BlockEntityState::SculkShrieker(value) => Ok(&mut value.vibration_data),
        other => Err(LevelError::UnsupportedBlockEntity(
            other.data().block_state.block(),
        )),
    }
}
pub(crate) fn radius(receiver: BlockEntityRef, level: &Level) -> Result<i32, LevelError> {
    Ok(
        if matches!(
            level.block_entity_state(receiver)?,
            BlockEntityState::CalibratedSculkSensor(_)
        ) {
            16
        } else {
            8
        },
    )
}
fn is_shrieker(receiver: BlockEntityRef, level: &Level) -> Result<bool, LevelError> {
    Ok(matches!(
        level.block_entity_state(receiver)?,
        BlockEntityState::SculkShrieker(_)
    ))
}
impl GameEventListener for Listener {
    fn get_listener_source(
        &self,
        listener: GameEventListenerRef,
        _level: &Level,
    ) -> Result<PositionSource, LevelError> {
        Ok(PositionSource::Block(receiver(listener).pos))
    }
    fn get_listener_radius(
        &self,
        listener: GameEventListenerRef,
        level: &Level,
    ) -> Result<i32, LevelError> {
        radius(receiver(listener), level)
    }
    fn handle_game_event(
        &self,
        listener: GameEventListenerRef,
        level: &mut Level,
        event: GameEvent,
        context: GameEventContext,
        origin: Vec3,
    ) -> Result<bool, LevelError> {
        let receiver = receiver(listener);
        if data(receiver, level)?.current_vibration.is_some()
            || !is_valid(receiver, event, context, level)?
        {
            return Ok(false);
        }
        if !can_receive(
            receiver,
            BlockPos::containing(origin.x, origin.y, origin.z),
            event,
            context,
            level,
        )? {
            return Ok(false);
        }
        let destination = Vec3::at_center_of(receiver.pos.into());
        if is_occluded(origin, destination, level)? {
            return Ok(false);
        }
        force_schedule(receiver, event, context, origin, level)?;
        Ok(true)
    }
}
/// Port of VibrationSystem#getGameEventFrequency. Unmapped events have frequency zero.
pub(crate) fn frequency(event: GameEvent) -> i32 {
    match event {
        GameEvent::STEP | GameEvent::SWIM | GameEvent::FLAP => 1,
        GameEvent::PROJECTILE_LAND
        | GameEvent::HIT_GROUND
        | GameEvent::SPLASH
        | GameEvent::BOUNCE => 2,
        GameEvent::ITEM_INTERACT_FINISH
        | GameEvent::PROJECTILE_SHOOT
        | GameEvent::INSTRUMENT_PLAY => 3,
        GameEvent::ENTITY_ACTION | GameEvent::ELYTRA_GLIDE | GameEvent::UNEQUIP => 4,
        GameEvent::ENTITY_DISMOUNT | GameEvent::EQUIP => 5,
        GameEvent::ENTITY_INTERACT | GameEvent::SHEAR | GameEvent::ENTITY_MOUNT => 6,
        GameEvent::ENTITY_DAMAGE => 7,
        GameEvent::DRINK | GameEvent::EAT => 8,
        GameEvent::CONTAINER_CLOSE
        | GameEvent::BLOCK_CLOSE
        | GameEvent::BLOCK_DEACTIVATE
        | GameEvent::BLOCK_DETACH => 9,
        GameEvent::CONTAINER_OPEN
        | GameEvent::BLOCK_OPEN
        | GameEvent::BLOCK_ACTIVATE
        | GameEvent::BLOCK_ATTACH
        | GameEvent::PRIME_FUSE
        | GameEvent::NOTE_BLOCK_PLAY => 10,
        GameEvent::BLOCK_CHANGE => 11,
        GameEvent::BLOCK_DESTROY | GameEvent::FLUID_PICKUP => 12,
        GameEvent::BLOCK_PLACE | GameEvent::FLUID_PLACE => 13,
        GameEvent::ENTITY_PLACE | GameEvent::LIGHTNING_STRIKE | GameEvent::TELEPORT => 14,
        GameEvent::ENTITY_DIE | GameEvent::EXPLODE => 15,
        _ => resonance_events()
            .iter()
            .position(|&value| value == event)
            .map_or(0, |index| index as i32 + 1),
    }
}
pub(super) fn resonance_events() -> [GameEvent; 15] {
    [
        GameEvent::RESONATE_1,
        GameEvent::RESONATE_2,
        GameEvent::RESONATE_3,
        GameEvent::RESONATE_4,
        GameEvent::RESONATE_5,
        GameEvent::RESONATE_6,
        GameEvent::RESONATE_7,
        GameEvent::RESONATE_8,
        GameEvent::RESONATE_9,
        GameEvent::RESONATE_10,
        GameEvent::RESONATE_11,
        GameEvent::RESONATE_12,
        GameEvent::RESONATE_13,
        GameEvent::RESONATE_14,
        GameEvent::RESONATE_15,
    ]
}
fn is_valid(
    receiver: BlockEntityRef,
    event: GameEvent,
    context: GameEventContext,
    level: &mut Level,
) -> Result<bool, LevelError> {
    let shrieker = is_shrieker(receiver, level)?;
    let tags = level.context().game_event_tags()?;
    if !tags.contains_key(
        if shrieker {
            GameEventTags::SHRIEKER_CAN_LISTEN
        } else {
            GameEventTags::VIBRATIONS
        },
        event,
    ) {
        return Ok(false);
    }
    if let Some(source) = context.source_entity {
        let behavior = level.entity_behavior(source)?;
        if behavior.is_spectator(source, level)? {
            return Ok(false);
        }
        if behavior.is_stepping_carefully(source, level)?
            && tags.contains_key(GameEventTags::IGNORE_VIBRATIONS_SNEAKING, event)
        {
            if !shrieker && matches!(level.entity_state(source)?, EntityState::Player(_)) {
                crate::advancements::criteria::avoid_vibration(level, source)?;
            }
            return Ok(false);
        }
        if behavior.dampens_vibrations(source, level)? {
            return Ok(false);
        }
    }
    Ok(match context.affected_state {
        Some(state) => !level
            .block_tags()?
            .contains_key(BlockTags::DAMPENS_VIBRATIONS, state.block()),
        None => true,
    })
}
pub(crate) fn can_receive(
    receiver: BlockEntityRef,
    origin: BlockPos,
    event: GameEvent,
    context: GameEventContext,
    level: &mut Level,
) -> Result<bool, LevelError> {
    let state = level.block_entity_state(receiver)?.data().block_state;
    if is_shrieker(receiver, level)? {
        return Ok(state.get(p::SHRIEKING) != Some(true)
            && try_get_player(context.source_entity, level)?.is_some());
    }
    if state.block() == Block::CALIBRATED_SCULK_SENSOR {
        let direction = super::super::survival::facing(state).opposite();
        let signal = level.get_signal(receiver.pos.relative(direction, 1), direction)?;
        if signal != 0 && frequency(event) != signal {
            return Ok(false);
        }
    }
    Ok(!(origin == receiver.pos
        && matches!(event, GameEvent::BLOCK_DESTROY | GameEvent::BLOCK_PLACE))
        && frequency(event) != 0
        && super::sensor::can_activate(state))
}
/// Port of SculkShriekerBlockEntity#tryGetPlayer.
fn try_get_player(
    source: Option<EntityRef>,
    level: &mut Level,
) -> Result<Option<EntityRef>, LevelError> {
    let Some(source) = source else {
        return Ok(None);
    };
    if matches!(level.entity_state(source)?, EntityState::Player(_)) {
        return Ok(Some(source));
    }
    if let Some(passenger) = level
        .entity_behavior(source)?
        .controlling_passenger(source, level)?
        && matches!(level.entity_state(passenger)?, EntityState::Player(_))
    {
        return Ok(Some(passenger));
    }
    if level.entity_state(source)?.as_projectile().is_some()
        || matches!(level.entity_state(source)?, EntityState::Item(_))
    {
        return Ok(level
            .entity_behavior(source)?
            .get_owner(source, level)?
            .filter(|&owner| level.entity::<ServerPlayer>(owner).is_ok()));
    }
    Ok(None)
}
/// Port of VibrationSelector#addCandidate, preserving the first candidate on
/// other ticks, nearer-first and higher-frequency-at-equal-distance selection.
pub(crate) fn add_candidate(data: &mut VibrationData, info: VibrationInfo, time: i64) -> bool {
    let replace = data
        .selection_strategy
        .current_vibration_data
        .as_ref()
        .is_none_or(|previous| {
            previous.tick == time
                && (info.distance < previous.event.distance
                    || info.distance.partial_cmp(&previous.event.distance)
                        != Some(std::cmp::Ordering::Greater)
                        && frequency(info.game_event) > frequency(previous.event.game_event))
        });
    if replace {
        data.selection_strategy.current_vibration_data = Some(VibrationEvent {
            event: info,
            tick: time,
        });
    }
    replace
}
pub(crate) fn force_schedule(
    receiver: BlockEntityRef,
    event: GameEvent,
    context: GameEventContext,
    origin: Vec3,
    level: &mut Level,
) -> Result<(), LevelError> {
    let uuid = context
        .source_entity
        .map(|source| level.entity::<EntityData>(source).map(|data| data.uuid))
        .transpose()?;
    let projectile_owner_uuid = if let Some(source) = context.source_entity
        && level.entity_state(source)?.as_projectile().is_some()
    {
        level
            .entity_behavior(source)?
            .get_owner(source, level)?
            .map(|owner| level.entity::<EntityData>(owner).map(|data| data.uuid))
            .transpose()?
    } else {
        None
    };
    let info = VibrationInfo {
        game_event: event,
        distance: origin
            .distance_to_sqr(Vec3::at_center_of(receiver.pos.into()))
            .sqrt() as f32,
        pos: origin,
        uuid,
        projectile_owner_uuid,
        entity: context.source_entity,
    };
    let time = level.game_time();
    let previous = data(receiver, level)?
        .selection_strategy
        .current_vibration_data
        .as_ref()
        .and_then(|candidate| candidate.event.entity);
    if add_candidate(data_mut(receiver, level)?, info, time) {
        if let Some(source) = context.source_entity {
            level.retain_entity(source)?;
        }
        if let Some(source) = previous {
            level.release_entity(source);
        }
    }
    Ok(())
}
fn is_occluded(origin: Vec3, destination: Vec3, level: &Level) -> Result<bool, LevelError> {
    let source = BlockPos::containing(origin.x, origin.y, origin.z);
    let target = BlockPos::containing(destination.x, destination.y, destination.z);
    let from = Vec3::at_center_of(source.into());
    let to = Vec3::at_center_of(target.into());
    for direction in Direction::VALUES {
        let step = direction.step();
        let (x, y, z) = (step.x, step.y, step.z);
        let from = Vec3::new(
            from.x + f64::from(x) * 9.999999747378752e-6,
            from.y + f64::from(y) * 9.999999747378752e-6,
            from.z + f64::from(z) * 9.999999747378752e-6,
        );
        let tags = level.block_tags()?;
        let result = level.is_block_in_line(&ClipBlockStateContext::new(
            from,
            to,
            |state: ferraloom_data::BlockState| {
                tags.contains_key(BlockTags::OCCLUDES_VIBRATION_SIGNALS, state.block())
            },
        ))?;
        if result.miss {
            return Ok(false);
        }
    }
    Ok(true)
}
fn travel_time(distance: f32) -> i32 {
    distance.floor() as i32
}
fn particle(
    receiver: BlockEntityRef,
    origin: Vec3,
    delay: i32,
    level: &mut Level,
) -> Result<i32, LevelError> {
    level.send_particles(
        Particle::Vibration(Vibration {
            destination: ParticleSource::Block(receiver.pos),
            arrival_in_ticks: VarInt(delay),
        }),
        origin,
        1,
        Vec3::new(0.0, 0.0, 0.0),
        0.0,
    )
}
fn on_data_changed(receiver: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
    #[cfg(test)]
    {
        let data = data(receiver, level)?;
        let entry = format!(
            "{},{},{}",
            data.travel_time_in_ticks,
            data.current_vibration.is_some(),
            data.selection_strategy.current_vibration_data.is_some()
        );
        crate::level::game_event::test_support::record_vibration_change(level, entry);
    }
    receiver.set_changed(level)
}
/// Port of VibrationSystem.Ticker#tick. Every callback reads/writes the original
/// receiver generation through short borrows, including detached receivers.
pub(crate) fn tick(receiver: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
    if data(receiver, level)?.current_vibration.is_none() {
        let chosen = data(receiver, level)?
            .selection_strategy
            .current_vibration_data
            .as_ref()
            .filter(|candidate| candidate.tick < level.game_time())
            .map(|candidate| candidate.event.clone());
        if let Some(info) = chosen {
            let (origin, delay) = (info.pos, travel_time(info.distance));
            if let Some(source) = info.entity {
                level.retain_entity(source)?;
            }
            let data = data_mut(receiver, level)?;
            data.current_vibration = Some(info);
            data.travel_time_in_ticks = delay;
            particle(receiver, origin, delay, level)?;
            on_data_changed(receiver, level)?;
            let previous = data_mut(receiver, level)?
                .selection_strategy
                .current_vibration_data
                .take();
            if let Some(source) = previous.and_then(|candidate| candidate.event.entity) {
                level.release_entity(source);
            }
        }
    }
    if data(receiver, level)?.current_vibration.is_some() {
        let mut changed = data(receiver, level)?.travel_time_in_ticks > 0;
        if data(receiver, level)?.reload_vibration_particle {
            let info = data(receiver, level)?.current_vibration.as_ref().unwrap();
            let origin = info.pos;
            let delay = data(receiver, level)?.travel_time_in_ticks;
            let alpha = 1.0 - f64::from(delay) / f64::from(travel_time(info.distance));
            let dest = Vec3::at_center_of(receiver.pos.into());
            let at = Vec3::new(
                origin.x + alpha * (dest.x - origin.x),
                origin.y + alpha * (dest.y - origin.y),
                origin.z + alpha * (dest.z - origin.z),
            );
            if particle(receiver, at, delay, level)? > 0 {
                data_mut(receiver, level)?.reload_vibration_particle = false;
            }
        }
        let data = data_mut(receiver, level)?;
        data.travel_time_in_ticks = data.travel_time_in_ticks.wrapping_sub(1).max(0);
        if data.travel_time_in_ticks <= 0 {
            changed = receive(receiver, level)?;
        }
        if changed {
            on_data_changed(receiver, level)?;
        }
    }
    Ok(())
}
fn receive(receiver: BlockEntityRef, level: &mut Level) -> Result<bool, LevelError> {
    let center = ChunkPos::containing(receiver.pos);
    for x in center.x - 1..=center.x + 1 {
        for z in center.z - 1..=center.z + 1 {
            let pos = ChunkPos::new(x, z);
            if !level.should_tick_blocks_at_chunk(pos) || level.get_chunk_now(pos)?.is_none() {
                return Ok(false);
            }
        }
    }
    let info = data(receiver, level)?
        .current_vibration
        .as_ref()
        .unwrap()
        .clone();
    let source = info
        .entity
        .or_else(|| info.uuid.and_then(|uuid| level.entity_by_uuid(uuid)));
    // VibrationInfo#getProjectileOwner prefers the live projectile owner over
    // the UUID captured when the vibration was scheduled.
    let projectile_owner = if let Some(source) = source
        && level.entity_state(source)?.as_projectile().is_some()
    {
        level.entity_behavior(source)?.get_owner(source, level)?
    } else {
        None
    }
    .or_else(|| {
        info.projectile_owner_uuid
            .and_then(|uuid| level.entity_by_uuid(uuid))
    });
    let retained = [source, projectile_owner];
    for (count, entity) in retained.into_iter().flatten().enumerate() {
        if let Err(error) = level.retain_entity(entity) {
            for entity in retained.into_iter().flatten().take(count) {
                level.release_entity(entity);
            }
            return Err(error);
        }
    }
    let result: Result<(), LevelError> = (|| {
        if is_shrieker(receiver, level)? {
            super::shrieker::try_shriek(
                receiver,
                try_get_player(projectile_owner.or(source), level)?,
                level,
            )?;
        } else {
            let origin = BlockPos::containing(info.pos.x, info.pos.y, info.pos.z);
            let dx = f64::from(origin.x) - f64::from(receiver.pos.x);
            let dy = f64::from(origin.y) - f64::from(receiver.pos.y);
            let dz = f64::from(origin.z) - f64::from(receiver.pos.z);
            super::sensor::receive(
                receiver,
                info.game_event,
                source,
                (dx * dx + dy * dy + dz * dz).sqrt() as f32,
                level,
            )?;
        }

        Ok(())
    })();
    for entity in retained.into_iter().flatten() {
        level.release_entity(entity);
    }
    result?;
    if let Some(source) = data_mut(receiver, level)?
        .current_vibration
        .take()
        .and_then(|info| info.entity)
    {
        level.release_entity(source);
    }

    Ok(true)
}

#[cfg(test)]
mod tests;
