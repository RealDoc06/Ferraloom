//! Entity common ticking and riding, with short canonical borrows around callbacks.
use super::{EntityData, EntityRef, EntityState};
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
    if rider.vehicle.get() == Some(vehicle) || entity == vehicle {
        return Ok(false);
    }
    if !force && (rider.shared_flag(1) || rider.boarding_cooldown > 0) {
        return Ok(false);
    }
    let mount = level.entity::<EntityData>(vehicle)?;
    if !level
        .entity_behavior(vehicle)?
        .could_accept_passenger(vehicle, level)?
        || !force && !mount.passengers.is_empty()
    {
        return Ok(false);
    }
    // Server-side Entity#startRiding refuses nonserializable mounts (players).
    if matches!(level.entity_state(vehicle)?, EntityState::Player(_)) {
        return Ok(false);
    }
    let mut ancestor = vehicle;
    while let Some(parent) = level.entity::<EntityData>(ancestor)?.vehicle.get() {
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
    super::dimensions::set_pose(entity, Pose::Standing, level)?;
    {
        let rider = level.entity_mut::<EntityData>(entity)?;
        rider.vehicle.set(Some(vehicle));
    }
    let passengers = &mut level.entity_mut::<EntityData>(vehicle)?.passengers;
    if player_first {
        passengers.insert(0, entity);
    } else {
        passengers.push(entity);
    }
    Ok(true)
}

/// EntityType#loadPassengersRecursive calls startRiding(force=true,
/// sendEventAndTriggers=false) on a newly constructed root after loading all
/// its descendants. Current factories leave the root unmounted and standing;
/// setPose is therefore a no-op, with no dimensions/world callback to suspend.
pub(crate) fn start_riding_loaded_shared(
    entity: EntityRef,
    vehicle: EntityRef,
    level: &Level,
) -> Result<bool, LevelError> {
    let rider = level.entity::<EntityData>(entity)?;
    assert!(
        rider.vehicle.get().is_none(),
        "fresh loaded root is unmounted"
    );
    assert_eq!(
        *rider.entity_data.get(super::synched_data::POSE),
        Pose::Standing,
        "a factory producing another pose needs shared setPose support"
    );
    if !level
        .entity_behavior(vehicle)?
        .could_accept_passenger(vehicle, level)?
        || matches!(level.entity_state(vehicle)?, EntityState::Player(_))
    {
        return Ok(false);
    }
    // Fresh handles cannot form the ancestor cycle checked by ordinary riding,
    // even when a rejected duplicate UUID names an existing managed entity.
    assert_ne!(entity, vehicle, "new child has a distinct handle");
    assert!(
        !matches!(level.entity_state(entity)?, EntityState::Player(_)),
        "players have no entity NBT factory"
    );
    rider.vehicle.set(Some(vehicle));
    let mount = level.entity::<EntityData>(vehicle)?;
    let mut passengers = mount.passengers.clone();
    passengers.push(entity);
    mount.passengers.replace_shared(passengers);
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
    if let Some(position) = rider_position(vehicle, passenger, level)? {
        level.set_entity_pos(passenger, position)?;
    }
    Ok(())
}

/// Captures the same virtual attachment queries for ordinary riding and snapTo.
pub(crate) fn rider_position(
    vehicle: EntityRef,
    passenger: EntityRef,
    level: &Level,
) -> Result<Option<ferraloom_util::Vec3>, LevelError> {
    if !level
        .entity::<EntityData>(vehicle)?
        .passengers
        .contains(&passenger)
    {
        return Ok(None);
    }
    let behavior = level.entity_behavior(vehicle)?;
    let position = behavior.passenger_riding_position(vehicle, passenger, level)?;
    let rider_behavior = level.entity_behavior(passenger)?;
    let attachment = rider_behavior.vehicle_attachment_point(passenger, vehicle, level)?;
    Ok(Some(position - attachment))
}

/// Port of Entity#rideTick. Tick may replace or remove the vehicle; position
/// against the live vehicle after returning from the derived dispatcher.
pub fn ride_tick<E: super::Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    ride_tick_with_tick_rate(
        behavior,
        entity,
        level,
        &crate::level::tick_rate::TickRateManager::default(),
    )
}

/// Entity#rideTick with the caller's borrowed entity-phase context.
pub fn ride_tick_with_tick_rate<E: super::Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
    tick_rate: &crate::level::tick_rate::TickRateManager,
) -> Result<(), LevelError> {
    level.entity_mut::<EntityData>(entity)?.velocity = ferraloom_util::Vec3::ZERO;
    behavior.tick_with_tick_rate(entity, level, tick_rate)?;
    if let Some(vehicle) = level.entity::<EntityData>(entity)?.vehicle.get() {
        position_rider(vehicle, entity, level)?;
    }
    Ok(())
}

