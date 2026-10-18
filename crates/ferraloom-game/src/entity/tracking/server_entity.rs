//! Port of ServerEntity for the implemented nonliving entity families.
use super::codec::{UpdateInterval, VecDeltaCodec, encoding_precision_loss};
use crate::{
    entity::{EntityData, EntityRef, EntityState},
    level::{Level, LevelError},
};
use ferraloom_protocol::{
    codec::{Angle, VarInt},
    entity_data::{DataValue, EntityDataItems},
    play::{
        self, PositionPath, VecDelta,
        clientbound::{self as packet, Packet},
    },
};
use ferraloom_util::{Vec3, mth};

#[derive(Debug)]
pub enum TrackingError {
    Level(LevelError),
    /// Living metadata, head rotation and AttributeMap must be ported before
    /// enabling a living entity's pairing/update stream.
    LivingSynchronizationPending(EntityRef),
}
impl From<LevelError> for TrackingError {
    fn from(error: LevelError) -> Self {
        Self::Level(error)
    }
}
impl std::fmt::Display for TrackingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for TrackingError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrackingAudience {
    Players,
    PlayersAndSelf,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TrackingPacket {
    pub audience: TrackingAudience,
    pub packet: Packet,
}
impl TrackingPacket {
    fn players(packet: Packet) -> Self {
        Self {
            audience: TrackingAudience::Players,
            packet,
        }
    }
    fn with_self(packet: Packet) -> Self {
        Self {
            audience: TrackingAudience::PlayersAndSelf,
            packet,
        }
    }
}

/// Tracking keeps handles and prior wire state. Simulation data remains solely
/// in Level's canonical EntityState; encoding runs after scoped borrows end.
pub struct ServerEntity {
    pub entity: EntityRef,
    update_interval: UpdateInterval,
    track_delta: bool,
    position_codec: VecDeltaCodec,
    last_sent_y_rot: i8,
    last_sent_x_rot: i8,
    last_sent_movement: Vec3,
    tick_count: i32,
    teleport_delay: i32,
    last_passengers: Vec<EntityRef>,
    was_riding: bool,
    was_on_ground: bool,
    tracked_data_values: Option<Vec<DataValue>>,
}
impl ServerEntity {
    pub fn new(
        entity: EntityRef,
        level: &Level,
        update_interval: UpdateInterval,
        track_delta: bool,
    ) -> Result<Self, TrackingError> {
        let state = level.entity_state(entity)?;
        if state.as_living().is_some() {
            return Err(TrackingError::LivingSynchronizationPending(entity));
        }
        let base = state.as_entity();
        Ok(Self {
            entity,
            update_interval,
            track_delta,
            position_codec: VecDeltaCodec {
                base: base.position,
            },
            last_sent_y_rot: mth::pack_degrees(base.yaw),
            last_sent_x_rot: mth::pack_degrees(base.pitch),
            last_sent_movement: base.velocity,
            tick_count: 0,
            teleport_delay: 0,
            last_passengers: Vec::new(),
            was_riding: false,
            was_on_ground: base.on_ground,
            tracked_data_values: base.entity_data.non_default_values(),
        })
    }
    pub fn position_base(&self) -> Vec3 {
        self.position_codec.base
    }
    pub fn last_sent_movement(&self) -> Vec3 {
        self.last_sent_movement
    }
    /// Native pairing order is spawn, cached non-default metadata, passengers,
    /// then the mounted vehicle's passengers. The runtime wraps this in a bundle.
    pub fn pairing_data(&self, level: &mut Level) -> Result<Vec<Packet>, TrackingError> {
        level
            .entity_behavior(self.entity)?
            .update_data_before_sync(self.entity, level)?;
        let state = level.entity_state(self.entity)?;
        let base = state.as_entity();
        let position = self.position_codec.base;
        let data = match state {
            EntityState::FallingBlock(block) => i32::from(block.block_state.id()),
            _ => 0,
        };
        let mut packets = vec![Packet::AddEntity(packet::AddEntity {
            id: VarInt(base.network_id),
            uuid: base.uuid,
            entity_type: play::EntityTypeId(i32::from(state.entity_type().protocol_id())),
            x: position.x,
            y: position.y,
            z: position.z,
            movement: play::LpVec3(self.last_sent_movement),
            x_rot: Angle(self.last_sent_x_rot as u8),
            y_rot: Angle(self.last_sent_y_rot as u8),
            y_head_rot: Angle(0),
            data: VarInt(data),
        })];
        if let Some(data) = &self.tracked_data_values {
            packets.push(Packet::SetEntityData(packet::SetEntityData {
                id: VarInt(base.network_id),
                packed_items: EntityDataItems(data.clone()),
            }));
        }
        if !base.passengers.is_empty() {
            packets.push(passengers_packet(self.entity, level)?);
        }
        if let Some(vehicle) = base.vehicle {
            packets.push(passengers_packet(vehicle, level)?);
        }
        Ok(packets)
    }
    pub fn removal_packet(&self, level: &Level) -> Result<Packet, TrackingError> {
        Ok(Packet::RemoveEntities(packet::RemoveEntities {
            entity_ids: vec![VarInt(level.entity::<EntityData>(self.entity)?.network_id)],
        }))
    }
    fn send_dirty_entity_data(
        &mut self,
        level: &mut Level,
        out: &mut Vec<TrackingPacket>,
    ) -> Result<(), TrackingError> {
        let base = level.entity_mut::<EntityData>(self.entity)?;
        if let Some(data) = base.entity_data.pack_dirty() {
            self.tracked_data_values = base.entity_data.non_default_values();
            out.push(TrackingPacket::with_self(Packet::SetEntityData(
                packet::SetEntityData {
                    id: VarInt(base.network_id),
                    packed_items: EntityDataItems(data),
                },
            )));
        }
        Ok(())
    }
    pub fn send_changes(
        &mut self,
        level: &mut Level,
    ) -> Result<Vec<TrackingPacket>, TrackingError> {
        level
            .entity_behavior(self.entity)?
            .update_data_before_sync(self.entity, level)?;
        let mut out = Vec::new();
        let base = level.entity::<EntityData>(self.entity)?;
        if base.passengers != self.last_passengers {
            out.push(TrackingPacket::players(passengers_packet(
                self.entity,
                level,
            )?));
            self.last_passengers = base.passengers.clone();
        }
        if base.sync_position {
            self.tick_count = self.update_interval.next_interval(self.tick_count);
            level.entity_mut::<EntityData>(self.entity)?.sync_position = false;
        }
        let base = level.entity::<EntityData>(self.entity)?;
        if base.needs_sync
            || self.update_interval.test(self.tick_count)
            || base.entity_data.is_dirty()
        {
            let (y_rot, x_rot) = (mth::pack_degrees(base.yaw), mth::pack_degrees(base.pitch));
            let rotation_changed = (i32::from(y_rot) - i32::from(self.last_sent_y_rot)).abs() >= 1
                || (i32::from(x_rot) - i32::from(self.last_sent_x_rot)).abs() >= 1;
            let position = base.position;
            if base.vehicle.is_some() {
                if rotation_changed {
                    out.push(TrackingPacket::players(Packet::MoveEntityRot(
                        packet::MoveEntityRot {
                            entity_id: VarInt(base.network_id),
                            on_ground: base.on_ground,
                            y_rot: Angle(y_rot as u8),
                            x_rot: Angle(x_rot as u8),
                        },
                    )));
                    self.last_sent_y_rot = y_rot;
                    self.last_sent_x_rot = x_rot;
                }
                self.position_codec.base = position;
                self.send_dirty_entity_data(level, &mut out)?;
                self.was_riding = true;
            } else {
                self.teleport_delay = self.teleport_delay.wrapping_add(1);
                let send_position = self.position_codec.delta(position).length_sqr()
                    >= 7.62939453125E-6
                    || self.tick_count % 60 == 0;
                let (movement_packet, has_position, has_rotation) =
                    self.create_move_packet(level, send_position, rotation_changed, y_rot, x_rot)?;
                let base = level.entity::<EntityData>(self.entity)?;
                if base.needs_sync || self.track_delta {
                    let movement = base.velocity;
                    let diff = movement.distance_to_sqr(self.last_sent_movement);
                    if diff > 1.0E-7 || diff > 0.0 && movement.length_sqr() == 0.0 {
                        self.last_sent_movement = movement;
                        out.push(TrackingPacket::players(Packet::SetEntityMotion(
                            packet::SetEntityMotion {
                                id: VarInt(base.network_id),
                                movement: play::LpVec3(movement),
                            },
                        )));
                    }
                }
                if let Some(packet) = movement_packet {
                    out.push(TrackingPacket::players(packet));
                    self.send_dirty_entity_data(level, &mut out)?;
                    if has_position {
                        self.position_codec.base = position;
                    }
                    if has_rotation {
                        self.last_sent_y_rot = y_rot;
                        self.last_sent_x_rot = x_rot;
                    }
                } else {
                    self.send_dirty_entity_data(level, &mut out)?;
                }
                self.was_riding = false;
            }
            // Entity#getYHeadRot is exactly zero for the supported nonliving
            // families, so this base dispatcher has no head-rotation packet.
            level.entity_mut::<EntityData>(self.entity)?.needs_sync = false;
        }
        self.tick_count = self.tick_count.wrapping_add(1);
        let base = level.entity_mut::<EntityData>(self.entity)?;
        if base.sync_velocity {
            base.sync_velocity = false;
            out.push(TrackingPacket::with_self(Packet::SetEntityMotion(
                packet::SetEntityMotion {
                    id: VarInt(base.network_id),
                    movement: play::LpVec3(base.velocity),
                },
            )));
        }
        Ok(out)
    }
    fn create_move_packet(
        &mut self,
        level: &Level,
        send_position: bool,
        send_rotation: bool,
        y_rot: i8,
        x_rot: i8,
    ) -> Result<(Option<Packet>, bool, bool), TrackingError> {
        let state = level.entity_state(self.entity)?;
        let base = state.as_entity();
        let absolute = |base: &EntityData| {
            Packet::EntityPositionSync(packet::EntityPositionSync {
                id: VarInt(base.network_id),
                position: PositionPath::Linear(base.position),
                y_rot: base.yaw,
                x_rot: base.pitch,
                on_ground: base.on_ground,
            })
        };
        if base.requires_precise_position
            || self.teleport_delay > 400
            || self.was_riding
            || self.was_on_ground != base.on_ground
        {
            self.was_on_ground = base.on_ground;
            self.teleport_delay = 0;
            return Ok((Some(absolute(base)), true, true));
        }
        if send_position {
            let Some(delta) = self.position_codec.try_encode(base.position) else {
                return Ok((Some(absolute(base)), true, true));
            };
            if matches!(state, EntityState::Item(_)) && item_requires_full_precision(base, &delta) {
                return Ok((Some(absolute(base)), true, true));
            }
            let delta = play::EntityDelta {
                on_ground: base.on_ground,
                delta,
            };
            let packet = if send_rotation {
                Packet::MoveEntityPosRot(packet::MoveEntityPosRot {
                    entity_id: VarInt(base.network_id),
                    delta,
                    y_rot: Angle(y_rot as u8),
                    x_rot: Angle(x_rot as u8),
                })
            } else {
                Packet::MoveEntityPos(packet::MoveEntityPos {
                    entity_id: VarInt(base.network_id),
                    delta,
                })
            };
            return Ok((Some(packet), true, send_rotation));
        }
        Ok((
            send_rotation.then_some(Packet::MoveEntityRot(packet::MoveEntityRot {
                entity_id: VarInt(base.network_id),
                on_ground: base.on_ground,
                y_rot: Angle(y_rot as u8),
                x_rot: Angle(x_rot as u8),
            })),
            false,
            send_rotation,
        ))
    }
}
fn passengers_packet(entity: EntityRef, level: &Level) -> Result<Packet, LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    let passengers = base
        .passengers
        .iter()
        .map(|&id| {
            level
                .entity::<EntityData>(id)
                .map(|base| VarInt(base.network_id))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Packet::SetPassengers(packet::SetPassengers {
        vehicle: VarInt(base.network_id),
        passengers,
    }))
}
fn item_requires_full_precision(base: &EntityData, delta: &VecDelta) -> bool {
    let (has_x, has_z) = match delta {
        VecDelta::Linear { xa, za, .. } => (*xa != 0, *za != 0),
        VecDelta::Stepped(steps) => (
            steps.iter().any(|step| step.xa != 0),
            steps.iter().any(|step| step.za != 0),
        ),
    };
    let x_loss = encoding_precision_loss(base.position.x) != 0.0;
    let z_loss = encoding_precision_loss(base.position.z) != 0.0;
    base.vertical_collision && (has_x && x_loss || has_z && z_loss)
        || base.horizontal_collision && (has_x && z_loss || has_z && x_loss)
}

#[cfg(test)]
mod tests;
