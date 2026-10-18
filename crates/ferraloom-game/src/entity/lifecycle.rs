//! Entity common ticking and riding, with short canonical borrows around callbacks.
use super::{EntityData, EntityRef, EntityState, synched_data};
use crate::level::{Level, LevelError};
use ferraloom_protocol::entity_data::Pose;

/// Port of Entity#commonTick. ServerPlayer#doTick remains a connection phase.
pub fn common_tick(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let base = level.entity_mut::<EntityData>(entity)?;
    if base.invulnerable_time > 0 {
        base.invulnerable_time -= 1;
    }
    base.set_old_pos_and_rot();
    base.tick_count = base.tick_count.wrapping_add(1);
    Ok(())
}

/// Port of Entity#startRiding's base restrictions and Player-first passenger order.
pub fn start_riding(
    entity: EntityRef,
    vehicle: EntityRef,
    force: bool,
    level: &mut Level,
) -> Result<bool, LevelError> {
    let rider = level.entity::<EntityData>(entity)?;
    if rider.vehicle == Some(vehicle) || entity == vehicle {
        return Ok(false);
    }
    if !force && (rider.shared_flag(1) || rider.boarding_cooldown > 0) {
        return Ok(false);
    }
    let mount = level.entity::<EntityData>(vehicle)?;
    if mount.removed || !force && !mount.passengers.is_empty() {
        return Ok(false);
    }
    // Server-side Entity#startRiding refuses nonserializable mounts (players).
    if matches!(level.entity_state(vehicle)?, EntityState::Player(_)) {
        return Ok(false);
    }
    let mut ancestor = vehicle;
    while let Some(parent) = level.entity::<EntityData>(ancestor)?.vehicle {
        if parent == entity {
            return Ok(false);
        }
        ancestor = parent;
    }
    stop_riding(entity, level)?;
    let player = matches!(level.entity_state(entity)?, EntityState::Player(_));
    let first = level
        .entity::<EntityData>(vehicle)?
        .passengers
        .first()
        .copied();
    let player_first = player
        && first.is_some_and(|id| !matches!(level.entity_state(id), Ok(EntityState::Player(_))));
    {
        let rider = level.entity_mut::<EntityData>(entity)?;
        rider.entity_data.set(synched_data::POSE, Pose::Standing);
        rider.vehicle = Some(vehicle);
    }
    let passengers = &mut level.entity_mut::<EntityData>(vehicle)?.passengers;
    if player_first {
        passengers.insert(0, entity);
    } else {
        passengers.push(entity);
    }
    Ok(true)
}
pub fn stop_riding(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let Some(vehicle) = level.entity_mut::<EntityData>(entity)?.vehicle.take() else {
        return Ok(());
    };
    level
        .entity_mut::<EntityData>(vehicle)?
        .passengers
        .retain(|id| *id != entity);
    // Player#removeVehicle explicitly clears Entity's normal boarding cooldown.
    let cooldown = if matches!(level.entity_state(entity)?, EntityState::Player(_)) {
        0
    } else {
        60
    };
    level.entity_mut::<EntityData>(entity)?.boarding_cooldown = cooldown;
    Ok(())
}
pub fn eject_passengers(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let passengers = level.entity::<EntityData>(entity)?.passengers.clone();
    for passenger in passengers.into_iter().rev() {
        stop_riding(passenger, level)?;
    }
    Ok(())
}
/// Port of Entity#positionRider using its default attachment positions. Overrides
/// supply species-specific attachment offsets through the stateless dispatcher.
pub fn position_rider(
    vehicle: EntityRef,
    passenger: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    if !level
        .entity::<EntityData>(vehicle)?
        .passengers
        .contains(&passenger)
    {
        return Ok(());
    }
    let behavior = level.entity_behavior(vehicle)?;
    let position = behavior.passenger_riding_position(vehicle, passenger, level)?;
    let rider_behavior = level.entity_behavior(passenger)?;
    let attachment = rider_behavior.vehicle_attachment_point(passenger, vehicle, level)?;
    level.set_entity_pos(passenger, position - attachment)
}