/// Port of LivingEntity#rideTick; `behavior` remains the concrete subclass for
/// Entity#rideTick's virtual tick call, including when the rider dismounts.
pub fn living_ride_tick<E: super::Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    living_ride_tick_with_tick_rate(
        behavior,
        entity,
        level,
        &crate::level::tick_rate::TickRateManager::default(),
    )
}

/// LivingEntity#rideTick keeps the same context through the virtual tick call.
pub fn living_ride_tick_with_tick_rate<E: super::Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
    tick_rate: &crate::level::tick_rate::TickRateManager,
) -> Result<(), LevelError> {
    ride_tick_with_tick_rate(behavior, entity, level, tick_rate)?;
    behavior.reset_fall_distance(entity, level)
}

/// Port of Entity#baseTick. Each borrow ends before fluid, damage or removal calls.
pub fn base_tick<E: super::Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    {
        let base = level.entity_mut::<EntityData>(entity)?;
        let previous = base.last_known_position.unwrap_or(base.position);
        base.last_known_speed = base.position - previous;
        base.last_known_position = Some(base.position);
        base.in_block_state.set(None);
    }
    let vehicle = level.entity::<EntityData>(entity)?.vehicle.get();
    if let Some(vehicle) = vehicle
        && level.entity::<EntityData>(vehicle)?.removed
    {
        stop_riding(entity, level)?;
    }
    {
        let base = level.entity_mut::<EntityData>(entity)?;
        if base.boarding_cooldown > 0 {
            base.boarding_cooldown -= 1;
        }
        if base.portal_cooldown > 0 {
            base.portal_cooldown -= 1;
        }
    }
    super::sounds::maybe_spawn_sprint_particle(behavior, entity, level)?;
    let was_eye = level
        .entity::<EntityData>(entity)?
        .fluid_interaction
        .eyes_inside(ferraloom_data::tags::FluidTags::WATER, level)?;
    {
        let base = level.entity_mut::<EntityData>(entity)?;
        base.was_in_powder_snow = base.is_in_powder_snow;
        base.is_in_powder_snow = false;
        base.was_eye_in_water = was_eye;
    }
    super::fluid::update_fluid_interaction(behavior, entity, level)?;
    update_swimming(behavior, entity, level)?;
    let base = level.entity::<EntityData>(entity)?;
    if base.remaining_fire_ticks > 0 {
        if base.entity_type.data().fire_immune {
            level.entity_mut::<EntityData>(entity)?.clear_fire();
        } else {
            if base.remaining_fire_ticks % 20 == 0 && !base.is_in_lava(level)? {
                let source = level.damage_sources().on_fire();
                behavior.hurt_server(entity, level, source, 1.0)?;
            }
            level.entity_mut::<EntityData>(entity)?.remaining_fire_ticks -= 1;
        }
    }
    if level.entity::<EntityData>(entity)?.is_in_lava(level)? {
        level.entity_mut::<EntityData>(entity)?.fall_distance *= 0.5;
    }
    if level.entity::<EntityData>(entity)?.position.y < f64::from(level.min_build_height() - 64) {
        level.remove_entity(entity)?;
    }
    let base = level.entity_mut::<EntityData>(entity)?;
    base.set_shared_flag(0, base.remaining_fire_ticks > 0 || base.has_visual_fire);
    base.first_tick = false;
    Ok(())
}
fn update_swimming<E: super::Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    let sprinting = base.shared_flag(3);
    let water = base.was_touching_water;
    let passenger = base.vehicle.get().is_some();
    let swimming = if behavior.is_swimming(entity, level)? {
        sprinting && water && !passenger
    } else {
        sprinting
            && base.was_eye_in_water
            && water
            && !passenger
            && level.fluid_tags()?.contains_key(
                ferraloom_data::tags::FluidTags::WATER,
                level
                    .get_block_state(base.block_position)?
                    .data()
                    .fluid
                    .fluid_type,
            )
    };
    level
        .entity_mut::<EntityData>(entity)?
        .set_shared_flag(4, swimming);
    Ok(())
}

/// Port of Entity#countPlayerPassengers; the root itself is excluded.
pub fn count_player_passengers(entity: EntityRef, level: &Level) -> Result<i32, LevelError> {
    let mut count = 0_i32;
    for passenger in &level.entity::<EntityData>(entity)?.passengers {
        if matches!(level.entity_state(*passenger)?, EntityState::Player(_)) {
            count = count.wrapping_add(1);
        }
        count = count.wrapping_add(count_player_passengers(*passenger, level)?);
    }
    Ok(count)
}
